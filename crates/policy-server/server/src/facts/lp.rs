//! `lp.*` enrichment-fact namespace — concentrated-liquidity / AMM-LP facts.
//!
//! Scaffold module: the inner [`dispatch`] match is FROZEN at scaffold time (one
//! arm per sim-server `lp.*` method in `schema/method-catalog.json`, plus a
//! catch-all). Devs fill in the per-method `fn` bodies; they must never edit the
//! match so the `catalog_conformance` drift test keeps passing.
//!
//! Param shapes arrive as **lowered Cedar** values (not `simulation-state`
//! shapes), resolved by the extension before the call — see the sibling
//! `facts/params.rs` helpers (`chain_id` string, lowered `AssetRef`/`TokenRef`,
//! hex `U256` amounts). LP facts compare tick ranges (Longs) against a
//! `currentPrice` (decimal String): the tick<->price conversion is beyond Cedar,
//! so it is done natively here.
//!
//! Both methods are sim-server. `lp.range_out_of_position` (`derived`) is an
//! add-liquidity entry-range check servable from the action's own range ticks +
//! live `currentPrice` (no wallet read). `lp.exit_asymmetry` (`fold`) folds the
//! HELD concentrated position's durable entry range against the current price, so
//! it reads wallet state. There are no external/local `lp.*` methods to exclude.

use serde_json::{json, Value};

use super::params::param_action;
use super::FactCtx;
use super::FactError;

/// Uniswap V3/V4 tick base: `price(token1/token0) = 1.0001^tick`. The
/// tick<->price conversion is the native step Cedar cannot do; f64 is adequate
/// here because the result feeds a single threshold comparison (is the live
/// price inside `[1.0001^tickLower, 1.0001^tickUpper]`?), not money math.
const TICK_BASE: f64 = 1.0001;

/// Convert a Uniswap V3/V4 tick to its `token1/token0` price via `1.0001^tick`.
fn tick_to_price(tick: i64) -> f64 {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    TICK_BASE.powi(tick as i32)
}

/// Dispatch an `lp.*` enrichment fact against `ctx`.
///
/// FROZEN: one arm per sim-server `lp.*` method in the catalog, plus a catch-all.
/// Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] for an unregistered method, or whatever
/// error the per-method fn surfaces (currently [`FactError::NotImplemented`]
/// until the body is filled in).
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "lp.range_out_of_position" => range_out_of_position(params, ctx),
        "lp.exit_asymmetry" => exit_asymmetry(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// UNI-14: does a concentrated `add_liquidity` position's tick range
/// `[range.tickLower, tickUpper]` fail to contain the current price? The range
/// bounds are Longs and `currentPrice` is a decimal String; converting a tick to
/// a comparable price is beyond Cedar, so this does the tick<->price check
/// natively. `true` = a one-sided range order (100% of one asset) with outsized
/// IL exposure.
///
/// readKind: `derived`.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `action`: Action (required) — `$.action`; the `add_liquidity` action whose
///   concentrated range is tested against the current price
///
/// Catalog outputs:
/// - `outOfRange`: Bool — from `$.result.outOfRange`
///
/// State accessors the implementer should call:
/// - (none) — derived purely from the action: the decoded range ticks
///   (`range.tickLower`/`tickUpper`) vs the live `currentPrice`
///   (`AddLiquidityLiveInputs.current_price`); no wallet-state read.
fn range_out_of_position(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let action = param_action(params, "action")?;

    // `currentPrice` is the inlined `AddLiquidityLiveInputs.current_price` (a
    // decimal String of token1/token0), top-level on the lowered add-liquidity
    // body. Parse to f64 for the tick threshold comparison only.
    let current_price = action
        .get("currentPrice")
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("missing `action.currentPrice`".to_owned()))?
        .parse::<f64>()
        .map_err(|e| FactError::BadParams(format!("action.currentPrice not a decimal: {e}")))?;

    // Only a concentrated tick range can be out of position; pooled adds have no
    // range, and bin/custom ranges carry no tick bounds at the policy layer.
    let range = action
        .get("params")
        .and_then(|p| p.get("range"))
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("tick"));
    let Some(range) = range else {
        // No tick range to test (pooled add, or bin/custom concentrated range):
        // not a one-sided tick order, so not out of range.
        return Ok(json!({ "outOfRange": false }));
    };

    let tick_lower = range
        .get("tickLower")
        .and_then(Value::as_i64)
        .ok_or_else(|| FactError::BadParams("missing `range.tickLower`".to_owned()))?;
    let tick_upper = range
        .get("tickUpper")
        .and_then(Value::as_i64)
        .ok_or_else(|| FactError::BadParams("missing `range.tickUpper`".to_owned()))?;

    let price_lower = tick_to_price(tick_lower);
    let price_upper = tick_to_price(tick_upper);

    // Out of range = the current price falls outside [priceLower, priceUpper] →
    // the mint deposits 100% of one asset (a one-sided range order).
    let out_of_range = current_price < price_lower || current_price > price_upper;

    Ok(json!({ "outOfRange": out_of_range }))
}

