//! `valuation.*` enrichment-fact namespace — USD valuation reads/derivations
//! against the simulated `WalletState` (sim-server fact host, ADR-009).
//!
//! These facts replace the live `oracle.usd_value` call with DIRECT reads of the
//! refrigerated `TokenHolding.price_usd` `LiveField`. Every method in this module
//! is a `server: sim-server` planned method drawn from
//! `schema/method-catalog.json` (namespace `valuation.`).
//!
//! ## Scaffold contract (FROZEN dispatch, stub bodies)
//!
//! [`dispatch`] is generated to mirror the catalog 1:1 and is **frozen**: one arm
//! per sim-server `valuation.*` method plus a catch-all. Devs filling in the
//! bodies must never edit the match. Each per-method fn currently returns
//! [`FactError::NotImplemented`] so the server still boots and serves the methods
//! that ARE implemented in sibling namespaces.
//!
//! ## Param shape contract
//!
//! Like the rest of `facts/`, `params` arrive as **lowered Cedar** shapes from
//! the extension (not `simulation-state` shapes):
//!   - `chain_id`: string (e.g. `"eip155:1"`)
//!   - `asset` / `value_asset`: lowered `Core::TokenRef`
//!     (`{ "key": { "standard", "chain", "address" } }`)
//!   - `amount` / `value_amount` / `gas_estimate`: hex-encoded `U256` strings
//!   - `owner`: hex address string

use serde_json::{json, Value};

use policy_state::primitives::{Decimal, U256};
use policy_state::token::holding::TokenHolding;
use policy_state::token::TokenKey;

use super::params::{
    over_balance_4dp, param_asset_contract, param_chain_id, param_u256, OVER_BALANCE_SENTINEL,
};
use super::FactCtx;
use super::FactError;

/// Number of fractional digits the USD results are rendered to. Matches the
/// `over_balance_4dp` convention in `params.rs` so Cedar `.greaterThan(...)`
/// thresholds compare against a consistent fixed-point shape.
const USD_DP: u32 = 4;

/// USD-decimal sentinel for "unpriced": when a touched token has no
/// `price_usd` `LiveField` we cannot value it, so a conservative huge number is
/// emitted (so a cap-deny policy trips) — mirroring `OVER_BALANCE_SENTINEL`.
const UNPRICED_USD_SENTINEL: &str = "1000000000.0000";

/// Parse a decimal string (e.g. `"3500.25"`, `"0"`, `"1e0"`-free plain form)
/// into `(mantissa, frac_digits)` such that `value == mantissa / 10^frac_digits`.
/// Returns `None` on any non-numeric input (a non-finite / scientific form), so
/// callers can fall back to the unpriced sentinel rather than fabricate a price.
fn parse_decimal_scaled(d: &Decimal) -> Option<(U256, u32)> {
    let s = d.as_str().trim();
    let s = s.strip_prefix('+').unwrap_or(s);
    // Negative prices are nonsensical for a USD valuation; reject.
    if s.starts_with('-') || s.is_empty() {
        return None;
    }
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{int_part}{frac_part}");
    let digits = if digits.is_empty() { "0" } else { &digits };
    let mantissa = U256::from_str_radix(digits, 10).ok()?;
    let frac_digits = u32::try_from(frac_part.len()).ok()?;
    Some((mantissa, frac_digits))
}

/// USD value of `raw_amount` raw token units priced at `price` (USD per whole
/// token), expressed as an integer `value * 10^USD_DP` via U256 integer math.
///
/// `usd = raw_amount * price / 10^decimals`. Parsing `price` to a scaled
/// integer, the closed form scaled by `10^USD_DP` is:
///   `raw * price_mantissa * 10^USD_DP / (10^decimals * 10^price_frac_digits)`.
///
/// Saturating multiplication guards adversarial `U256::MAX` inputs (saturates
/// huge — the conservative direction for a cap-deny policy). Returns `None`
/// when `price` is unparseable so the caller can emit the unpriced sentinel.
fn usd_value_scaled(raw_amount: U256, decimals: u8, price: &Decimal) -> Option<U256> {
    let (price_mantissa, price_frac) = parse_decimal_scaled(price)?;
    let numerator = raw_amount
        .saturating_mul(price_mantissa)
        .saturating_mul(U256::from(10u64).pow(U256::from(USD_DP)));
    let denom_pow = u64::from(decimals) + u64::from(price_frac);
    let denominator = U256::from(10u64).pow(U256::from(denom_pow));
    Some(numerator / denominator)
}

/// [`usd_value_scaled`] rendered to a [`USD_DP`]-place decimal string.
fn usd_value_dp(raw_amount: U256, decimals: u8, price: &Decimal) -> Option<String> {
    usd_value_scaled(raw_amount, decimals, price).map(render_scaled)
}

