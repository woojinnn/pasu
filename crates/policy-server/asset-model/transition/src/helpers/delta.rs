//! Delta utilities — forward-apply a `StateDelta` to a `WalletState`, merge
//! deltas, and check internal consistency.
//!
//! These two primitives are used by `apply_multicall` (children produce
//! independent deltas; each one is applied to advance the running state
//! before the next child sees it; the deltas are merged into a single
//! aggregate `StateDelta` for the caller) and by any code path that wants
//! the post-action `WalletState` rather than just the delta.

use policy_state::delta::{
    ApprovalScope, PendingChange, PositionChange, PositionPatch, TokenChange,
};
use policy_state::pending::{PendingLifecycle, PendingTx};
use policy_state::primitives::{Address, ChainId, SignedI256, Time, U256};
use policy_state::token::{Balance, TokenHolding, TokenKey, TokenKind, TokenRef};
use policy_state::{StateDelta, WalletState};

use crate::error::{ReducerError, ReducerResult};

// ---------------------------------------------------------------------------
// Small numeric helpers (kept private to this module).
// ---------------------------------------------------------------------------

/// Saturating `U256 → I256`.
fn u256_to_signed_saturating(v: U256) -> SignedI256 {
    SignedI256::try_from(v).unwrap_or(SignedI256::MAX)
}

/// Saturating `I256 → U256`. Negative inputs clamp to zero (callers must have
/// already verified non-negativity for hard underflow detection).
fn signed_to_u256_saturating(v: SignedI256) -> U256 {
    U256::try_from(v).unwrap_or(U256::ZERO)
}

// ---------------------------------------------------------------------------
// Internal helpers — apply individual change groups onto a mutable wallet.
// ---------------------------------------------------------------------------

/// Classify the wallet-level approval map that an `ApprovalSet` change targets.
/// `TokenChange::ApprovalSet` does not currently carry an explicit
/// `ApprovalScope` (PDF §8 may add one later); until it does we infer the
/// destination map from the `TokenKey` standard:
/// * `Native` / `Erc20`        → `approvals.erc20`
/// * `Erc721` / `Erc1155`      → `approvals.set_for_all`
///
/// Permit2 entries cannot be distinguished from plain ERC20 with the current
/// shape — `Permit2ApproveAction` must therefore drive `approvals.permit2`
/// through a dedicated helper or wait for a `scope` field on the variant.
const fn approval_scope_for_set(key: &TokenKey) -> ApprovalScope {
    match key {
        TokenKey::Native { .. } | TokenKey::Erc20 { .. } => ApprovalScope::Erc20,
        TokenKey::Erc721 { .. } | TokenKey::Erc1155 { .. } => ApprovalScope::SetForAll,
    }
}

/// Resolve `(chain, contract address)` from a `TokenKey` for the approval map
/// keys. `Native` has no contract and produces `Invariant`.
fn contract_addr_key(key: &TokenKey) -> ReducerResult<(ChainId, Address)> {
    let chain = key.chain().clone();
    let addr = key.contract().copied().ok_or_else(|| {
        ReducerError::Invariant(format!("token key {key:?} has no contract address"))
    })?;
    Ok((chain, addr))
}

/// Append a missing-token holding for `Mint`. Sync orchestrator fills the
/// remaining `LiveField`s later; reducer-side we only need a structurally
/// valid placeholder so that subsequent `BalanceDelta` entries find a `holding`.
fn mint_stub_holding(key: &TokenKey, kind_hint: TokenKind) -> TokenHolding {
    use policy_state::live_field::DataSource;
    let chain = key.chain().clone();
    let contract = key
        .contract()
        .copied()
        .unwrap_or_else(|| Address::from([0u8; 20]));
    TokenHolding {
        key: key.clone(),
        kind: kind_hint,
        symbol: String::new(),
        decimals: 0,
        balance: Balance::zero_fungible(),
        committed: Balance::zero_fungible(),
        approved_to: None,
        price_usd: None,
        metadata: None,
        value_usd: None,
        last_synced_at: Time::from_unix(0),
        primitives_source: DataSource::OnchainView {
            chain,
            contract,
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        },
    }
}

