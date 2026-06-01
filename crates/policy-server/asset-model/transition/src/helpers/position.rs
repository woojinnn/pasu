//! Position helpers — upsert / remove / mutate `Position` entries.
//!
//! All helpers operate on the *effective state* (`state` overlaid with the
//! changes already accumulated in `delta`). They never mutate `state` directly
//! — they only push `PositionChange` entries onto `delta.position_changes`.
//! `PositionPatch.fields` is `serde_json::Value`. Computing a structural diff
//! between the old and new `Position` would require either a generic JSON
//! differ or a per-`PositionKind` field-walker, both of which are out of scope
//! `Position`:
//! ```text
//! { "after": <serde_json::to_value(new_position)> }
//! ```
//! `apply_delta` can later replace `state.positions[i]` with the snapshot
//! verbatim. A true field-diff optimisation is deferred to a future change.
//! ### Effective-state lookup
//! A position becomes part of the effective state as soon as a
//! `PositionChange::Open` is pushed onto `delta`, even before any mutation
//! lands in `state.positions`. `upsert_*` and `close_position` therefore
//! consider both:
//!   - `state.positions` (already-committed positions)
//!   - the pending `PositionChange::Open { position }` entries on `delta`
//!
//! Similarly, an already-pushed `PositionChange::Close { id }` on `delta`
//! removes the position from effective state, even if it still appears in
//! `state.positions`.

use policy_state::delta::PositionPatch;
use policy_state::position::{Position, PositionId, PositionKind};
use policy_state::PositionChange;
use policy_state::{StateDelta, WalletState};
use serde_json::json;

use crate::error::{ReducerError, ReducerResult};

/// Returns `true` if a position with `id` is already part of the effective
/// state (either committed in `state` or queued as `PositionChange::Open` in
/// `delta`) and not yet closed by `delta`.
fn effective_exists(state: &WalletState, delta: &StateDelta, id: &PositionId) -> bool {
    let in_state = state.positions.iter().any(|p| &p.id == id);
    let mut exists = in_state;

    for change in &delta.position_changes {
        match change {
            PositionChange::Open { position } if &position.id == id => exists = true,
            PositionChange::Close { id: closed_id } if closed_id == id => exists = false,
            _ => {}
        }
    }

    exists
}

/// Returns the effective `Position` for `id` — the most recent representation
/// considering both `state.positions` and any `PositionChange::Open` already
/// queued on `delta`. Returns `None` once a `Close` has been queued.
fn effective_position<'a>(
    state: &'a WalletState,
    delta: &'a StateDelta,
    id: &PositionId,
) -> Option<&'a Position> {
    // Search the delta in reverse so the most recent Open wins over an older
    // committed copy. A Close short-circuits to None.
    for change in delta.position_changes.iter().rev() {
        match change {
            PositionChange::Open { position } if &position.id == id => return Some(position),
            PositionChange::Close { id: closed_id } if closed_id == id => return None,
            _ => {}
        }
    }
    state.positions.iter().find(|p| &p.id == id)
}

/// Build a `PositionPatch` from the post-mutation `Position`. The patch stores
/// a full snapshot under the `"after"` key — see the module docs for rationale.
fn snapshot_patch(new_position: &Position) -> PositionPatch {
    let after = serde_json::to_value(new_position).unwrap_or(serde_json::Value::Null);
    PositionPatch {
        fields: json!({ "after": after }),
    }
}

/// Insert a new `Position` and emit `PositionChange::Open`.
/// Errors if a position with the same id is already part of the effective
/// state (already committed or queued in `delta`).
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] if a position with the same id already
/// exists in the effective state.
pub fn open_position(
    state: &WalletState,
    delta: &mut StateDelta,
    position: Position,
) -> ReducerResult<()> {
    if effective_exists(state, delta, &position.id) {
        return Err(ReducerError::Invariant(format!(
            "position {} already exists",
            position.id
        )));
    }
    delta
        .position_changes
        .push(PositionChange::Open { position });
    Ok(())
}

/// Mutate the existing `LendingAccount` position for a venue.
///
/// Looks up the effective position by `position_id`, applies `mutate`, then
/// pushes a `PositionChange::Update` carrying the full post-mutation snapshot.
///
/// # Errors
///
///   - `PositionNotFound` if no position with `position_id` is in the
///     effective state.
///   - `Invariant` if the position exists but is not a `LendingAccount`.
pub fn upsert_lending_account<F>(
    state: &WalletState,
    delta: &mut StateDelta,
    position_id: &PositionId,
    mutate: F,
) -> ReducerResult<()>
where
    F: FnOnce(&mut Position),
{
    let existing = effective_position(state, delta, position_id)
        .ok_or_else(|| ReducerError::PositionNotFound(position_id.clone()))?;

    if !matches!(existing.kind, PositionKind::LendingAccount(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} exists but is not a LendingAccount"
        )));
    }

    let mut new_position = existing.clone();
    mutate(&mut new_position);

    if !matches!(new_position.kind, PositionKind::LendingAccount(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} mutate changed kind away from LendingAccount"
        )));
    }

    let patch = snapshot_patch(&new_position);
    delta.position_changes.push(PositionChange::Update {
        id: position_id.clone(),
        patch,
    });
    Ok(())
}