/// Render an integer that already represents `value * 10^USD_DP` as a
/// `whole.frac` decimal string with exactly [`USD_DP`] fractional digits.
fn render_scaled(scaled: U256) -> String {
    let scale = U256::from(10u64).pow(U256::from(USD_DP));
    let whole = scaled / scale;
    let frac = scaled % scale;
    let dp = USD_DP as usize;
    format!("{whole}.{frac:0dp$}")
}

/// Read a token's `price_usd` `LiveField` value (a [`Decimal`]); `None` when the
/// holding is absent or unpriced.
fn holding_price(holding: &TokenHolding) -> Option<&Decimal> {
    holding.price_usd.as_ref().map(|lf| &lf.value)
}

/// Dispatch a `valuation.*` enrichment fact against `ctx`.
///
/// FROZEN at scaffold time: one arm per sim-server `valuation.*` method from the
/// catalog, plus a catch-all. Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when `method` is not a `valuation.*`
/// method in this registry, [`FactError::NotImplemented`] when the matched fact
/// body is still a scaffold stub, or [`FactError::BadParams`] from an
/// implemented body whose `params` are missing/ill-shaped.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "valuation.asset_usd" => asset_usd(params, ctx),
        "valuation.tx_total_usd" => tx_total_usd(params, ctx),
        "valuation.gas_cost_usd" => gas_cost_usd(params, ctx),
        "valuation.gas_cost_ratio" => gas_cost_ratio(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// `valuation.asset_usd` — USD valuation of a single asset position
/// (token amount × DB price). DIRECT read of the refrigerated `price_value`
/// `LiveField` (ADR-009), replacing the live `oracle.usd_value` call.
///
/// readKind: `derived`
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `asset`: `AssetRef` (required) — `$.action.inputToken.asset`
///   - `amount`: String (required) — `$.action.inputToken.amount.value`
///
/// Outputs (catalog): `usd`: decimal — from `$.result.usd`
///
/// State accessors to call (Ground list):
///   - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — look up the
///     `TokenHolding` for `asset`'s reconstructed `TokenKey`.
///   - `TokenHolding.price_usd: Option<LiveField<Price>>` — the DB price; multiply
///     by the (decimals-scaled) `amount` to get USD.
fn asset_usd(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let amount = param_u256(params, "amount")?;
    let asset_contract = param_asset_contract(params, "asset")?;

    let key = TokenKey::Erc20 {
        chain,
        address: asset_contract,
    };
    let usd = ctx
        .state
        .tokens
        .get(&key)
        .and_then(|h| holding_price(h).map(|p| (h.decimals, p)))
        .and_then(|(decimals, price)| usd_value_dp(amount, decimals, price))
        .unwrap_or_else(|| UNPRICED_USD_SENTINEL.to_owned());

    Ok(json!({ "usd": usd }))
}

/// `valuation.tx_total_usd` — total USD outflow of the whole transaction (incl.
/// multicall children): sum of net-negative balance deltas × `price_value`,
/// computed against State₂ via the reducer. Drives GEN-05.
///
/// readKind: `reducer`
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `owner`: String (required) — `$.root.from`
///   - `action`: Action (required) — `$.action` (tx/multicall applied via reducer)
///
/// Outputs (catalog): `usd`: decimal — from `$.result.usd`
///
/// State accessors to call (Ground list):
///   - `TokenHolding.price_usd: Option<LiveField<Price>>` — price each net outflow.
///   - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — resolve per-token
///     holdings the deltas touch.
// BLOCKED: needs the reducer's `StateDelta.token_changes` (per-token BalanceDelta)
// for State_2, none of which is reachable here:
//   - `FactCtx` carries only `state: &WalletState` (State_1) — no `state_after`
//     and no `StateDelta` field.
//   - the state surface has NO `apply(action) -> StateDelta` (state_map: "NO
//     apply() method; NO apply()->delta transformation logic").
//   - the lowered `$.action` param carries NO documented net-outflow field
//     (no `live_inputs.netOutflow` / per-token outflow vec) the catalog points at,
//     unlike `reserve.*` which rides `live_inputs.reserveState`.
// Summing net outflow would require either applying the reducer (absent) or
// decoding arbitrary tx/multicall calldata into per-token deltas (not in params).
fn tx_total_usd(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    Err(FactError::NotImplemented("valuation.tx_total_usd".into()))
}

/// `valuation.gas_cost_usd` — gas cost in USD = `gas_estimate` (action field) ×
/// native-token `price_value`. Drives GEN-07/08.
///
/// readKind: `derived`
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `gas_estimate`: String (required) — `$.action.gasEstimate`
///
/// Outputs (catalog): `usd`: decimal — from `$.result.usd`
///
/// State accessors to call (Ground list):
///   - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — locate the native
///     token holding for `chain_id`.
///   - `TokenHolding.price_usd: Option<LiveField<Price>>` — native price multiplied
///     by the gas estimate.
fn gas_cost_usd(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let gas_estimate = param_u256(params, "gas_estimate")?;

    // The native gas token is keyed by `TokenKey::Native { chain }`; look it up
    // directly in the holdings map (no by-chain helper exists in state).
    let native_key = TokenKey::Native { chain };
    let usd = ctx
        .state
        .tokens
        .get(&native_key)
        .and_then(|h| holding_price(h).map(|p| (h.decimals, p)))
        .and_then(|(decimals, price)| usd_value_dp(gas_estimate, decimals, price))
        .unwrap_or_else(|| UNPRICED_USD_SENTINEL.to_owned());

    Ok(json!({ "usd": usd }))
}

/// `valuation.gas_cost_ratio` — ratio of the tx's gas cost in USD to the USD
/// value the action moves (`gas_usd / action_value_usd`). The division is done
/// server-side because Cedar cannot divide two decimals. Drives GEN-08.
///
/// readKind: `derived`
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `gas_estimate`: String (required) — `$.action.gasEstimate`
///   - `value_asset`: `AssetRef` (required) — `$.action.tokenIn`
///   - `value_amount`: String (required) — `$.action.direction.amountIn`
///
/// Outputs (catalog): `ratio`: decimal — from `$.result.ratio`
///
/// State accessors to call (Ground list):
///   - `WalletState.tokens: BTreeMap<TokenKey, TokenHolding>` — native token (gas)
///     and `value_asset` holdings.
///   - `TokenHolding.price_usd: Option<LiveField<Price>>` — native price for the
///     numerator's `gas_usd` and `value_asset` price for the denominator.
fn gas_cost_ratio(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_chain_id(params, "chain_id")?;
    let gas_estimate = param_u256(params, "gas_estimate")?;
    let value_contract = param_asset_contract(params, "value_asset")?;
    let value_amount = param_u256(params, "value_amount")?;

    // gas_usd numerator (native token), scaled by 10^USD_DP.
    let native_key = TokenKey::Native {
        chain: chain.clone(),
    };
    let gas_scaled = ctx
        .state
        .tokens
        .get(&native_key)
        .and_then(|h| holding_price(h).map(|p| (h.decimals, p)))
        .and_then(|(decimals, price)| usd_value_scaled(gas_estimate, decimals, price));

    // value_usd denominator (value_asset, an ERC20/NFT contract).
    let value_key = TokenKey::Erc20 {
        chain,
        address: value_contract,
    };
    let value_scaled = ctx
        .state
        .tokens
        .get(&value_key)
        .and_then(|h| holding_price(h).map(|p| (h.decimals, p)))
        .and_then(|(decimals, price)| usd_value_scaled(value_amount, decimals, price));

    // An unpriced gas token alone can't justify the ratio; an unpriced/zero
    // value denominator means "ratio unbounded" → sentinel (cap-deny trips).
    let ratio = match (gas_scaled, value_scaled) {
        (Some(g), Some(v)) => over_balance_4dp(g, v),
        (None, _) | (_, None) => OVER_BALANCE_SENTINEL.to_owned(),
    };

    Ok(json!({ "ratio": ratio }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use serde_json::json;

    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Price, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::{WalletId, WalletState};

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn wallet() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        ))
    }

    fn priced_holding(key: &TokenKey, decimals: u8, price: &str) -> TokenHolding {
        TokenHolding {
            key: key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Volatile,
                peg_to: None,
            },
            symbol: "TKN".to_owned(),
            decimals,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(LiveField::new(
                Price::new(price),
                DataSource::UserSupplied,
                Time::from_unix(1_700_000_000),
            )),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_700_000_000),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn native_key() -> TokenKey {
        TokenKey::Native { chain: chain() }
    }

    fn usdc_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str(USDC).unwrap(),
        }
    }

    fn asset_param() -> Value {
        json!({ "key": { "standard": "erc20", "chain": chain().to_string(), "address": USDC } })
    }

    #[test]
    fn parse_decimal_scaled_handles_int_and_frac() {
        assert_eq!(
            parse_decimal_scaled(&Decimal::new("3500.25")),
            Some((U256::from(350_025_u64), 2))
        );
        assert_eq!(
            parse_decimal_scaled(&Decimal::new("7")),
            Some((U256::from(7u64), 0))
        );
        assert_eq!(
            parse_decimal_scaled(&Decimal::new("0")),
            Some((U256::ZERO, 0))
        );
        assert_eq!(parse_decimal_scaled(&Decimal::new("-5")), None);
        assert_eq!(parse_decimal_scaled(&Decimal::new("1e9")), None);
    }

    #[test]
    fn usd_value_dp_scales_by_decimals() {
        // 2 whole tokens (6dp) at $1.50 → $3.0000.
        let two_tokens = U256::from(2_000_000u64);
        assert_eq!(
            usd_value_dp(two_tokens, 6, &Decimal::new("1.5")).unwrap(),
            "3.0000"
        );
        // 1 whole 18dp token at $3500.25 → $3500.2500.
        let one_eth = U256::from(10u64).pow(U256::from(18u64));
        assert_eq!(
            usd_value_dp(one_eth, 18, &Decimal::new("3500.25")).unwrap(),
            "3500.2500"
        );
    }

    #[test]
    fn asset_usd_prices_holding() {
        let mut state = wallet();
        // $1.50 per whole token, 6 decimals.
        state
            .tokens
            .insert(usdc_key(), priced_holding(&usdc_key(), 6, "1.5"));
        let params = json!({
            "chain_id": chain().to_string(),
            "asset": asset_param(),
            "amount": format!("{:#x}", U256::from(2_000_000u64))
        });
        let out = dispatch("valuation.asset_usd", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["usd"], json!("3.0000"));
    }

    #[test]
    fn asset_usd_unpriced_returns_sentinel() {
        let state = wallet(); // no holding at all → unpriced.
        let params = json!({
            "chain_id": chain().to_string(),
            "asset": asset_param(),
            "amount": format!("{:#x}", U256::from(2_000_000u64))
        });
        let out = dispatch("valuation.asset_usd", &params, &FactCtx { state: &state }).unwrap();
        assert_eq!(out["usd"], json!(UNPRICED_USD_SENTINEL));
    }

    #[test]
    fn gas_cost_usd_prices_native() {
        let mut state = wallet();
        // ETH at $2000, 18 decimals.
        state
            .tokens
            .insert(native_key(), priced_holding(&native_key(), 18, "2000"));
        // 0.001 ETH of gas = 1e15 wei.
        let gas = U256::from(10u64).pow(U256::from(15u64));
        let params = json!({
            "chain_id": chain().to_string(),
            "gas_estimate": format!("{gas:#x}")
        });
        let out = dispatch(
            "valuation.gas_cost_usd",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        // 0.001 ETH * $2000 = $2.0000.
        assert_eq!(out["usd"], json!("2.0000"));
    }

    #[test]
    fn gas_cost_usd_unpriced_native_returns_sentinel() {
        let state = wallet();
        let params = json!({ "chain_id": chain().to_string(), "gas_estimate": "0x5af3107a4000" });
        let out = dispatch(
            "valuation.gas_cost_usd",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["usd"], json!(UNPRICED_USD_SENTINEL));
    }

    #[test]
    fn gas_cost_ratio_divides_gas_by_value() {
        let mut state = wallet();
        // ETH $2000 (18dp); value asset $1.50 (6dp).
        state
            .tokens
            .insert(native_key(), priced_holding(&native_key(), 18, "2000"));
        state
            .tokens
            .insert(usdc_key(), priced_holding(&usdc_key(), 6, "1.5"));
        // gas 0.001 ETH = $2.0000; value 200 tokens = $300.0000 → ratio 0.0066…
        let gas = U256::from(10u64).pow(U256::from(15u64));
        let value_amount = U256::from(200_000_000u64); // 200 * 1e6
        let params = json!({
            "chain_id": chain().to_string(),
            "gas_estimate": format!("{gas:#x}"),
            "value_asset": asset_param(),
            "value_amount": format!("{value_amount:#x}")
        });
        let out = dispatch(
            "valuation.gas_cost_ratio",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        // 2.0000 / 300.0000 = 0.0066 (4dp truncated).
        assert_eq!(out["ratio"], json!("0.0066"));
    }

    #[test]
    fn gas_cost_ratio_unpriced_value_returns_sentinel() {
        let mut state = wallet();
        state
            .tokens
            .insert(native_key(), priced_holding(&native_key(), 18, "2000"));
        // value asset absent → denominator unpriced → sentinel.
        let params = json!({
            "chain_id": chain().to_string(),
            "gas_estimate": "0x38d7ea4c68000",
            "value_asset": asset_param(),
            "value_amount": "0xbebc200"
        });
        let out = dispatch(
            "valuation.gas_cost_ratio",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["ratio"], json!(OVER_BALANCE_SENTINEL));
    }

    #[test]
    fn tx_total_usd_is_blocked() {
        let state = wallet();
        let params = json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "action": { "kind": "multicall" }
        });
        let err = dispatch(
            "valuation.tx_total_usd",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