/// Apply a `BalanceDelta` to the matching holding.
fn apply_balance_delta(next: &mut WalletState, key: &TokenKey, d: SignedI256) -> ReducerResult<()> {
    let holding = next
        .tokens
        .get_mut(key)
        .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
    let current = holding.balance.as_fungible().ok_or_else(|| {
        ReducerError::Invariant(format!("BalanceDelta on non-fungible holding {key:?}"))
    })?;
    let signed = u256_to_signed_saturating(current).saturating_add(d);
    if signed.is_negative() {
        return Err(ReducerError::Invariant(format!(
            "balance underflow applying delta to {key:?}: current {current}, delta {d}"
        )));
    }
    holding.balance = Balance::Fungible {
        amount: signed_to_u256_saturating(signed),
    };
    Ok(())
}

/// Apply an `ApprovalSet` to the wallet-level approval map, inferring scope
/// from the `TokenKey` standard (see `approval_scope_for_set`).
fn apply_approval_set(
    next: &mut WalletState,
    key: &TokenKey,
    spender: &Address,
    allowance: &policy_state::approval::AllowanceSpec,
) -> ReducerResult<()> {
    let scope = approval_scope_for_set(key);
    match scope {
        ApprovalScope::Erc20 => {
            let addr_key = contract_addr_key(key)?;
            next.approvals
                .erc20
                .entry(addr_key)
                .or_default()
                .insert(*spender, allowance.clone());
            Ok(())
        }
        ApprovalScope::SetForAll => {
            let addr_key = contract_addr_key(key)?;
            next.approvals
                .set_for_all
                .entry(addr_key)
                .or_default()
                .insert(*spender);
            Ok(())
        }
        // Reachable only if `approval_scope_for_set` grows new return values
        // without a matching arm above; the two below are not produced by
        // today's classifier.
        ApprovalScope::Permit2 | ApprovalScope::Erc721Token => {
            Err(ReducerError::Invariant(format!(
                "ApprovalSet scope {scope:?} cannot be inferred from key {key:?}; \
                 add an explicit scope field on TokenChange::ApprovalSet to route \
                 Permit2 / Erc721Token"
            )))
        }
    }
}

/// Apply an `ApprovalRevoke` to the requested `scope` map.
fn apply_approval_revoke(
    next: &mut WalletState,
    key: &TokenKey,
    spender: &Address,
    scope: &ApprovalScope,
) -> ReducerResult<()> {
    match scope {
        ApprovalScope::Erc20 => {
            let addr_key = contract_addr_key(key)?;
            if let Some(map) = next.approvals.erc20.get_mut(&addr_key) {
                map.remove(spender);
                if map.is_empty() {
                    next.approvals.erc20.remove(&addr_key);
                }
            }
            Ok(())
        }
        ApprovalScope::SetForAll => {
            let addr_key = contract_addr_key(key)?;
            if let Some(set) = next.approvals.set_for_all.get_mut(&addr_key) {
                set.remove(spender);
                if set.is_empty() {
                    next.approvals.set_for_all.remove(&addr_key);
                }
            }
            Ok(())
        }
        ApprovalScope::Permit2 => {
            let (chain, addr) = contract_addr_key(key)?;
            next.approvals.permit2.remove(&(chain, addr, *spender));
            Ok(())
        }
        ApprovalScope::Erc721Token => {
            let holding = next
                .tokens
                .get_mut(key)
                .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
            holding.approved_to = None;
            Ok(())
        }
    }
}

