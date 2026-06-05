//! PoC: a Hyperliquid `/exchange` action, modeled as the thin
//! `ActionBody::HyperliquidCore` variant and lowered, is BLOCKED
//! (`Verdict::Fail`) / flagged (`Verdict::Warn`) by the shipped-shape policies.
//!
//! This exercises the exact computation the extension's v2 path
//! (`evaluate_action_v2_json`) performs internally, minus the JSON/WASM
//! marshalling. The deny/warn conditions read only base context fields lowered
//! directly from the action (`context.venue.name`, `context.side`,
//! `context.leverage`, and the action UID), so no external facts and no
//! `results` map are required — the whole point of the thin HL Core variant is
//! that it needs NO live inputs.
//!
//! Pipeline mirrored here:
//!   HL Core action
//!     → `ActionBody::HyperliquidCore(...)`
//!     → `lower_action(body, meta, TxMeta{from, to})`
//!     → `compose_per_policy(manifest)`  (per-policy Cedar schema)
//!     → `PolicyEngine::build_from_per_policy([(policy, schema)])`
//!     → `engine.evaluate(...)` → `Verdict`.
#![allow(clippy::all, clippy::pedantic, clippy::nursery, missing_docs)]

use std::str::FromStr;

use serde_json::json;

use policy_engine::lowering_v2::{lower_action, TxMeta};
use policy_engine::policy::{PolicyEngine, Verdict};
use policy_engine::policy_rpc::ManifestV2;
use policy_engine::schema::compose_per_policy;

use policy_state::primitives::{Address, Decimal, Time};
use policy_transition::action::hyperliquid_core::{
    HlOrderAction, HlTwapOrderAction, HlUpdateLeverageAction, HlWithdrawAction,
    HyperliquidCoreAction,
};
use policy_transition::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};

const FROM: &str = "0x1111111111111111111111111111111111111111";
// HL Core actions have no on-chain settlement address; the SW supplies a
// sentinel `to`. Policies bind on `context.venue`/`side`/action, not `resource`.
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

/// Build a Hyperliquid order action. `is_buy=false` ⇒ short (sell) DIRECTION;
/// `reduce_only=true` ⇒ the order can only shrink an existing position (a
/// long-CLOSE when the side is short), so `positionEffect` lowers to "reduce".
fn order(is_buy: bool, size: &str, reduce_only: bool) -> ActionBody {
    ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
        asset_index: 0,
        symbol: Some("BTC".to_owned()),
        is_buy,
        price: Decimal::new("60000"),
        // Fractional size is preserved verbatim — the whole reason the HL Core
        // model uses `Decimal`, not `U256` (which rejects "0.1").
        size: Decimal::new(size),
        reduce_only,
        tif: "gtc".to_owned(),
    }))
}

/// Build a TWAP order action. `is_buy=false` ⇒ short; `reduce_only=true` ⇒ a
/// reduce-only TWAP (closes/shrinks → positionEffect "reduce").
fn twap(is_buy: bool, reduce_only: bool) -> ActionBody {
    ActionBody::HyperliquidCore(HyperliquidCoreAction::TwapOrder(HlTwapOrderAction {
        asset_index: 0,
        symbol: Some("BTC".to_owned()),
        is_buy,
        size: Decimal::new("10"),
        reduce_only,
        minutes: 30,
        randomize: false,
    }))
}

/// A minimal v2 manifest triggering on the given HL Core action tag.
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

/// Manifest whose trigger matches BOTH HL order tags, so `compose_per_policy`
/// synthesizes a schema containing HlOrder AND HlTwapOrder — required for a
/// policy scoped to `action in [HlOrder, HlTwapOrder]` to validate.
fn manifest_multi() -> ManifestV2 {
    serde_json::from_value(json!({
        "id": "hl-order-multi-guard",
        "schema_version": 2,
        "trigger": { "where": { "action.tag": { "in": ["hl_order", "hl_twap_order"] } } },
        "policy_rpc": [],
        "custom_context": { "fields": {} }
    }))
    .expect("ManifestV2 deserializes")
}

