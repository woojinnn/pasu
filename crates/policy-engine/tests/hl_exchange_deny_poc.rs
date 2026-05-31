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

use simulation_reducer::action::hyperliquid_core::{
    HlOrderAction, HlUpdateLeverageAction, HlWithdrawAction, HyperliquidCoreAction,
};
use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
use simulation_state::primitives::{Address, Decimal, Time};

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

/// Build a Hyperliquid order action. `is_buy=false` ⇒ short.
fn order(is_buy: bool, size: &str) -> ActionBody {
    ActionBody::HyperliquidCore(HyperliquidCoreAction::Order(HlOrderAction {
        asset_index: 0,
        symbol: Some("BTC".to_owned()),
        is_buy,
        price: Decimal::new("60000"),
        // Fractional size is preserved verbatim — the whole reason the HL Core
        // model uses `Decimal`, not `U256` (which rejects "0.1").
        size: Decimal::new(size),
        reduce_only: false,
        tif: "gtc".to_owned(),
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

// The deny policy under test: block SHORT orders on Hyperliquid. Pure
// string-equality on base context fields — no decimal/has guards needed.
const DENY_SHORT: &str = "\
@id(\"hl/no-short\")\n\
@severity(\"deny\")\n\
@reason(\"Short orders on Hyperliquid are blocked by policy\")\n\
forbid(principal, action == HyperliquidCore::Action::\"HlOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" };\n";

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

/// THE PROOF: a Hyperliquid short order is BLOCKED (`Verdict::Fail`).
#[test]
fn hyperliquid_short_order_is_denied() {
    match evaluate(&order(false, "0.1"), "hl_order", DENY_SHORT) {
        Verdict::Fail(matched) => assert!(
            matched.iter().any(|m| m.policy_id == "hl/no-short"),
            "expected the hl/no-short deny rule to match, got: {matched:?}"
        ),
        other => panic!("expected Verdict::Fail (blocked), got {other:?}"),
    }
}

/// CONTROL: a LONG order (is_buy=true) is NOT blocked by the short-only deny.
/// Proves the deny is conditional on the order, not a blanket fail — and that a
/// fractional size flows through (no truncation, no deserialize failure).
#[test]
fn hyperliquid_long_order_passes() {
    assert_eq!(
        evaluate(&order(true, "0.1"), "hl_order", DENY_SHORT),
        Verdict::Pass,
        "a long order must pass the short-only deny"
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
        evaluate(&order(false, "0.1"), "hl_order", ALLOW_ALL),
        Verdict::Pass
    );
}
