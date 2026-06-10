//! PoC: a Hyperliquid `/exchange` order, decoded into the generic
//! `ActionBody::Perp(PlaceOrder)` model and lowered, is BLOCKED
//! (`Verdict::Fail`) / flagged (`Verdict::Warn`) by the shipped-shape policies.
//!
//! This exercises the exact computation the extension's v2 path
//! (`evaluate_action_v2_json`) performs internally, minus the JSON/WASM
//! marshalling. HL orders now decode to `Perp::PlaceOrder` (orderType
//! limit/stop/twap); the deny/warn conditions read only base context fields
//! (`context.venue.name`, `context.side`, `context.reduceOnly`,
//! `context.orderType.kind`, `context.newLeverage`), so no live inputs /
//! `results` map are required. `positionEffect == "open"` collapses to
//! `reduceOnly == false` (a non-reduce-only order opens/adds exposure). HL
//! `updateLeverage` decodes to `Perp::ChangeLeverage`; the withdraw
//! (`HlWithdraw`) HL Core action is unchanged.
//!
//! Pipeline mirrored here:
//!   HL order → `ActionBody::Perp(PlaceOrder)` → `lower_action(...)`
//!     → `compose_per_policy(manifest)` → `PolicyEngine::build_from_per_policy`
//!     → `engine.evaluate(...)` → `Verdict`.
#![allow(clippy::all, clippy::pedantic, clippy::nursery, missing_docs)]

use std::str::FromStr;

use serde_json::json;

use policy_engine::lowering_v2::{lower_action, TxMeta};
use policy_engine::policy::{PolicyEngine, Verdict};
use policy_engine::policy_rpc::ManifestV2;
use policy_engine::schema::compose_per_policy;

use policy_state::position::PerpSide;
use policy_state::primitives::{Address, ChainId, Decimal, MarketRef, Time, VenueRef};
use policy_transition::action::hyperliquid_core::{HlWithdrawAction, HyperliquidCoreAction};
use policy_transition::action::perp::{
    OrderType, PerpAction, PerpVenue, PlaceOrderAction, SizeSpec, TimeInForce,
};
use policy_transition::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};

const FROM: &str = "0x1111111111111111111111111111111111111111";
// HL actions have no on-chain settlement address; the SW supplies a sentinel
// `to`. Policies bind on `context.venue`/`side`/action, not `resource`.
const TO_SENTINEL: &str = "0x0000000000000000000000000000000000000000";

/// An off-chain-sig meta (HL orders are agent-signed, never an on-chain tx).
fn hl_meta() -> ActionMeta {
    ActionMeta {
        submitted_at: Time::from_unix(1_738_000_000),
        submitter: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
        nature: ActionNature::OffchainSig {
            domain: Eip712Domain {
                name: "Hyperliquid".to_owned(),
                version: Some("1".to_owned()),
                chain_id: None,
                verifying_contract: None,
                salt: None,
            },
            deadline: Time::from_unix(1_738_000_600),
            nonce_key: None,
        },
    }
}

/// Build a Hyperliquid `Perp::PlaceOrder` body. `is_buy=false` ⇒ short (sell)
/// DIRECTION; `reduce_only=true` ⇒ the order can only shrink existing exposure
/// (a long-CLOSE when the side is short). Fractional size flows through the
/// `base_decimal` SizeSpec verbatim (the representation HL orders carry).
fn place_order(is_buy: bool, size: &str, reduce_only: bool, order_type: OrderType) -> ActionBody {
    ActionBody::Perp(PerpAction::PlaceOrder(PlaceOrderAction {
        venue: PerpVenue::Hyperliquid {
            chain: ChainId::new("hyperliquid:mainnet"),
        },
        market: MarketRef {
            symbol: "BTC".to_owned(),
            venue: VenueRef::new("hyperliquid"),
        },
        side: if is_buy {
            PerpSide::Long
        } else {
            PerpSide::Short
        },
        size: SizeSpec::BaseDecimal {
            amount: Decimal::new(size),
        },
        reduce_only,
        order_type,
        live_inputs: None,
    }))
}

/// A limit order (the common HL order kind).
fn order(is_buy: bool, size: &str, reduce_only: bool) -> ActionBody {
    place_order(
        is_buy,
        size,
        reduce_only,
        OrderType::Limit {
            price: Decimal::new("60000"),
            time_in_force: TimeInForce::Gtc,
        },
    )
}

/// A TWAP order — the same `Perp::PlaceOrder` action with `orderType.kind ==
/// "twap"`, so a single-action no-short rule covers it (no bypass).
fn twap(is_buy: bool, reduce_only: bool) -> ActionBody {
    place_order(
        is_buy,
        "10",
        reduce_only,
        OrderType::Twap {
            duration_minutes: 30,
            randomize: false,
        },
    )
}

