//! `token.*` enrichment-fact namespace — sim-server fact host.
//!
//! Auto-generated stub scaffold (one arm per sim-server `token.*` method in the
//! method-catalog `planned` set). The inner [`dispatch`] match is FROZEN at
//! scaffold time: it mirrors the catalog registry exactly and must never be
//! hand-edited. Devs fill in the per-method `fn` bodies (currently
//! [`FactError::NotImplemented`]) but leave the match arms untouched so the
//! `catalog_conformance` drift test keeps passing.
//!
//! Param shapes arrive as **lowered Cedar** values (not `simulation-state`
//! shapes), resolved by the extension before the call — see the sibling
//! `facts/params.rs` helpers (`chain_id` string, lowered `AssetRef`/`TokenRef`,
//! hex `U256` amounts).

use serde_json::{json, Value};

use policy_state::primitives::{Address, ChainId, U256};
use policy_state::token::holding::TokenHolding;
use policy_state::token::kind::{PegKind, TokenKind};

use super::params::{param_asset_contract, param_chain_id, param_u256};
use super::FactCtx;
use super::FactError;

/// Dispatch a `token.*` enrichment method against `ctx`.
///
/// FROZEN: one arm per sim-server `token.*` catalog method, plus a catch-all.
/// Do not edit — fill in the per-method `fn` bodies instead.
///
/// # Errors
///
/// [`FactError::UnknownMethod`] for an unregistered method; per-method errors
/// ([`FactError::NotImplemented`] / [`FactError::BadParams`]) propagate from the
/// individual fact fns.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "token.peg_ratio" => peg_ratio(params, ctx),
        "token.outflow_pct_of_holding" => outflow_pct_of_holding(params, ctx),
        "token.swap_out_classification" => swap_out_classification(params, ctx),
        "token.swap_special_token" => swap_special_token(params, ctx),
        "token.interest_bearing" => interest_bearing(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// Find the held [`TokenHolding`] for `(chain, contract)` by scanning
/// `WalletState.tokens`. Matches any standard (ERC20/721/1155) sharing the
/// contract address on the given chain — the lowered `AssetRef` only carries the
/// contract, so we key on `TokenKey::contract()` rather than reconstructing the
/// exact variant. Returns `None` when the token is not held.
fn holding_for<'a>(
    ctx: &'a FactCtx,
    chain: &ChainId,
    contract: &Address,
) -> Option<&'a TokenHolding> {
    ctx.state
        .tokens
        .values()
        .find(|h| h.key.chain() == chain && h.key.contract() == Some(contract))
}

/// `token.peg_ratio` (readKind: derived) — STK-01, UNI-13.
///
/// Ratio of an LST/stable token's market `price_value` to its declared peg
/// (`TokenKind.peg_to`). `< 1` signals depeg.
///
// BLOCKED: the two consuming policies (stk-lst-depeg-sell-warn,
// aave-lst-emode-divergence-warn) target LSTs = `TokenKind::Wrapped`
// (stETH/wstETH), whose correct peg ratio is `price_usd` over the UNDERLYING ETH
// price (a `PegTarget::Token` denominator). That underlying-ETH price is NOT
// reachable from single-token State_1 here:
//   - `FactCtx` carries only `state: &WalletState`; the underlying ETH is a
//     SEPARATE holding (possibly absent), and there is no cross-token price join
//     in this method's params (only `chain_id` + the single `asset`).
//   - the `Wrapped { underlying, .. }` ref names the token but yields no price;
//     `price_usd` is denominated in USD, not in the underlying.
// The only sub-case with a known denominator was the USD-stablecoin
// `Base { peg_to: Fiat(Usd) }` case (ratio == `price_usd`/1.0 USD); every
// `Wrapped` LST fell through to a hardcoded "1.0", so BOTH depeg policies could
// never fire — emitting a plausible-but-wrong peg. Downgraded to NotImplemented
// until a cross-token (underlying-price) join is available.
fn peg_ratio(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    Err(FactError::NotImplemented("token.peg_ratio".into()))
}