/// Mutate the existing `PerpPosition` for a venue.
/// Same shape as [`upsert_lending_account`] but for `PerpPosition`.
///
/// # Errors
///
/// Returns [`ReducerError::PositionNotFound`] if the position is absent, or
/// [`ReducerError::Invariant`] if the effective position is not a `PerpPosition`.
pub fn upsert_perp_position<F>(
    state: &WalletState,
    delta: &mut StateDelta,
    position_id: &PositionId,
    mutate: F,
) -> ReducerResult<()>
where
    F: FnOnce(&mut Position),
{
    let existing = effective_position(state, delta, position_id)
        .ok_or_else(|| ReducerError::PositionNotFound(position_id.clone()))?;

    if !matches!(existing.kind, PositionKind::PerpPosition(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} exists but is not a PerpPosition"
        )));
    }

    let mut new_position = existing.clone();
    mutate(&mut new_position);

    if !matches!(new_position.kind, PositionKind::PerpPosition(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} mutate changed kind away from PerpPosition"
        )));
    }

    let patch = snapshot_patch(&new_position);
    delta.position_changes.push(PositionChange::Update {
        id: position_id.clone(),
        patch,
    });
    Ok(())
}

/// Mutate the existing `HyperliquidAccount` position for a wallet.
///
/// Same shape as [`upsert_perp_position`] but for `HlAccount`. Looks up the
/// effective position by `position_id`, applies `mutate`, then pushes a
/// `PositionChange::Update` carrying the full post-mutation snapshot.
///
/// # Errors
///
///   - `PositionNotFound` if no position with `position_id` is effective.
///   - `Invariant` if the position exists but is not a `HyperliquidAccount`.
pub fn upsert_hl_account<F>(
    state: &WalletState,
    delta: &mut StateDelta,
    position_id: &PositionId,
    mutate: F,
) -> ReducerResult<()>
where
    F: FnOnce(&mut Position),
{
    let existing = effective_position(state, delta, position_id)
        .ok_or_else(|| ReducerError::PositionNotFound(position_id.clone()))?;

    if !matches!(existing.kind, PositionKind::HyperliquidAccount(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} exists but is not a HyperliquidAccount"
        )));
    }

    let mut new_position = existing.clone();
    mutate(&mut new_position);

    if !matches!(new_position.kind, PositionKind::HyperliquidAccount(_)) {
        return Err(ReducerError::Invariant(format!(
            "position {position_id} mutate changed kind away from HyperliquidAccount"
        )));
    }

    let patch = snapshot_patch(&new_position);
    delta.position_changes.push(PositionChange::Update {
        id: position_id.clone(),
        patch,
    });
    Ok(())
}