/// A minimal v2 manifest triggering on the given action tag.
fn manifest(tag: &str) -> ManifestV2 {
    serde_json::from_value(json!({
        "id": format!("{tag}-guard"),
        "schema_version": 2,
        "trigger": { "where": { "action.tag": { "eq": tag } } },
        "policy_rpc": [],
        "custom_context": { "fields": {} }
    }))
    .expect("ManifestV2 deserializes")
}

/// Lower one action + evaluate it against `policy` (whose manifest triggers on
/// `tag`), returning the `Verdict`.
fn evaluate(body: &ActionBody, tag: &str, policy: &str) -> Verdict {
    let meta = hl_meta();
    let tx = TxMeta {
        from: FROM,
        to: TO_SENTINEL,
    };
    let lowered = lower_action(body, &meta, &tx).expect("lower_action");
    let schema = compose_per_policy(&manifest(tag)).expect("compose_per_policy");
    let engine = PolicyEngine::build_from_per_policy(&[(policy.to_owned(), schema)])
        .expect("build_from_per_policy");
    engine
        .evaluate(
            &lowered.principal,
            &lowered.action_uid,
            &lowered.resource,
            &json!([]),
            &lowered.context,
        )
        .expect("evaluate")
}

// The shipped no-new-short shape. One `Perp::PlaceOrder` action covers BOTH a
// plain order and a TWAP, so a TWAP short cannot bypass it. Mirrors
// default_policies_v2/hl-no-short-perp.
const DENY_SHORT: &str = "\
@id(\"hl/no-short\")\n\
@severity(\"deny\")\n\
@reason(\"Opening a new short on Hyperliquid is blocked by policy\")\n\
forbid(principal, action == Perp::Action::\"PlaceOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" && context.reduceOnly == false };\n";

// Reduce-only lockdown: only position-CLOSING (reduce-only) orders may pass —
// every position-OPENING order is blocked regardless of side. Matches on the
// `reduceOnly` flag alone.
const DENY_OPEN: &str = "\
@id(\"hl/reduce-only-mode\")\n\
@severity(\"deny\")\n\
@reason(\"Reduce-only mode — only position-closing orders are allowed\")\n\
forbid(principal, action == Perp::Action::\"PlaceOrder\", resource)\n\
when { context.reduceOnly == false };\n";

// A confirm (warn) policy for the fund-movement `HlWithdraw` action (unchanged).
const CONFIRM_WITHDRAW: &str = "\
@id(\"hl/confirm-withdraw\")\n\
@severity(\"warn\")\n\
@reason(\"Withdrawing funds off Hyperliquid — confirm\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlWithdraw\", resource);\n";

// A threshold confirm (warn) policy for high leverage. HL updateLeverage
// decodes to the generic `Perp::ChangeLeverage`; `newLeverage` is a Cedar
// `decimal`, compared via `.greaterThan`. HL-scoped via the venue guard.
const CONFIRM_HIGH_LEVERAGE: &str = "\
@id(\"hl/confirm-high-leverage\")\n\
@severity(\"warn\")\n\
@reason(\"High leverage on Hyperliquid — confirm\")\n\
forbid(principal, action == Perp::Action::\"ChangeLeverage\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.newLeverage.greaterThan(decimal(\"20.0\")) };\n";

/// THE PROOF: opening a NEW short order (short side, NOT reduce-only) is BLOCKED
/// (`Verdict::Fail`).
#[test]
fn hyperliquid_opening_short_order_is_denied() {
    match evaluate(&order(false, "0.1", false), "place_order", DENY_SHORT) {
        Verdict::Fail(matched) => assert!(
            matched.iter().any(|m| m.policy_id == "hl/no-short"),
            "expected the hl/no-short deny rule to match, got: {matched:?}"
        ),
        other => panic!("expected Verdict::Fail (blocked), got {other:?}"),
    }
}

/// THE FALSE-POSITIVE FIX: a reduce-only SHORT order is a long-CLOSE (position
/// exit), NOT new short exposure. The deny (`reduceOnly == false`) must let it
/// PASS — a naive `side == "short"` rule would wrongly block reduce-only
/// long-closes, trapping users in their positions.
#[test]
fn hyperliquid_reduce_only_short_is_long_close_and_passes() {
    assert_eq!(
        evaluate(&order(false, "0.1", true), "place_order", DENY_SHORT),
        Verdict::Pass,
        "a reduce-only short (long-close / position exit) must NOT be blocked by a no-new-short deny"
    );
}

/// CONTROL: a LONG order (is_buy=true) is NOT blocked by the short-only deny.
/// Proves the deny is conditional on the order, not a blanket fail — and that a
/// fractional `base_decimal` size flows through (no deserialize failure).
#[test]
fn hyperliquid_long_order_passes() {
    assert_eq!(
        evaluate(&order(true, "0.1", false), "place_order", DENY_SHORT),
        Verdict::Pass,
        "a long order must pass the short-only deny"
    );
}