/// Evaluate one action against `policy` using the multi-tag (order + twap)
/// manifest, returning the `Verdict`.
fn evaluate_multi(body: &ActionBody, policy: &str) -> Verdict {
    let meta = hl_meta();
    let tx = TxMeta {
        from: FROM,
        to: TO_SENTINEL,
    };
    let lowered = lower_action(body, &meta, &tx).expect("lower_action");
    let schema = compose_per_policy(&manifest_multi()).expect("compose_per_policy");
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

// The shipped no-new-short shape, scoped to BOTH order types so a TWAP short
// cannot bypass it. Mirrors default_policies_v2/hl-no-short-perp.
const DENY_SHORT_MULTI: &str = "\
@id(\"hl/no-short-multi\")\n\
@severity(\"deny\")\n\
@reason(\"Opening a new short on Hyperliquid is blocked by policy\")\n\
forbid(principal, action in [HyperliquidCore::Action::\"HlOrder\", HyperliquidCore::Action::\"HlTwapOrder\"], resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" && context.positionEffect == \"open\" };\n";

// The deny policy under test: block opening NEW short exposure on Hyperliquid.
// `side == "short"` is the order DIRECTION (sell), which also covers reduce-only
// long-CLOSES; the `positionEffect == "open"` guard restricts the deny to new
// short exposure so a position exit is never blocked. Pure string-equality on
// base context fields — no decimal/has guards needed.
const DENY_SHORT: &str = "\
@id(\"hl/no-short\")\n\
@severity(\"deny\")\n\
@reason(\"Opening a new short on Hyperliquid is blocked by policy\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" && context.positionEffect == \"open\" };\n";

// Reduce-only lockdown: only position-CLOSING (reduce-only) orders may pass —
// every position-OPENING order is blocked regardless of side. Matches on the
// derived `positionEffect` alone.
const DENY_OPEN: &str = "\
@id(\"hl/reduce-only-mode\")\n\
@severity(\"deny\")\n\
@reason(\"Reduce-only mode — only position-closing orders are allowed\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlOrder\", resource)\n\
when { context.positionEffect == \"open\" };\n";

// A confirm (warn) policy for the fund-movement `HlWithdraw` action.
const CONFIRM_WITHDRAW: &str = "\
@id(\"hl/confirm-withdraw\")\n\
@severity(\"warn\")\n\
@reason(\"Withdrawing funds off Hyperliquid — confirm\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlWithdraw\", resource);\n";

// A threshold confirm (warn) policy for high leverage.
const CONFIRM_HIGH_LEVERAGE: &str = "\
@id(\"hl/confirm-high-leverage\")\n\
@severity(\"warn\")\n\
@reason(\"High leverage on Hyperliquid — confirm\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlUpdateLeverage\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.leverage > 20 };\n";

/// THE PROOF: opening a NEW short order (short side, NOT reduce-only) is BLOCKED
/// (`Verdict::Fail`).
#[test]
fn hyperliquid_opening_short_order_is_denied() {
    match evaluate(&order(false, "0.1", false), "hl_order", DENY_SHORT) {
        Verdict::Fail(matched) => assert!(
            matched.iter().any(|m| m.policy_id == "hl/no-short"),
            "expected the hl/no-short deny rule to match, got: {matched:?}"
        ),
        other => panic!("expected Verdict::Fail (blocked), got {other:?}"),
    }
}

/// THE FALSE-POSITIVE FIX: a reduce-only SHORT order is a long-CLOSE (position
/// exit), NOT new short exposure. The corrected deny (`positionEffect == "open"`)
/// must let it PASS. Before this fix, a naive `side == "short"` rule wrongly
/// blocked ~72% of "short" orders (reduce-only long-closes), trapping users in
/// their positions. This is the core behavioral guarantee of the (A+D) change.
#[test]
fn hyperliquid_reduce_only_short_is_long_close_and_passes() {
    assert_eq!(
        evaluate(&order(false, "0.1", true), "hl_order", DENY_SHORT),
        Verdict::Pass,
        "a reduce-only short (long-close / position exit) must NOT be blocked by a no-new-short deny"
    );
}

/// CONTROL: a LONG order (is_buy=true) is NOT blocked by the short-only deny.
/// Proves the deny is conditional on the order, not a blanket fail — and that a
/// fractional size flows through (no truncation, no deserialize failure).
#[test]
fn hyperliquid_long_order_passes() {
    assert_eq!(
        evaluate(&order(true, "0.1", false), "hl_order", DENY_SHORT),
        Verdict::Pass,
        "a long order must pass the short-only deny"
    );
}

/// Reduce-only lockdown: every OPENING order (long or short) is blocked, while
/// every reduce-only (closing) order passes — proving `positionEffect` cleanly
/// separates open-vs-close independent of side.
#[test]
fn reduce_only_lockdown_blocks_opens_allows_closes() {
    assert!(
        matches!(
            evaluate(&order(true, "0.1", false), "hl_order", DENY_OPEN),
            Verdict::Fail(_)
        ),
        "opening a long must be blocked under reduce-only lockdown"
    );
    assert!(
        matches!(
            evaluate(&order(false, "0.1", false), "hl_order", DENY_OPEN),
            Verdict::Fail(_)
        ),
        "opening a short must be blocked under reduce-only lockdown"
    );
    assert_eq!(
        evaluate(&order(true, "0.1", true), "hl_order", DENY_OPEN),
        Verdict::Pass,
        "a reduce-only buy (short-close) must pass reduce-only lockdown"
    );
    assert_eq!(
        evaluate(&order(false, "0.1", true), "hl_order", DENY_OPEN),
        Verdict::Pass,
        "a reduce-only sell (long-close) must pass reduce-only lockdown"
    );
}

/// TWAP COVERAGE: a no-new-short scoped to `action in [HlOrder, HlTwapOrder]`
/// blocks an opening TWAP short but passes a reduce-only TWAP (long-close) and a
/// TWAP long — closing the bypass where a short is submitted as a TWAP. The same
/// policy still blocks a plain opening order short.
#[test]
fn twap_short_is_covered_by_multi_action_no_short() {
    // Opening TWAP short → blocked.
    assert!(
        matches!(
            evaluate_multi(&twap(false, false), DENY_SHORT_MULTI),
            Verdict::Fail(_)
        ),
        "an opening TWAP short must be blocked"
    );
    // Reduce-only TWAP short (long-close) → passes (positionEffect == reduce).
    assert_eq!(
        evaluate_multi(&twap(false, true), DENY_SHORT_MULTI),
        Verdict::Pass,
        "a reduce-only TWAP short (long-close) must pass"
    );
    // TWAP long → passes.
    assert_eq!(
        evaluate_multi(&twap(true, false), DENY_SHORT_MULTI),
        Verdict::Pass,
        "a TWAP long must pass"
    );
    // Plain opening order short still blocked through the SAME multi-action policy.
    assert!(
        matches!(
            evaluate_multi(&order(false, "0.1", false), DENY_SHORT_MULTI),
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

/// D4: a high-leverage `updateLeverage` is flagged (`Verdict::Warn`); a modest
/// one passes — proves the threshold guard on `context.leverage`.
#[test]
fn hyperliquid_high_leverage_is_flagged_but_modest_passes() {
    let lev = |x: u32| {
        ActionBody::HyperliquidCore(HyperliquidCoreAction::UpdateLeverage(
            HlUpdateLeverageAction {
                asset_index: 0,
                symbol: Some("BTC".to_owned()),
                is_cross: true,
                leverage: x,
            },
        ))
    };
    assert!(
        matches!(
            evaluate(&lev(25), "hl_update_leverage", CONFIRM_HIGH_LEVERAGE),
            Verdict::Warn(_)
        ),
        "25x leverage must warn"
    );
    assert_eq!(
        evaluate(&lev(10), "hl_update_leverage", CONFIRM_HIGH_LEVERAGE),
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
        evaluate(&order(false, "0.1", false), "hl_order", ALLOW_ALL),
        Verdict::Pass
    );
}