/// AMMLP-2: does removing this liquidity lock in impermanent loss because the
/// HELD concentrated position has drifted heavily to one side of its entry range?
/// Folds the held position's entry range (durable state) against the current
/// price. `true` = exit-at-a-loss (distinct from UNI-14's one-sided entry).
/// `sv = PART`: the remove path does not yet carry a `current_price` `LiveField`.
///
/// readKind: `fold`.
///
/// Catalog params:
/// - `chain_id`: Long (required) — `$.root.chain_id`
/// - `owner`: String (required) — `$.root.from`; wallet whose held position entry
///   range is read
/// - `action`: Action (required) — `$.action`; the `remove_liquidity` action
///   identifying the position (nftKey / lpToken) being burned
///
/// Catalog outputs:
/// - `asymmetric`: Bool — from `$.result.asymmetric`
///
/// State accessors the implementer should call:
/// - `WalletState.positions: Vec<Position>` — locate the held concentrated-liquidity
///   position for (owner, nftKey/lpToken) and read its `entry_range`
///   (`lp_positions.entry_range`).
// STATE-WORKER ASK (entry range — REACHABLE, revising the original ask): the held
// concentrated entry range IS present in state, NOT under `WalletState.positions`
// (which has no LP variant) but as a `TokenHolding` whose
// `kind = TokenKind::LpShare { shape: LpShape::Concentrated { range: RangeSpec::Tick
// { lower, upper, .. }, .. }, .. }`, located by the remove action's `nftKey`
// (`params.nftKey` → TokenKey) in `WalletState.tokens`. So `lp_positions.entry_range`
// maps to `tokens[nftKey].kind.LpShare.shape.Concentrated.range.Tick.{lower,upper}`.
// BLOCKED on the SECOND ask only:
// STATE-WORKER ASK: needs a live current-price injection on the remove-liquidity path
// (sv = PART). `RemoveLiquidityLiveInputs` carries only `pool_state` + `fees_owed`; the
// lowered remove body emits `feesOwed` but NO `currentPrice`. With the entry range in
// hand but no current price to test it against, the asymmetry fold cannot be computed.
fn exit_asymmetry(_params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    // BLOCKED: missing `action.currentPrice` on the remove-liquidity path
    // (RemoveLiquidityLiveInputs has no `current_price` LiveField; the lowered
    // body emits `feesOwed` only). Entry range itself is reachable via
    // `state.tokens[params.nftKey].kind = LpShare::Concentrated.range::Tick`,
    // but with no current price the fold has nothing to compare against.
    Err(FactError::NotImplemented("lp.exit_asymmetry".into()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use serde_json::json;

    use policy_state::primitives::ChainId;
    use policy_state::{WalletId, WalletState};

    use super::*;

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            "0x000000000000000000000000000000000000a01c"
                .parse()
                .unwrap(),
            [ChainId::ethereum_mainnet()],
        ))
    }

    /// A lowered `add_liquidity` body (`concentrated_mint`, tick range) with the
    /// given bounds + inlined `currentPrice`. Mirrors the policy-engine
    /// `lowering_v2::amm::add_liquidity` output shape.
    fn add_liquidity_action(tick_lower: i64, tick_upper: i64, current_price: &str) -> Value {
        json!({
            "action": {
                "venue": { "kind": "uniswap_v3" },
                "params": {
                    "kind": "concentrated_mint",
                    "range": {
                        "kind": "tick",
                        "tickLower": tick_lower,
                        "tickUpper": tick_upper,
                        "liquidity": "0x75bcd15"
                    }
                },
                "currentPrice": current_price
            }
        })
    }

    fn dispatch_out_of_range(params: &Value) -> Value {
        let state = empty_state();
        super::super::dispatch(
            "lp.range_out_of_position",
            params,
            &FactCtx { state: &state },
        )
        .unwrap()
    }

    #[test]
    fn current_price_inside_range_is_in_position() {
        // tick 0 → price 1.0; range [-100, 100] → ~[0.9900, 1.0101]; price 1.0 in.
        let out = dispatch_out_of_range(&add_liquidity_action(-100, 100, "1.0"));
        assert_eq!(out["outOfRange"], json!(false));
    }

    #[test]
    fn current_price_below_range_is_out_of_position() {
        // Range [10, 200] → ~[1.0010, 1.0202]; price 1.0 sits below the lower bound.
        let out = dispatch_out_of_range(&add_liquidity_action(10, 200, "1.0"));
        assert_eq!(out["outOfRange"], json!(true));
    }

    #[test]
    fn current_price_above_range_is_out_of_position() {
        // Range [-200, -10] → ~[0.9802, 0.9990]; price 1.0 sits above the upper bound.
        let out = dispatch_out_of_range(&add_liquidity_action(-200, -10, "1.0"));
        assert_eq!(out["outOfRange"], json!(true));
    }

    #[test]
    fn non_tick_range_is_never_out_of_position() {
        // Bin range carries no tick bounds at the policy layer → not a one-sided
        // tick order → not out of range.
        let params = json!({
            "action": {
                "params": { "kind": "concentrated_mint", "range": { "kind": "bin", "activeId": 8_388_608 } },
                "currentPrice": "1.0"
            }
        });
        let out = dispatch_out_of_range(&params);
        assert_eq!(out["outOfRange"], json!(false));
    }

    #[test]
    fn pooled_add_without_range_is_never_out_of_position() {
        let params = json!({
            "action": {
                "params": { "kind": "pooled", "minLpOut": "0x0" },
                "currentPrice": "2.0"
            }
        });
        let out = dispatch_out_of_range(&params);
        assert_eq!(out["outOfRange"], json!(false));
    }

    #[test]
    fn missing_current_price_is_bad_params() {
        let params = json!({
            "action": { "params": { "kind": "concentrated_mint", "range": { "kind": "tick", "tickLower": -100, "tickUpper": 100 } } }
        });
        let state = empty_state();
        let err = super::super::dispatch(
            "lp.range_out_of_position",
            &params,
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn exit_asymmetry_is_blocked_not_implemented() {
        // BLOCKED: no `current_price` on the remove path. Until injected, the
        // method must surface NotImplemented (server still boots).
        let params = json!({
            "chain_id": 1,
            "owner": "0x000000000000000000000000000000000000a01c",
            "action": { "params": { "kind": "concentrated_burn", "nftKey": {} } }
        });
        let state = empty_state();
        let err = super::super::dispatch("lp.exit_asymmetry", &params, &FactCtx { state: &state })
            .unwrap_err();
        assert!(matches!(err, FactError::NotImplemented(_)), "{err:?}");
    }
}