/// Reduce-only lockdown: every OPENING order (long or short) is blocked, while
/// every reduce-only (closing) order passes — proving `reduceOnly` cleanly
/// separates open-vs-close independent of side.
#[test]
fn reduce_only_lockdown_blocks_opens_allows_closes() {
    assert!(
        matches!(
            evaluate(&order(true, "0.1", false), "place_order", DENY_OPEN),
            Verdict::Fail(_)
        ),
        "opening a long must be blocked under reduce-only lockdown"
    );
    assert!(
        matches!(
            evaluate(&order(false, "0.1", false), "place_order", DENY_OPEN),
            Verdict::Fail(_)
        ),
        "opening a short must be blocked under reduce-only lockdown"
    );
    assert_eq!(
        evaluate(&order(true, "0.1", true), "place_order", DENY_OPEN),
        Verdict::Pass,
        "a reduce-only buy (short-close) must pass reduce-only lockdown"
    );
    assert_eq!(
        evaluate(&order(false, "0.1", true), "place_order", DENY_OPEN),
        Verdict::Pass,
        "a reduce-only sell (long-close) must pass reduce-only lockdown"
    );
}

/// TWAP COVERAGE: the unified `Perp::PlaceOrder` action means a no-new-short
/// rule blocks an opening TWAP short but passes a reduce-only TWAP (long-close)
/// and a TWAP long — closing the bypass where a short is submitted as a TWAP.
/// The SAME single-action policy still blocks a plain opening order short.
#[test]
fn twap_short_is_covered_by_unified_place_order_no_short() {
    // Opening TWAP short → blocked.
    assert!(
        matches!(
            evaluate(&twap(false, false), "place_order", DENY_SHORT),
            Verdict::Fail(_)
        ),
        "an opening TWAP short must be blocked"
    );
    // Reduce-only TWAP short (long-close) → passes.
    assert_eq!(
        evaluate(&twap(false, true), "place_order", DENY_SHORT),
        Verdict::Pass,
        "a reduce-only TWAP short (long-close) must pass"
    );
    // TWAP long → passes.
    assert_eq!(
        evaluate(&twap(true, false), "place_order", DENY_SHORT),
        Verdict::Pass,
        "a TWAP long must pass"
    );
    // Plain opening order short still blocked through the SAME policy.
    assert!(
        matches!(
            evaluate(&order(false, "0.1", false), "place_order", DENY_SHORT),
            Verdict::Fail(_)
        ),
        "a plain opening order short must still be blocked"
    );
}

/// D4: a fund-movement `withdraw3` is flagged for confirmation (`Verdict::Warn`)
/// — the genuinely-dangerous action class is guarded, not just orders.
#[test]
fn hyperliquid_withdraw_is_flagged_for_confirmation() {
    let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Withdraw(HlWithdrawAction {
        destination: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
        amount: Decimal::new("1000.50"),
    }));
    match evaluate(&body, "hl_withdraw", CONFIRM_WITHDRAW) {
        Verdict::Warn(matched) => assert!(
            matched.iter().any(|m| m.policy_id == "hl/confirm-withdraw"),
            "expected the confirm-withdraw warn rule to match, got: {matched:?}"
        ),
        other => panic!("expected Verdict::Warn (confirm), got {other:?}"),
    }
}

/// D4: a high-leverage change (HL `updateLeverage` → `Perp::ChangeLeverage`) is
/// flagged (`Verdict::Warn`); a modest one passes — proves the threshold guard
/// on the Cedar-`decimal` `context.newLeverage`.
#[test]
fn hyperliquid_high_leverage_is_flagged_but_modest_passes() {
    use policy_transition::action::perp::ChangeLeverageAction;
    let lev = |x: &str| {
        ActionBody::Perp(PerpAction::ChangeLeverage(ChangeLeverageAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::new("hyperliquid:mainnet"),
            },
            market: MarketRef {
                symbol: "BTC".to_owned(),
                venue: VenueRef::new("hyperliquid"),
            },
            new_leverage: Decimal::new(x),
            live_inputs: None,
        }))
    };
    assert!(
        matches!(
            evaluate(&lev("25"), "change_leverage", CONFIRM_HIGH_LEVERAGE),
            Verdict::Warn(_)
        ),
        "25x leverage must warn"
    );
    assert_eq!(
        evaluate(&lev("10"), "change_leverage", CONFIRM_HIGH_LEVERAGE),
        Verdict::Pass,
        "10x leverage must pass the >20x confirm"
    );
}

/// CONTROL: no deny policy installed ⇒ baseline Pass. Blocking requires an
/// explicit deny; guards against a false-green where everything "fails".
#[test]
fn no_policy_passes_baseline() {
    const ALLOW_ALL: &str =
        "@id(\"noop\")\n@severity(\"warn\")\npermit(principal, action, resource);\n";
    assert_eq!(
        evaluate(&order(false, "0.1", false), "place_order", ALLOW_ALL),
        Verdict::Pass
    );
}
