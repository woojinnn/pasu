//! `stake.*` enrichment-fact namespace — sim-server (state/reducer) facts.
//!
//! Scaffold generated to mirror the current dispatch registry in
//! `crates/simulation/server/src/facts.rs`. The inner [`dispatch`] match is
//! FROZEN: one arm per `sim-server` `stake.*` method from
//! `schema/method-catalog.json` (`planned`), plus a catch-all. Devs fill in the
//! per-method `fn` bodies; they must NOT edit the match.
//!
//! Params arrive as **lowered Cedar** shapes resolved by the extension (not
//! `simulation-state` shapes): `chain_id` is a CAIP-2 string, `owner` a hex
//! address, `action` the lowered action body. Facts read wallet state readonly.

use serde_json::{json, Value};

use policy_state::primitives::ChainId;
use policy_state::token::kind::{TokenKind, UnlockSchedule};

use super::params::{param_action, param_addr, param_chain_id};
use super::FactCtx;
use super::FactError;

/// Dispatch a `stake.*` enrichment fact by method name.
///
/// FROZEN at scaffold time — one arm per sim-server method in this namespace.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when `method` is not a registered
/// `stake.*` fact, or whatever error the per-method fn returns once implemented
/// (currently [`FactError::NotImplemented`]).
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "stake.unlock_eta" => unlock_eta(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// STK-02 `stake.unlock_eta` — for an unstake/withdrawal request, match the
/// target to the wallet's held `StakeReceipt` and report whether a withdrawal
/// lock applies and the seconds until funds become withdrawable.
///
/// readKind: `derived`.
///
/// Params (name: type, required?):
/// - `chain_id`: Long, required — EVM chain id of the unstake target.
/// - `owner`: String, required — wallet whose `StakeReceipt` set is matched.
/// - `action`: Action, required — the unstake action (`Core::Unknown`
///   target/calldata) used to locate the held `StakeReceipt` being exited.
///
/// Outputs (field: type, from-selector):
/// - `hasLockup`: Bool, from `$.result.hasLockup`.
/// - `unlockEtaSecs`: Long, from `$.result.unlockEtaSecs`.
///
/// State to read: the held `StakeReceipt` token holding matching the unstake
/// target, then its `TokenKind::StakeReceipt { unlock, protocol }` — `unlock`
/// is `Option<UnlockSchedule>` (`Cliff { unlock_at } | Linear { start, end } |
/// Cooldown { cooldown_secs }`), evaluated against the current clock to derive
/// the ETA; `protocol` distinguishes provider escrow (Lido 1-5d, `EigenLayer`
/// +7d).
///
/// ## Implementation status: PARTIAL
///
/// `hasLockup` and the `Cooldown` ETA are derived from real state fields.
/// Two limitations are documented inline:
///
/// 1. **Target → receipt matching is best-effort.** The lowered `Core::Unknown`
///    action carries the *staking contract* `target` address; the held receipt
///    is keyed by the *receipt token* address. State exposes no link between a
///    staking contract and its receipt token (`StakeReceipt.protocol` is a
///    name-only `ProtocolRef`, no contract address). We therefore (a) prefer a
///    holding whose own contract address equals `target` (direct-token unstake),
///    else (b) fall back to the unique `StakeReceipt` on the chain. With zero or
///    multiple ambiguous receipts we return `hasLockup=false, eta=0` (no lock
///    asserted) rather than guessing.
/// 2. **`Cliff`/`Linear` absolute ETA needs an evaluation clock absent here.**
///    Those schedules unlock at an absolute `Time`; the seconds-until value is
///    `unlock_at - now` / `end - now`. `FactCtx` carries only `state` — no `now`
///    (see `FactCtx` in `facts/mod.rs`; `EvalContext.now` is not threaded in).
///    For these schedules we report `hasLockup=true` (the lock is real) but
///    `unlockEtaSecs=0`. See the `// BLOCKED:` note below for the exact missing
///    field.
fn unlock_eta(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    // `owner` is validated for shape (a fact precondition) but state is already
    // scoped to this wallet; no per-owner filtering of `state.tokens` is needed.
    let _owner = param_addr(params, "owner")?;
    let action = param_action(params, "action")?;

    let target = action
        .get("target")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<policy_state::primitives::Address>().ok());

    let unlock = locate_stake_unlock(ctx, &chain, target.as_ref());

    let (has_lockup, eta_secs) = match unlock {
        None => (false, 0_i64),
        Some(UnlockSchedule::Cooldown { cooldown_secs }) => {
            // Relative schedule: the cooldown duration IS the ETA from the
            // moment withdrawal is initiated — no wall clock required.
            (true, i64::try_from(*cooldown_secs).unwrap_or(i64::MAX))
        }
        // BLOCKED: FactCtx.now (evaluation clock) — `Cliff.unlock_at` /
        // `Linear.end` are absolute `Time`s; `unlockEtaSecs = absolute - now`
        // is uncomputable without a current-time input on FactCtx. Report the
        // lock as present with a 0 ETA placeholder.
        Some(UnlockSchedule::Cliff { .. } | UnlockSchedule::Linear { .. }) => (true, 0_i64),
    };

    Ok(json!({
        "hasLockup": has_lockup,
        "unlockEtaSecs": eta_secs,
    }))
}