/// `token.outflow_pct_of_holding` (readKind: direct) — GEN-06.
///
/// The proposed action's `amount` as a percentage of the wallet's CURRENT
/// holding of that exact token (`amount / token_holdings.balance_amount × 100`).
/// Single-asset fraction-of-balance — distinct from the whole-portfolio fold
/// `portfolio.group_pct`.
///
/// Params:
/// - `chain_id`: Long (required)
/// - `owner`: String (required)
/// - `token`: `AssetRef` (required)
/// - `amount`: String (required) — proposed outflow (U256 hex), the percentage numerator.
///
/// Outputs:
/// - `pct`: decimal — from `$.result.pct`
fn outflow_pct_of_holding(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let contract = param_asset_contract(params, "token")?;
    let amount = param_u256(params, "amount")?;

    let held = holding_for(ctx, &chain, &contract)
        .and_then(|h| h.balance.as_fungible())
        .unwrap_or(U256::ZERO);

    Ok(json!({ "pct": pct_4dp(amount, held) }))
}

/// `token.swap_out_classification` (readKind: fold) — UNI-05.
///
/// Classification of the swap's `tokenOut`. `unclassified` = the token is held
/// but its `TokenKind` is `Unknown`, OR it is unheld and the external registry
/// cannot classify it; `honeypot` = an external sim/detector flags it
/// buyable-but-not-sellable. Folds the held-token kind read with an external
/// probe; emits both flags from one call.
///
/// PARTIAL: only the held-state half is computable here. `unclassified` is true
/// when the token is held with `TokenKind::Unknown`, OR when the token is not
/// held at all (no state classification available — conservative warn). The
/// "unheld AND external registry classifies it" refinement and the `honeypot`
/// probe both require an external token-reputation/honeypot registry that is not
/// part of `WalletState`; `honeypot` is reported `false` (conservative — no
/// detection without the feed).
///
/// Params:
/// - `chain_id`: Long (required)
/// - `owner`: String (required)
/// - `asset`: `AssetRef` (required) — the token being bought.
///
/// Outputs:
/// - `unclassified`: Bool — from `$.result.unclassified`
/// - `honeypot`: Bool — from `$.result.honeypot`
fn swap_out_classification(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let contract = param_asset_contract(params, "asset")?;

    let unclassified = match holding_for(ctx, &chain, &contract) {
        Some(h) => matches!(h.kind, TokenKind::Unknown),
        // Not held → no in-state classification; flag for warn (the external
        // registry refinement is unavailable here, see PARTIAL note).
        None => true,
    };

    Ok(json!({
        "unclassified": unclassified,
        // BLOCKED-HALF: external honeypot/reputation registry not in WalletState.
        "honeypot": false,
    }))
}

/// `token.swap_special_token` (readKind: fold) — UNI-13.
///
/// Is the swap's `tokenIn` a special-transfer token? `special` = held token
/// whose `TokenKind` is `Wrapped` with `PegKind::Rebasing` (durable state);
/// `feeOnTransfer` = an external token-tax registry flags it as fee-on-transfer.
/// Folds the held-state peg read with the external `FoT` probe; emits both flags
/// from one call.
///
/// PARTIAL: only the held-state `special` half is computable here (held
/// `Wrapped` token with `PegKind::Rebasing`). The `feeOnTransfer` half needs an
/// external token-tax / fee-on-transfer registry that is not part of
/// `WalletState`; it is reported `false` (conservative — no `FoT` flag without the
/// feed).
///
/// Params:
/// - `chain_id`: Long (required)
/// - `owner`: String (required)
/// - `asset`: `AssetRef` (required) — the token being sold.
///
/// Outputs:
/// - `special`: Bool — from `$.result.special`
/// - `feeOnTransfer`: Bool — from `$.result.feeOnTransfer`
fn swap_special_token(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let contract = param_asset_contract(params, "asset")?;

    let special = holding_for(ctx, &chain, &contract).is_some_and(|h| {
        matches!(
            h.kind,
            TokenKind::Wrapped {
                peg_kind: PegKind::Rebasing,
                ..
            }
        )
    });

    Ok(json!({
        "special": special,
        // BLOCKED-HALF: external token-tax (fee-on-transfer) registry not in WalletState.
        "feeOnTransfer": false,
    }))
}