/// Apply one `TokenChange` against `next`.
fn apply_token_change(next: &mut WalletState, tc: &TokenChange) -> ReducerResult<()> {
    match tc {
        TokenChange::BalanceDelta { key, delta: d } => apply_balance_delta(next, key, *d),
        TokenChange::ApprovalSet {
            key,
            spender,
            allowance,
        } => apply_approval_set(next, key, spender, allowance),
        TokenChange::ApprovalRevoke {
            key,
            spender,
            scope,
        } => apply_approval_revoke(next, key, spender, scope),
        TokenChange::Erc721ApprovedTo { key, spender } => {
            let holding = next
                .tokens
                .get_mut(key)
                .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
            holding.approved_to = *spender;
            Ok(())
        }
        TokenChange::Mint { key, kind_hint } => {
            next.tokens
                .entry(key.clone())
                .or_insert_with(|| mint_stub_holding(key, kind_hint.clone()));
            Ok(())
        }
    }
}

/// Merge a `PositionPatch`'s `fields` JSON object into the on-state `Position`.
/// Convention (PDF §8 reserves the patch shape; until the producer side is
/// frozen we accept two forms):
/// 1. `{ "after": <Position> }` — full replacement, deserialised back into a
///    `Position`. Useful for the simplest producers.
/// 2. `{ <field path>: <value>, ... }` — field-level merge via
///    `serde_json::Value` map. Each key replaces the corresponding top-level
///    field on the JSON-serialised position. Unknown keys cause `Invariant`.
fn apply_position_patch(
    target: &mut policy_state::position::Position,
    patch: &PositionPatch,
) -> ReducerResult<()> {
    let fields = &patch.fields;

    // Form 1: explicit "after".
    if let Some(after) = fields.get("after") {
        let new_pos: policy_state::position::Position = serde_json::from_value(after.clone())
            .map_err(|e| {
                ReducerError::Invariant(format!(
                    "PositionPatch.after did not deserialise as Position: {e}"
                ))
            })?;
        *target = new_pos;
        return Ok(());
    }

    // Form 2: field-level merge. Round-trip via serde_json::Value.
    let mut as_json = serde_json::to_value(&*target)
        .map_err(|e| ReducerError::Invariant(format!("Position serialise failed: {e}")))?;
    let target_obj = as_json.as_object_mut().ok_or_else(|| {
        ReducerError::Invariant("Position did not serialise as JSON object".into())
    })?;
    let patch_obj = fields.as_object().ok_or_else(|| {
        ReducerError::Invariant(format!(
            "PositionPatch.fields must be a JSON object, got {fields}"
        ))
    })?;
    for (k, v) in patch_obj {
        target_obj.insert(k.clone(), v.clone());
    }
    *target = serde_json::from_value(as_json)
        .map_err(|e| ReducerError::Invariant(format!("Position re-deserialise failed: {e}")))?;
    Ok(())
}

/// Apply one `PositionChange` against `next`.
fn apply_position_change(next: &mut WalletState, pc: &PositionChange) -> ReducerResult<()> {
    match pc {
        PositionChange::Open { position } => {
            next.positions.push(position.clone());
        }
        PositionChange::Update { id, patch } => {
            let target = next
                .positions
                .iter_mut()
                .find(|p| &p.id == id)
                .ok_or_else(|| ReducerError::PositionNotFound(id.clone()))?;
            apply_position_patch(target, patch)?;
        }
        PositionChange::Close { id } => {
            let len_before = next.positions.len();
            next.positions.retain(|p| &p.id != id);
            if next.positions.len() == len_before {
                return Err(ReducerError::PositionNotFound(id.clone()));
            }
        }
    }
    Ok(())
}

/// Apply one `PendingChange` against `next`.
fn apply_pending_change(next: &mut WalletState, pc: &PendingChange) -> ReducerResult<()> {
    match pc {
        PendingChange::Add { pending } => {
            let p: &PendingTx = pending;
            next.pending.push(p.clone());
        }
        PendingChange::Update {
            id,
            status,
            partial_fill,
        } => {
            let target = next
                .pending
                .iter_mut()
                .find(|p| &p.id == id)
                .ok_or_else(|| {
                    ReducerError::Invariant(format!("pending id {id} not found for update"))
                })?;
            target.lifecycle = PendingLifecycle {
                status: status.clone(),
                ..target.lifecycle.clone()
            };
            // `partial_fill` is accepted for wire compatibility but is not part
            // of `PendingLifecycle`.
            let _ = partial_fill;
        }
        PendingChange::Remove { id, reason: _ } => {
            let len_before = next.pending.len();
            next.pending.retain(|p| &p.id != id);
            if next.pending.len() == len_before {
                return Err(ReducerError::Invariant(format!(
                    "pending id {id} not found for remove"
                )));
            }
        }
    }
    Ok(())
}