/// Best-effort resolution of the `UnlockSchedule` for the `StakeReceipt` being
/// exited (see `unlock_eta` doc, limitation 1). Returns a reference into
/// `state.tokens`.
///
/// Match strategy on the requested `chain`:
/// 1. If `target` is given and a held `StakeReceipt` token's own contract
///    address equals it, use that holding (direct-token unstake).
/// 2. Otherwise, if exactly one `StakeReceipt` is held on the chain, use it.
/// 3. Otherwise (none, or multiple with no `target` match) return `None` — we
///    do not assert a lockup we cannot attribute.
fn locate_stake_unlock<'a>(
    ctx: &FactCtx<'a>,
    chain: &ChainId,
    target: Option<&policy_state::primitives::Address>,
) -> Option<&'a UnlockSchedule> {
    let mut on_chain = ctx.state.tokens.iter().filter(|(key, holding)| {
        key.chain() == chain && matches!(holding.kind, TokenKind::StakeReceipt { .. })
    });

    if let Some(addr) = target {
        let direct = ctx.state.tokens.iter().find(|(key, holding)| {
            key.chain() == chain
                && key.contract() == Some(addr)
                && matches!(holding.kind, TokenKind::StakeReceipt { .. })
        });
        if let Some((_, holding)) = direct {
            return stake_unlock_of(&holding.kind);
        }
    }

    let first = on_chain.next()?;
    if on_chain.next().is_some() {
        // Ambiguous: multiple StakeReceipts on the chain and no `target` match.
        return None;
    }
    stake_unlock_of(&first.1.kind)
}

/// Borrow the `unlock` schedule out of a `TokenKind::StakeReceipt`, or `None`
/// for any other kind or a receipt with no schedule.
fn stake_unlock_of(kind: &TokenKind) -> Option<&UnlockSchedule> {
    match kind {
        TokenKind::StakeReceipt { unlock, .. } => unlock.as_ref(),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::DataSource;
    use policy_state::primitives::{Address, ProtocolRef, Time, U256};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::TokenKind;
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{WalletId, WalletState};

    const STAKING_TARGET: &str = "0x00000000000000000000000000000000deadbeef";
    const RECEIPT: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn receipt_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str(RECEIPT).unwrap(),
        }
    }

    fn stake_holding(unlock: Option<UnlockSchedule>) -> TokenHolding {
        TokenHolding {
            key: receipt_key(),
            kind: TokenKind::StakeReceipt {
                protocol: ProtocolRef::new("lido"),
                underlying: TokenRef {
                    key: TokenKey::Native { chain: chain() },
                },
                unlock,
                voting_power: None,
            },
            symbol: "stETH".to_owned(),
            decimals: 18,
            balance: Balance::fungible(U256::from(1_000u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn state_with(holding: TokenHolding) -> WalletState {
        let mut state = WalletState::new(wallet_id());
        state.tokens.insert(holding.key.clone(), holding);
        state
    }

    fn params(target: &str) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "action": { "target": target, "calldata": "0xdeadbeef" },
        })
    }

    #[test]
    fn cooldown_eta_is_the_cooldown_secs() {
        let state = state_with(stake_holding(Some(UnlockSchedule::Cooldown {
            cooldown_secs: 604_800,
        })));
        let out = dispatch(
            "stake.unlock_eta",
            &params(STAKING_TARGET),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["hasLockup"], json!(true));
        assert_eq!(out["unlockEtaSecs"], json!(604_800));
    }

    #[test]
    fn cliff_has_lockup_but_eta_blocked_on_clock() {
        let state = state_with(stake_holding(Some(UnlockSchedule::Cliff {
            unlock_at: Time::from_unix(1_800_000_000),
        })));
        let out = dispatch(
            "stake.unlock_eta",
            &params(STAKING_TARGET),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["hasLockup"], json!(true));
        // BLOCKED on FactCtx.now → conservative 0 ETA placeholder.
        assert_eq!(out["unlockEtaSecs"], json!(0));
    }

    #[test]
    fn no_schedule_means_no_lockup() {
        let state = state_with(stake_holding(None));
        let out = dispatch(
            "stake.unlock_eta",
            &params(STAKING_TARGET),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["hasLockup"], json!(false));
        assert_eq!(out["unlockEtaSecs"], json!(0));
    }

    #[test]
    fn direct_token_target_match() {
        // `target` == the held receipt's own address → matched directly even if
        // other StakeReceipts coexist.
        let mut state = state_with(stake_holding(Some(UnlockSchedule::Cooldown {
            cooldown_secs: 100,
        })));
        let other_key = TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str("0x0000000000000000000000000000000000000bad").unwrap(),
        };
        let mut other = stake_holding(Some(UnlockSchedule::Cooldown { cooldown_secs: 999 }));
        other.key = other_key.clone();
        state.tokens.insert(other_key, other);

        let out = dispatch(
            "stake.unlock_eta",
            &params(RECEIPT),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["unlockEtaSecs"], json!(100));
    }

    #[test]
    fn ambiguous_multiple_receipts_no_target_match_is_no_lock() {
        let mut state = state_with(stake_holding(Some(UnlockSchedule::Cooldown {
            cooldown_secs: 100,
        })));
        let other_key = TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str("0x0000000000000000000000000000000000000bad").unwrap(),
        };
        let mut other = stake_holding(Some(UnlockSchedule::Cooldown { cooldown_secs: 999 }));
        other.key = other_key.clone();
        state.tokens.insert(other_key, other);

        // `target` matches neither receipt's own address → ambiguous → no lock.
        let out = dispatch(
            "stake.unlock_eta",
            &params(STAKING_TARGET),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["hasLockup"], json!(false));
        assert_eq!(out["unlockEtaSecs"], json!(0));
    }

    #[test]
    fn missing_action_is_bad_params() {
        let state = state_with(stake_holding(None));
        let err = dispatch(
            "stake.unlock_eta",
            &json!({ "chain_id": chain().to_string(), "owner": "0x000000000000000000000000000000000000a01c" }),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }
}