/// `token.interest_bearing` (readKind: direct) — H2 (riba).
///
/// Is the acquired token interest/yield-bearing per its `TokenKind`?
/// `YieldReceipt` / `DebtReceipt` (`RateMode`) / `StakeReceipt` variants
/// structurally identify interest-bearing assets — a DIRECT state read of the
/// token's kind, no external feed. (The content-screening half of H2 is out of
/// scope for this method.) A token that is not held (or whose kind is not one of
/// those variants) is reported `false`.
///
/// Params:
/// - `chain_id`: Long (required)
/// - `asset`: `AssetRef` (required) — the token being acquired.
///
/// Outputs:
/// - `isInterestBearing`: Bool — from `$.result.isInterestBearing`
fn interest_bearing(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let contract = param_asset_contract(params, "asset")?;

    let is_interest_bearing = holding_for(ctx, &chain, &contract).is_some_and(|h| {
        matches!(
            h.kind,
            TokenKind::YieldReceipt { .. }
                | TokenKind::DebtReceipt { .. }
                | TokenKind::StakeReceipt { .. }
        )
    });

    Ok(json!({ "isInterestBearing": is_interest_bearing }))
}

/// Render `numerator / divisor × 100` to a 4-decimal-place percentage string
/// using U256 integer math (no float — `numerator` can be `U256::MAX`). Divisor
/// zero → `"100.0000"` floor for a zero numerator (0% of nothing), else the
/// over-balance sentinel scale (a positive spend against a zero holding is an
/// unbounded percentage). Mirrors the `over_balance_4dp` idiom but scales by
/// 100 × `10_000` for a percentage at 4dp.
fn pct_4dp(numerator: U256, divisor: U256) -> String {
    if divisor.is_zero() {
        // No holding to take a percentage of: 0% if nothing is moving, else a
        // deliberately huge value so a `.greaterThan(threshold)` policy trips.
        return if numerator.is_zero() {
            "0.0000".to_owned()
        } else {
            "100000000.0000".to_owned()
        };
    }
    // scaled = numerator * 100 * 10_000 / divisor, then split into whole.frac4.
    let pct_scale = U256::from(1_000_000u64);
    let scale = U256::from(10_000u64);
    let scaled = numerator.saturating_mul(pct_scale) / divisor;
    let whole = scaled / scale;
    let frac = scaled % scale;
    format!("{whole}.{frac:04}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Price, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{
        BaseCategory, FiatCurrency, PegKind, PegTarget, TokenKind,
    };
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

    fn source() -> DataSource {
        DataSource::OnchainView {
            chain: chain(),
            contract: token_addr(),
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        }
    }

    /// A `WalletState` holding `balance` of the token with the given `kind` and
    /// optional `price` (USD).
    fn state_with(balance: u64, kind: TokenKind, price: Option<&str>) -> WalletState {
        let mut state = WalletState::new(wallet_id());
        state.tokens.insert(
            token_key(),
            TokenHolding {
                key: token_key(),
                kind,
                symbol: "TKN".to_owned(),
                decimals: 18,
                balance: Balance::fungible(U256::from(balance)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: price.map(|p| {
                    LiveField::new(
                        Price::new(p.to_owned()),
                        source(),
                        Time::from_unix(1_700_000_000),
                    )
                }),
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: source(),
            },
        );
        state
    }

    fn asset_param() -> Value {
        json!({ "key": { "standard": "erc20", "chain": chain().to_string(), "address": TOKEN } })
    }

    fn base_kind(peg_to: Option<PegTarget>) -> TokenKind {
        TokenKind::Base {
            category: BaseCategory::Stable,
            peg_to,
        }
    }

    #[test]
    fn outflow_pct_half_of_holding_is_50pct() {
        let state = state_with(2_000, base_kind(None), None);
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "token": asset_param(),
            "amount": format!("{:#x}", U256::from(1_000u64)),
        });
        let out = dispatch(
            "token.outflow_pct_of_holding",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("50.0000"));
    }

    #[test]
    fn outflow_pct_unheld_token_is_sentinel() {
        let state = WalletState::new(wallet_id());
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "token": asset_param(),
            "amount": format!("{:#x}", U256::from(1u64)),
        });
        let out = dispatch(
            "token.outflow_pct_of_holding",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("100000000.0000"));
    }

    #[test]
    fn interest_bearing_true_for_yield_receipt() {
        let kind = TokenKind::YieldReceipt {
            protocol: policy_state::primitives::ProtocolRef::new("aave-v3"),
            underlying: TokenRef::new(token_key()),
            rebase_form: policy_state::token::kind::RebaseForm::Index,
        };
        let state = state_with(1_000, kind, None);
        let params = json!({ "chain_id": chain().to_string(), "asset": asset_param() });
        let out = dispatch(
            "token.interest_bearing",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["isInterestBearing"], json!(true));
    }

    #[test]
    fn interest_bearing_false_for_base_and_unheld() {
        let state = state_with(1_000, base_kind(None), None);
        let params = json!({ "chain_id": chain().to_string(), "asset": asset_param() });
        let out = dispatch(
            "token.interest_bearing",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["isInterestBearing"], json!(false));

        let empty = WalletState::new(wallet_id());
        let out = dispatch(
            "token.interest_bearing",
            &params,
            &FactCtx { state: &empty },
        )
        .unwrap();
        assert_eq!(out["isInterestBearing"], json!(false));
    }

    #[test]
    fn swap_special_token_true_for_rebasing_wrapped() {
        let kind = TokenKind::Wrapped {
            underlying: TokenRef::new(token_key()),
            peg_kind: PegKind::Rebasing,
        };
        let state = state_with(1_000, kind, None);
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "asset": asset_param(),
        });
        let out = dispatch(
            "token.swap_special_token",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["special"], json!(true));
        assert_eq!(out["feeOnTransfer"], json!(false));
    }

    #[test]
    fn swap_special_token_false_for_hard_peg_wrapped() {
        let kind = TokenKind::Wrapped {
            underlying: TokenRef::new(token_key()),
            peg_kind: PegKind::HardPeg,
        };
        let state = state_with(1_000, kind, None);
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "asset": asset_param(),
        });
        let out = dispatch(
            "token.swap_special_token",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["special"], json!(false));
    }

    #[test]
    fn swap_out_classification_unknown_is_unclassified() {
        let state = state_with(1_000, TokenKind::Unknown, None);
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "asset": asset_param(),
        });
        let out = dispatch(
            "token.swap_out_classification",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["unclassified"], json!(true));
        assert_eq!(out["honeypot"], json!(false));
    }

    #[test]
    fn swap_out_classification_unheld_is_unclassified() {
        let state = WalletState::new(wallet_id());
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "asset": asset_param(),
        });
        let out = dispatch(
            "token.swap_out_classification",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["unclassified"], json!(true));
    }

    #[test]
    fn swap_out_classification_base_is_classified() {
        let state = state_with(1_000, base_kind(None), None);
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "asset": asset_param(),
        });
        let out = dispatch(
            "token.swap_out_classification",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["unclassified"], json!(false));
    }

    #[test]
    fn peg_ratio_is_blocked() {
        // Downgraded: a correct LST peg ratio needs the underlying-ETH price,
        // which is not reachable from single-token State_1 here. The former
        // computed path returned a plausible-but-wrong "1.0" for every Wrapped
        // LST, so both depeg policies could never fire.
        let state = state_with(
            1_000,
            base_kind(Some(PegTarget::Fiat(FiatCurrency::Usd))),
            Some("0.9912"),
        );
        let params = json!({ "chain_id": chain().to_string(), "asset": asset_param() });
        let err = dispatch("token.peg_ratio", &params, &FactCtx { state: &state }).unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
