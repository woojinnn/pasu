//! `governance.*` enrichment-fact namespace — sim-server (state/reducer) facts.
//!
//! Scaffold generated to mirror the current dispatch registry in
//! `crates/simulation/server/src/facts.rs`. The inner [`dispatch`] match is
//! COMPLETE and FROZEN: one arm per `sim-server` `governance.*` method from
//! `schema/method-catalog.json` (`planned`), plus the unknown-method catch-all.
//! Devs fill in the per-method `fn` bodies; they must NOT edit the match.
//!
//! Params arrive as **lowered Cedar** shapes resolved by the extension (not
//! `simulation-state` shapes): `chain_id` is a CAIP-2 / EVM chain id, `owner` a
//! hex address, `token` a lowered `Core::TokenRef`, `action` the lowered action
//! body. Facts read wallet state readonly.

use serde_json::{json, Value};

use policy_state::primitives::U256;
use policy_state::token::kind::TokenKind;
use policy_state::token::TokenKey;

use super::params::{over_balance_4dp, param_action, param_asset_contract, param_chain_id};
use super::FactCtx;
use super::FactError;

/// Dispatch a `governance.*` enrichment fact by method name.
///
/// The inner match is COMPLETE and FROZEN: exactly one arm per sim-server method
/// in this namespace plus the unknown-method catch-all. Do not edit it when
/// implementing fact bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when `method` is not a registered
/// `governance.*` fact, [`FactError::NotImplemented`] for a registered but
/// not-yet-implemented method, or [`FactError::BadParams`] when `params` is
/// missing a required field or has the wrong shape.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "governance.voting_power_at_risk" => voting_power_at_risk(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// `governance.voting_power_at_risk` (readKind: `direct`) — GOV-02.
///
/// How much governance voting power this `delegate(address)` call would
/// re-point, expressed as a comparable decimal magnitude. Reads the durable
/// `StakeReceipt.voting_power` for the governance `token` in wallet state (raw
/// U256), scaling it to a decimal; when no durable receipt exists it falls back
/// to the action's live `votingPower` `live_input`. Drives the size half of the
/// re-delegation warn (the `delegatee != currentDelegate` half is tested
/// directly in Cedar over the action String fields).
///
/// ## Params (catalog)
/// - `chain_id`: Long, required — `$.root.chain_id`.
/// - `owner`: String, required — `$.root.from`; wallet whose held governance
///   `StakeReceipt` is matched.
/// - `token`: `AssetRef`, required — `$.action.token`; governance token whose
///   voting power is being delegated.
/// - `action`: Action, required — `$.action`; the Delegate action carrying
///   `delegatee`, `currentDelegate` (live), and the U256 `votingPower`
///   `live_input` used as the state-absent fallback.
///
/// ## Outputs (catalog)
/// - `votingPowerAtRisk`: decimal, from `$.result.votingPowerAtRisk`.
///
/// ## `WalletState` accessors to call
/// - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — locate the
///   governance token's `TokenHolding` by the lowered `token` `TokenKey`.
///
/// The durable signal lives in `TokenHolding.kind`'s
/// `TokenKind::StakeReceipt { voting_power, .. }` — a raw U256. `kind` is a
/// public field (`pub kind: TokenKind`), so it is read by direct pattern match;
/// no dedicated accessor is required (the Ground accessor list only curated
/// helper methods, not a restriction on public-field access). The state-absent
/// fallback reads the action body's lowered `votingPower` (`0x`-hex U256), an
/// action-body field rather than a `WalletState` read.
fn voting_power_at_risk(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let token_contract = param_asset_contract(params, "token")?;
    let action = param_action(params, "action")?;

    // Durable signal: the held governance token's StakeReceipt voting_power
    // (raw U256). The lowered governance `token` is ERC20-shaped, so match it as
    // an Erc20 TokenKey holding.
    let durable = ctx
        .state
        .tokens
        .get(&TokenKey::Erc20 {
            chain,
            address: token_contract,
        })
        .and_then(|h| match &h.kind {
            TokenKind::StakeReceipt { voting_power, .. } => *voting_power,
            _ => None,
        });

    // State-absent fallback: the action body carries `votingPower` (lowered from
    // `live_inputs.voting_power`) as a `0x`-hex U256 string.
    let raw = if let Some(vp) = durable {
        vp
    } else {
        let s = action
            .get("votingPower")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                FactError::BadParams(
                    "no durable StakeReceipt voting_power and action.votingPower absent".into(),
                )
            })?;
        U256::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|e| {
            FactError::BadParams(format!("action.votingPower is not a U256 hex: {e}"))
        })?
    };

    // Render the raw integer voting power as a comparable 4-dp decimal. Both
    // paths emit raw on-chain magnitude (the fallback cannot know `decimals`),
    // keeping durable and fallback in identical units for the Cedar threshold.
    Ok(json!({
        "votingPowerAtRisk": over_balance_4dp(raw, U256::from(1u64)),
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use serde_json::json;

    use policy_state::live_field::DataSource;
    use policy_state::primitives::{Address, ChainId, ProtocolRef, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::TokenKind;
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{WalletId, WalletState};

    const TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn token_addr() -> Address {
        Address::from_str(TOKEN).unwrap()
    }

    fn token_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: token_addr(),
        }
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn token_param() -> Value {
        json!({
            "key": {
                "standard": "erc20",
                "chain": chain().to_string(),
                "address": TOKEN
            }
        })
    }

    /// Lowered `Airdrop::Delegate` action body (mirrors `airdrop/delegate.rs`):
    /// carries `votingPower` as a `0x`-hex U256 fallback string.
    fn delegate_action(voting_power_hex: &str) -> Value {
        json!({
            "token": token_param(),
            "delegatee": "0x000000000000000000000000000000000000b0b0",
            "currentDelegate": "0x000000000000000000000000000000000000c0c0",
            "votingPower": voting_power_hex
        })
    }

    fn params(action: &Value) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "token": token_param(),
            "action": action
        })
    }

    /// A held governance `StakeReceipt` with the given durable voting power.
    fn stake_holding(voting_power: Option<U256>) -> TokenHolding {
        TokenHolding {
            key: token_key(),
            kind: TokenKind::StakeReceipt {
                protocol: ProtocolRef::new("uniswap"),
                underlying: TokenRef { key: token_key() },
                unlock: None,
                voting_power,
            },
            symbol: "UNI".to_owned(),
            decimals: 18,
            balance: Balance::fungible(U256::from(1_000u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: DataSource::OnchainView {
                chain: chain(),
                contract: token_addr(),
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    #[test]
    fn durable_stake_receipt_voting_power_wins() {
        let mut state = WalletState::new(wallet_id());
        state
            .tokens
            .insert(token_key(), stake_holding(Some(U256::from(5_000u64))));

        // Action fallback differs from the durable value to prove durable wins.
        let out = dispatch(
            "governance.voting_power_at_risk",
            &params(&delegate_action("0x1")),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["votingPowerAtRisk"], json!("5000.0000"));
    }

    #[test]
    fn action_voting_power_used_when_no_durable_receipt() {
        // Token held, but StakeReceipt confers no voting power → fall back.
        let mut state = WalletState::new(wallet_id());
        state.tokens.insert(token_key(), stake_holding(None));

        let vp_hex = format!("{:#x}", U256::from(42u64));
        let out = dispatch(
            "governance.voting_power_at_risk",
            &params(&delegate_action(&vp_hex)),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["votingPowerAtRisk"], json!("42.0000"));
    }

    #[test]
    fn action_voting_power_used_when_token_absent() {
        // Token not held at all → fall back to the action's lowered votingPower.
        let state = WalletState::new(wallet_id());
        let vp_hex = format!("{:#x}", U256::from(7u64));
        let out = dispatch(
            "governance.voting_power_at_risk",
            &params(&delegate_action(&vp_hex)),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["votingPowerAtRisk"], json!("7.0000"));
    }

    #[test]
    fn missing_durable_and_missing_action_field_is_bad_params() {
        let state = WalletState::new(wallet_id());
        let action_without_vp = json!({
            "token": token_param(),
            "delegatee": "0x000000000000000000000000000000000000b0b0"
        });
        let err = dispatch(
            "governance.voting_power_at_risk",
            &params(&action_without_vp),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn unknown_governance_method_is_unknown() {
        let state = WalletState::new(wallet_id());
        let err = dispatch(
            "governance.not_a_real_method",
            &json!({}),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }
}