/// Charge `(token, amount)` against the wallet's gas-token holding.
fn apply_gas_paid(next: &mut WalletState, token: &TokenRef, amount: U256) -> ReducerResult<()> {
    let key = &token.key;
    let holding = next
        .tokens
        .get_mut(key)
        .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
    let current = holding.balance.as_fungible().ok_or_else(|| {
        ReducerError::Invariant(format!("gas charged against non-fungible holding {key:?}"))
    })?;
    let next_amount = current.checked_sub(amount).ok_or_else(|| {
        ReducerError::Invariant(format!(
            "gas underflow on {key:?}: current {current}, gas {amount}"
        ))
    })?;
    holding.balance = Balance::Fungible {
        amount: next_amount,
    };
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API.
// ---------------------------------------------------------------------------

/// Apply `delta` to `state` and return the resulting new state.
///
/// Inverse direction of `Reducer::apply`: where a reducer produces a delta
/// from `(state, action)`, this consumes a delta to advance `state`. Used by
/// `apply_multicall` to thread `state → state' → state''` across child
/// actions, and by callers that want the post-action snapshot.
///
/// Order of application (matches PDF §8):
///   1. `token_changes` (mint stubs are emitted explicitly via
///      `TokenChange::Mint` *before* any `BalanceDelta` on a new key)
///   2. `position_changes`
///   3. `pending_changes`
///   4. `gas_paid` — last, so any swap producing native gas already credited
///      its receipt before the gas charge debits it
///
/// # Errors
///
/// Returns [`ReducerError`] if any change is invalid for the current state.
pub fn apply_delta(state: &WalletState, delta: &StateDelta) -> ReducerResult<WalletState> {
    let mut next = state.clone();

    for tc in &delta.token_changes {
        apply_token_change(&mut next, tc)?;
    }
    for pc in &delta.position_changes {
        apply_position_change(&mut next, pc)?;
    }
    for pc in &delta.pending_changes {
        apply_pending_change(&mut next, pc)?;
    }
    if let Some((token, amount)) = &delta.gas_paid {
        apply_gas_paid(&mut next, token, *amount)?;
    }

    Ok(next)
}

/// Merge `b` into `a` such that applying the merged delta is equivalent to
/// applying `a` then `b` in sequence.
///
/// The merge is **structural concatenation** today: the resulting delta
/// preserves the relative order of every change record, so `apply_delta`
/// against the merged delta produces the same `WalletState` as applying
/// `a` followed by `b` separately. Coalescing (e.g. combining two
/// `BalanceDelta` entries on the same key, or cancelling `Open(id)` against
/// a later `Close(id)`) is left to a future canonicalisation pass — it's a
/// pure optimisation, not a correctness requirement, and skipping it keeps
/// `merge_delta` total.
///
/// `gas_paid` is the only field that *must* combine: the merged delta can
/// only carry one `(token, amount)` pair. Two pairs on the same token are
/// summed (saturating); pairs on different tokens are rejected with
/// `Invariant` — we don't model multi-token gas today.
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when both deltas contain gas payments for
/// different tokens.
pub fn merge_delta(a: StateDelta, b: StateDelta) -> ReducerResult<StateDelta> {
    let StateDelta {
        mut token_changes,
        mut position_changes,
        mut pending_changes,
        gas_paid: gas_a,
    } = a;
    let StateDelta {
        token_changes: b_token,
        position_changes: b_position,
        pending_changes: b_pending,
        gas_paid: gas_b,
    } = b;

    token_changes.extend(b_token);
    position_changes.extend(b_position);
    pending_changes.extend(b_pending);

    let gas_paid = match (gas_a, gas_b) {
        (None, None) => None,
        (Some(g), None) | (None, Some(g)) => Some(g),
        (Some((tok_a, amt_a)), Some((tok_b, amt_b))) => {
            if tok_a.key != tok_b.key {
                return Err(ReducerError::Invariant(format!(
                    "merge_delta: incompatible gas tokens {:?} vs {:?}",
                    tok_a.key, tok_b.key
                )));
            }
            Some((tok_a, amt_a.saturating_add(amt_b)))
        }
    };

    Ok(StateDelta {
        token_changes,
        position_changes,
        pending_changes,
        gas_paid,
    })
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::approval::AllowanceSpec;
    use policy_state::delta::{PositionChange, PositionPatch, TokenChange};
    use policy_state::live_field::DataSource;
    use policy_state::position::{AirdropClaim, ClaimStatus, Position, PositionKind};
    use policy_state::primitives::{ChainId, ProtocolRef, Time};
    use policy_state::token::{BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKind};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn mainnet_usdc_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        }
    }

    fn make_fungible_holding(key: TokenKey, amount: u128) -> TokenHolding {
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn empty_state() -> WalletState {
        let owner = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        WalletState::new(WalletId::new(owner, [ChainId::ethereum_mainnet()]))
    }

    fn state_with(holding: TokenHolding) -> WalletState {
        let mut s = empty_state();
        s.tokens.insert(holding.key.clone(), holding);
        s
    }

    /// `apply_delta` — a single `BalanceDelta` debits the holding.
    #[test]
    fn apply_delta_balance_debit() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::BalanceDelta {
            key: key.clone(),
            delta: -SignedI256::try_from(300i64).unwrap(),
        });

        let next = apply_delta(&state, &delta).unwrap();

        let h = next.tokens.get(&key).unwrap();
        assert_eq!(h.balance.as_fungible().unwrap(), U256::from(700u64));
        // Source unchanged.
        let h0 = state.tokens.get(&key).unwrap();
        assert_eq!(h0.balance.as_fungible().unwrap(), U256::from(1_000u64));
    }

    /// `apply_delta` — `BalanceDelta` that would underflow is rejected.
    #[test]
    fn apply_delta_balance_underflow_is_invariant() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 100));
        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::BalanceDelta {
            key,
            delta: -SignedI256::try_from(101i64).unwrap(),
        });
        let err = apply_delta(&state, &delta).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `apply_delta` — `Mint` followed by `BalanceDelta` inserts a placeholder
    /// holding
    /// and then credits it.
    #[test]
    fn apply_delta_mint_then_credit() {
        let state = empty_state();
        let new_key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xb0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        };
        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::Mint {
            key: new_key.clone(),
            kind_hint: TokenKind::Unknown,
        });
        delta.token_changes.push(TokenChange::BalanceDelta {
            key: new_key.clone(),
            delta: SignedI256::try_from(42i64).unwrap(),
        });

        let next = apply_delta(&state, &delta).unwrap();
        let h = next.tokens.get(&new_key).unwrap();
        assert_eq!(h.balance.as_fungible().unwrap(), U256::from(42u64));
    }

    /// `apply_delta` — `ApprovalSet` on an ERC20 key updates `approvals.erc20`.
    #[test]
    fn apply_delta_approval_set_erc20() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let spender = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let addr = *key.contract().unwrap();
        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::ApprovalSet {
            key,
            spender,
            allowance: AllowanceSpec::new(U256::from(500u64), Time::from_unix(0)),
        });

        let next = apply_delta(&state, &delta).unwrap();
        let entry = next
            .approvals
            .erc20
            .get(&(ChainId::ethereum_mainnet(), addr))
            .unwrap();
        assert_eq!(entry.get(&spender).unwrap().amount, U256::from(500u64));
    }

    /// `apply_delta` — `ApprovalRevoke { scope: Erc20 }` removes the entry.
    #[test]
    fn apply_delta_approval_revoke_erc20() {
        let key = mainnet_usdc_key();
        let spender = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();

        // Pre-seed the wallet with an existing allowance so revoke has
        // something to remove.
        let mut state = state_with(make_fungible_holding(key.clone(), 1_000));
        let addr_key = (ChainId::ethereum_mainnet(), *key.contract().unwrap());
        state.approvals.erc20.entry(addr_key).or_default().insert(
            spender,
            AllowanceSpec::new(U256::from(100u64), Time::from_unix(0)),
        );

        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::ApprovalRevoke {
            key,
            spender,
            scope: ApprovalScope::Erc20,
        });
        let next = apply_delta(&state, &delta).unwrap();
        assert!(next.approvals.erc20.is_empty());
    }

    fn user_supplied_source() -> DataSource {
        DataSource::UserSupplied
    }

    fn airdrop_claim(amount: u64, status: ClaimStatus) -> AirdropClaim {
        AirdropClaim {
            source: ProtocolRef::new("test"),
            claimable: TokenRef::new(mainnet_usdc_key()),
            amount: U256::from(amount),
            proof: None,
            claim_window: None,
            status,
        }
    }

    /// `apply_delta` — `PositionChange::Open` then `Close` round-trips.
    #[test]
    fn apply_delta_position_open_and_close() {
        let state = empty_state();
        let position = Position {
            id: "p1".into(),
            protocol: ProtocolRef::new("test"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::AirdropClaim(airdrop_claim(1, ClaimStatus::Eligible)),
            primitives_synced_at: Time::from_unix(0),
            primitives_source: user_supplied_source(),
        };

        let mut delta_open = StateDelta::new();
        delta_open
            .position_changes
            .push(PositionChange::Open { position });
        let after_open = apply_delta(&state, &delta_open).unwrap();
        assert_eq!(after_open.positions.len(), 1);
        assert_eq!(after_open.positions[0].id, "p1");

        let mut delta_close = StateDelta::new();
        delta_close
            .position_changes
            .push(PositionChange::Close { id: "p1".into() });
        let after_close = apply_delta(&after_open, &delta_close).unwrap();
        assert!(after_close.positions.is_empty());
    }

    /// `apply_delta` — `PositionChange::Update` with explicit `{ "after": ... }`
    /// patch replaces the position wholesale.
    #[test]
    fn apply_delta_position_update_replace() {
        let mut after_open = empty_state();
        let original = Position {
            id: "p1".into(),
            protocol: ProtocolRef::new("test"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::AirdropClaim(airdrop_claim(1, ClaimStatus::Eligible)),
            primitives_synced_at: Time::from_unix(0),
            primitives_source: user_supplied_source(),
        };
        after_open.positions.push(original.clone());

        let mut updated = original;
        updated.kind = PositionKind::AirdropClaim(airdrop_claim(99, ClaimStatus::Claimed));

        let mut delta = StateDelta::new();
        delta.position_changes.push(PositionChange::Update {
            id: "p1".into(),
            patch: PositionPatch {
                fields: serde_json::json!({ "after": updated }),
            },
        });
        let next = apply_delta(&after_open, &delta).unwrap();
        assert_eq!(next.positions.len(), 1);
        if let PositionKind::AirdropClaim(c) = &next.positions[0].kind {
            assert_eq!(c.amount, U256::from(99u64));
            assert!(matches!(c.status, ClaimStatus::Claimed));
        } else {
            panic!("expected AirdropClaim variant");
        }
    }

    /// `apply_delta` — Closing a missing position id is an error.
    #[test]
    fn apply_delta_close_missing_position_errors() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        delta.position_changes.push(PositionChange::Close {
            id: "missing".into(),
        });
        let err = apply_delta(&state, &delta).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    /// `apply_delta` — `gas_paid` debits the gas token.
    #[test]
    fn apply_delta_gas_paid_debits_token() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();
        delta.gas_paid = Some((TokenRef::new(key.clone()), U256::from(50u64)));

        let next = apply_delta(&state, &delta).unwrap();
        let h = next.tokens.get(&key).unwrap();
        assert_eq!(h.balance.as_fungible().unwrap(), U256::from(950u64));
    }

    /// `apply_delta` — `gas_paid` against an unknown holding errors.
    #[test]
    fn apply_delta_gas_paid_missing_holding_errors() {
        let state = empty_state();
        let key = mainnet_usdc_key();
        let mut delta = StateDelta::new();
        delta.gas_paid = Some((TokenRef::new(key), U256::from(1u64)));
        let err = apply_delta(&state, &delta).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// `merge_delta` — concatenates token / position / pending changes in order.
    #[test]
    fn merge_delta_concatenates_changes() {
        let key = mainnet_usdc_key();
        let mut a = StateDelta::new();
        a.token_changes.push(TokenChange::BalanceDelta {
            key: key.clone(),
            delta: -SignedI256::try_from(100i64).unwrap(),
        });
        let mut b = StateDelta::new();
        b.token_changes.push(TokenChange::BalanceDelta {
            key,
            delta: SignedI256::try_from(40i64).unwrap(),
        });

        let merged = merge_delta(a, b).unwrap();
        assert_eq!(merged.token_changes.len(), 2);
        match &merged.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => {
                assert_eq!(*d, -SignedI256::try_from(100i64).unwrap());
            }
            other => panic!("expected BalanceDelta first, got {other:?}"),
        }
    }

    /// `merge_delta` — gas paid on the same token is summed.
    #[test]
    fn merge_delta_gas_paid_same_token_sums() {
        let key = mainnet_usdc_key();
        let mut a = StateDelta::new();
        a.gas_paid = Some((TokenRef::new(key.clone()), U256::from(10u64)));
        let mut b = StateDelta::new();
        b.gas_paid = Some((TokenRef::new(key.clone()), U256::from(25u64)));

        let merged = merge_delta(a, b).unwrap();
        let (tok, amt) = merged.gas_paid.unwrap();
        assert_eq!(tok.key, key);
        assert_eq!(amt, U256::from(35u64));
    }

    /// `merge_delta` — gas paid on different tokens is rejected.
    #[test]
    fn merge_delta_gas_paid_different_tokens_errors() {
        let key_a = mainnet_usdc_key();
        let key_b = TokenKey::Native {
            chain: ChainId::ethereum_mainnet(),
        };
        let mut a = StateDelta::new();
        a.gas_paid = Some((TokenRef::new(key_a), U256::from(10u64)));
        let mut b = StateDelta::new();
        b.gas_paid = Some((TokenRef::new(key_b), U256::from(25u64)));

        let err = merge_delta(a, b).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `merge_delta` + `apply_delta` — applying `merge_delta(a, b)` is the
    /// same as applying `a` then `b`.
    #[test]
    fn merge_then_apply_equals_apply_then_apply() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));

        let mut a = StateDelta::new();
        a.token_changes.push(TokenChange::BalanceDelta {
            key: key.clone(),
            delta: -SignedI256::try_from(300i64).unwrap(),
        });
        let mut b = StateDelta::new();
        b.token_changes.push(TokenChange::BalanceDelta {
            key: key.clone(),
            delta: SignedI256::try_from(50i64).unwrap(),
        });

        let stepwise = {
            let s1 = apply_delta(&state, &a).unwrap();
            apply_delta(&s1, &b).unwrap()
        };
        let merged = merge_delta(a, b).unwrap();
        let merged_applied = apply_delta(&state, &merged).unwrap();

        assert_eq!(
            stepwise.tokens.get(&key).unwrap().balance,
            merged_applied.tokens.get(&key).unwrap().balance
        );
        assert_eq!(
            stepwise
                .tokens
                .get(&key)
                .unwrap()
                .balance
                .as_fungible()
                .unwrap(),
            U256::from(750u64)
        );
    }
}