/// Remove a position and emit `PositionChange::Close`.
///
/// Accepts a position that exists in the effective state (i.e. either in
/// `state.positions` or queued as `PositionChange::Open` on `delta`). Errors
/// with `PositionNotFound` otherwise.
///
/// # Errors
///
/// Returns [`ReducerError::PositionNotFound`] if no effective position exists.
pub fn close_position(
    state: &WalletState,
    delta: &mut StateDelta,
    position_id: &PositionId,
) -> ReducerResult<()> {
    if !effective_exists(state, delta, position_id) {
        return Err(ReducerError::PositionNotFound(position_id.clone()));
    }
    delta.position_changes.push(PositionChange::Close {
        id: position_id.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::position::{AirdropClaim, PositionKind};
    use policy_state::position::{ClaimStatus, LendingAccount, MarginMode, PerpPosition, PerpSide};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, Price, ProtocolRef, SignedI256, Time, VenueRef, U256,
    };
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn mainnet() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn user_source() -> DataSource {
        DataSource::UserSupplied
    }

    fn wallet() -> WalletState {
        let addr = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        WalletState::new(WalletId::new(addr, [mainnet()]))
    }

    fn live_dec(v: &str) -> LiveField<Decimal> {
        LiveField::new(Decimal::new(v), user_source(), now())
    }

    fn live_price() -> LiveField<Price> {
        LiveField::new(Price::zero(), user_source(), now())
    }

    fn live_opt_price() -> LiveField<Option<Price>> {
        LiveField::new(None, user_source(), now())
    }

    fn live_signed() -> LiveField<SignedI256> {
        LiveField::new(SignedI256::ZERO, user_source(), now())
    }

    fn lending_position(id: &str) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("aave_v3"),
            chain: Some(mainnet()),
            kind: PositionKind::LendingAccount(LendingAccount {
                market: MarketRef {
                    symbol: "AAVE-MAIN".into(),
                    venue: VenueRef::new("aave_v3"),
                },
                collaterals: vec![],
                debts: vec![],
                emode: None,
                is_isolated: false,
                health_factor: live_dec("1.5"),
                ltv: live_dec("0.7"),
                liquidation_threshold: live_dec("0.8"),
            }),
            primitives_synced_at: now(),
            primitives_source: user_source(),
        }
    }

    fn perp_position(id: &str) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::PerpPosition(PerpPosition {
                venue: VenueRef::new("hyperliquid"),
                market: MarketRef {
                    symbol: "ETH-PERP".into(),
                    venue: VenueRef::new("hyperliquid"),
                },
                side: PerpSide::Long,
                size_base: U256::from(0u64),
                notional_usd: U256::from(0u64),
                collateral: vec![],
                entry_price: Price::zero(),
                margin_mode: MarginMode::Cross,
                mark_price: live_price(),
                liq_price: live_opt_price(),
                unrealized_pnl: live_signed(),
                funding_owed: live_signed(),
                leverage: live_dec("1"),
            }),
            primitives_synced_at: now(),
            primitives_source: user_source(),
        }
    }

    fn airdrop_position(id: &str) -> Position {
        let token = TokenRef::new(TokenKey::Erc20 {
            chain: mainnet(),
            address: Address::from_str("0x0000000000000000000000000000000000000aaa").unwrap(),
        });
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("custom_airdrop"),
            chain: Some(mainnet()),
            kind: PositionKind::AirdropClaim(AirdropClaim {
                source: ProtocolRef::new("custom_airdrop"),
                claimable: token,
                amount: U256::from(0u64),
                proof: None,
                claim_window: None,
                status: ClaimStatus::Claimable,
            }),
            primitives_synced_at: now(),
            primitives_source: user_source(),
        }
    }

    // ===== open_position =====

    #[test]
    fn open_position_pushes_open_change() {
        let state = wallet();
        let mut delta = StateDelta::new();
        let pos = lending_position("p1");

        open_position(&state, &mut delta, pos.clone()).unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Open { position } => assert_eq!(position, &pos),
            other => panic!("expected Open, got {other:?}"),
        }
    }

    #[test]
    fn open_position_rejects_duplicate_in_state() {
        let mut state = wallet();
        let pos = lending_position("p1");
        state.positions.push(pos.clone());
        let mut delta = StateDelta::new();

        let err = open_position(&state, &mut delta, pos).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.position_changes.is_empty());
    }

    #[test]
    fn open_position_rejects_duplicate_in_delta() {
        let state = wallet();
        let mut delta = StateDelta::new();
        let pos = lending_position("p1");

        open_position(&state, &mut delta, pos.clone()).unwrap();
        let err = open_position(&state, &mut delta, pos).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        // Should still only have the first push.
        assert_eq!(delta.position_changes.len(), 1);
    }

    #[test]
    fn open_position_after_close_is_allowed() {
        let mut state = wallet();
        let pos = lending_position("p1");
        state.positions.push(pos.clone());
        let mut delta = StateDelta::new();

        close_position(&state, &mut delta, &pos.id).unwrap();
        open_position(&state, &mut delta, pos).unwrap();
        assert_eq!(delta.position_changes.len(), 2);
    }

    // ===== upsert_lending_account =====

    #[test]
    fn upsert_lending_account_updates_committed_position() {
        let mut state = wallet();
        let pos = lending_position("lend1");
        state.positions.push(pos);
        let mut delta = StateDelta::new();

        upsert_lending_account(&state, &mut delta, &"lend1".to_string(), |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                la.is_isolated = true;
            }
        })
        .unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Update { id, patch } => {
                assert_eq!(id, "lend1");
                let after = patch.fields.get("after").expect("after key present");
                let new_pos: Position = serde_json::from_value(after.clone()).unwrap();
                if let PositionKind::LendingAccount(la) = &new_pos.kind {
                    assert!(la.is_isolated);
                } else {
                    panic!("expected LendingAccount in patch");
                }
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn upsert_lending_account_updates_delta_only_position() {
        let state = wallet();
        let mut delta = StateDelta::new();
        let pos = lending_position("lend1");
        open_position(&state, &mut delta, pos).unwrap();

        upsert_lending_account(&state, &mut delta, &"lend1".to_string(), |p| {
            if let PositionKind::LendingAccount(la) = &mut p.kind {
                la.is_isolated = true;
            }
        })
        .unwrap();

        assert_eq!(delta.position_changes.len(), 2);
        assert!(matches!(
            delta.position_changes[1],
            PositionChange::Update { .. }
        ));
    }

    #[test]
    fn upsert_lending_account_missing_returns_position_not_found() {
        let state = wallet();
        let mut delta = StateDelta::new();

        let err =
            upsert_lending_account(&state, &mut delta, &"ghost".to_string(), |_| {}).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(ref id) if id == "ghost"));
        assert!(delta.position_changes.is_empty());
    }

    #[test]
    fn upsert_lending_account_wrong_kind_returns_invariant() {
        let mut state = wallet();
        state.positions.push(airdrop_position("air1"));
        let mut delta = StateDelta::new();

        let err =
            upsert_lending_account(&state, &mut delta, &"air1".to_string(), |_| {}).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.position_changes.is_empty());
    }

    #[test]
    fn upsert_lending_account_after_close_returns_position_not_found() {
        let mut state = wallet();
        state.positions.push(lending_position("lend1"));
        let mut delta = StateDelta::new();
        close_position(&state, &mut delta, &"lend1".to_string()).unwrap();

        let err =
            upsert_lending_account(&state, &mut delta, &"lend1".to_string(), |_| {}).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    // ===== upsert_perp_position =====

    #[test]
    fn upsert_perp_position_updates_committed_position() {
        let mut state = wallet();
        state.positions.push(perp_position("perp1"));
        let mut delta = StateDelta::new();

        upsert_perp_position(&state, &mut delta, &"perp1".to_string(), |p| {
            if let PositionKind::PerpPosition(pp) = &mut p.kind {
                pp.side = PerpSide::Short;
            }
        })
        .unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Update { id, patch } => {
                assert_eq!(id, "perp1");
                let after = patch.fields.get("after").expect("after key present");
                let new_pos: Position = serde_json::from_value(after.clone()).unwrap();
                if let PositionKind::PerpPosition(pp) = &new_pos.kind {
                    assert!(matches!(pp.side, PerpSide::Short));
                } else {
                    panic!("expected PerpPosition in patch");
                }
            }
            other => panic!("expected Update, got {other:?}"),
        }
    }

    #[test]
    fn upsert_perp_position_missing_returns_position_not_found() {
        let state = wallet();
        let mut delta = StateDelta::new();

        let err =
            upsert_perp_position(&state, &mut delta, &"ghost".to_string(), |_| {}).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
        assert!(delta.position_changes.is_empty());
    }

    #[test]
    fn upsert_perp_position_wrong_kind_returns_invariant() {
        let mut state = wallet();
        state.positions.push(lending_position("lend1"));
        let mut delta = StateDelta::new();

        let err =
            upsert_perp_position(&state, &mut delta, &"lend1".to_string(), |_| {}).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ===== close_position =====

    #[test]
    fn close_position_pushes_close_change_for_committed_position() {
        let mut state = wallet();
        state.positions.push(lending_position("p1"));
        let mut delta = StateDelta::new();

        close_position(&state, &mut delta, &"p1".to_string()).unwrap();
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Close { id } => assert_eq!(id, "p1"),
            other => panic!("expected Close, got {other:?}"),
        }
    }

    #[test]
    fn close_position_can_close_delta_only_position() {
        let state = wallet();
        let mut delta = StateDelta::new();
        open_position(&state, &mut delta, lending_position("p1")).unwrap();

        close_position(&state, &mut delta, &"p1".to_string()).unwrap();
        assert_eq!(delta.position_changes.len(), 2);
        assert!(matches!(
            delta.position_changes[1],
            PositionChange::Close { .. }
        ));
    }

    #[test]
    fn close_position_missing_returns_position_not_found() {
        let state = wallet();
        let mut delta = StateDelta::new();

        let err = close_position(&state, &mut delta, &"ghost".to_string()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(ref id) if id == "ghost"));
        assert!(delta.position_changes.is_empty());
    }

    #[test]
    fn close_position_after_close_returns_position_not_found() {
        let mut state = wallet();
        state.positions.push(lending_position("p1"));
        let mut delta = StateDelta::new();

        close_position(&state, &mut delta, &"p1".to_string()).unwrap();
        let err = close_position(&state, &mut delta, &"p1".to_string()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
        // The first Close is still queued.
        assert_eq!(delta.position_changes.len(), 1);
    }
}
