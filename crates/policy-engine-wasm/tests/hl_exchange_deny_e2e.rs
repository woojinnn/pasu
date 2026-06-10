//! E2E: a Hyperliquid `/exchange` action is evaluated through the **literal
//! extension entry point** — `evaluate_action_v2_json` — using the thin
//! `ActionBody::HyperliquidCore` model.
//!
//! This feeds the EXACT JSON envelope the browser extension's service worker
//! sends — `{ action, meta, tx, bundles, results }` — into
//! `evaluate_action_v2_json(input_json) -> String` and parses the returned
//! `{ ok, data: { verdict } }`. The `action` JSON is byte-for-byte the shape the
//! TS converter (`hl-order-to-action.ts`) emits, so a serde drift on either side
//! fails loudly here instead of silently fail-closing at runtime.
//!
//! Run: `cargo test -p policy-engine-wasm --test hl_exchange_deny_e2e`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use serde_json::{json, Value};

use policy_engine_wasm::evaluate_action_v2_json;

/// The off-chain-sig meta the converter emits.
fn hl_meta() -> Value {
    json!({
        "submitted_at": 1_738_000_000u64,
        "submitter": "0x000000000000000000000000000000000000a01c",
        "nature": {
            "kind": "offchain_sig",
            "domain": { "name": "Hyperliquid", "version": "1" },
            "deadline": 1_738_000_600u64
        }
    })
}

/// A Hyperliquid limit order JSON — the exact `hl-order-to-action.ts` shape: a
/// generic `Perp::PlaceOrder` body (`{ domain:"perp", action:"place_order", … }`)
/// with the HL venue, a fractional `base_decimal` size, and `orderType.kind ==
/// "limit"`. `is_buy=false` ⇒ short; non-reduce-only (opens exposure).
fn order_action(is_buy: bool, size: &str) -> Value {
    json!({
        "domain": "perp",
        "action": "place_order",
        "venue": { "name": "hyperliquid", "chain": "hyperliquid:mainnet" },
        "market": { "symbol": "BTC", "venue": { "name": "hyperliquid" } },
        "side": if is_buy { "long" } else { "short" },
        "size": { "kind": "base_decimal", "amount": size },
        "reduce_only": false,
        "order_type": { "kind": "limit", "price": "60000", "time_in_force": { "kind": "gtc" } }
    })
}

/// A Hyperliquid updateLeverage JSON — the `hl-order-to-action.ts` shape: a
/// generic `Perp::ChangeLeverage` body (`newLeverage` is a decimal string).
fn update_leverage_action(leverage: i64) -> Value {
    json!({
        "domain": "perp",
        "action": "change_leverage",
        "venue": { "name": "hyperliquid", "chain": "hyperliquid:mainnet" },
        "market": { "symbol": "BTC", "venue": { "name": "hyperliquid" } },
        "new_leverage": leverage.to_string()
    })
}

/// A NON-Hyperliquid (`gmx_v2`) `Perp::ChangeLeverage` body — the cross-venue
/// control: the HL-scoped leverage policies must NOT fire on it (venue guard).
fn gmx_leverage_action(leverage: i64) -> Value {
    json!({
        "domain": "perp",
        "action": "change_leverage",
        "venue": { "name": "gmx_v2", "chain": "eip155:42161" },
        "market": { "symbol": "ETH", "venue": { "name": "gmx_v2" } },
        "new_leverage": leverage.to_string()
    })
}

/// A manifest triggering on a given HL Core action tag (no policy-RPC; the HL
/// deny/confirm rules read only base context).
fn manifest(tag: &str) -> Value {
    json!({
        "id": format!("{tag}-guard"),
        "schema_version": 2,
        "trigger": { "where": { "action.tag": { "eq": tag } } },
        "policy_rpc": [],
        "custom_context": { "fields": {} }
    })
}

const DENY_SHORT: &str = "\
@id(\"hl/no-short\")\n\
@severity(\"deny\")\n\
@reason(\"Opening a new short on Hyperliquid is blocked by policy\")\n\
forbid(principal, action == Perp::Action::\"PlaceOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context.side == \"short\" && context.reduceOnly == false };\n";

/// Assemble the `EvaluateActionInput` envelope and run it through the entry
/// point. Returns the parsed output envelope.
fn run(action: Value, bundles: Value) -> Value {
    let input = json!({
        "action": action,
        "meta": hl_meta(),
        "tx": {
            "chain_id": "hl-mainnet",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x0000000000000000000000000000000000000000"
        },
        "bundles": bundles,
        "results": {}
    });
    serde_json::from_str(&evaluate_action_v2_json(input.to_string()))
        .expect("entry point returns JSON")
}

/// Read a shipped seed bundle (`policy.cedar` + `manifest.json`) verbatim — the
/// SAME artifact `copy-default-policies.js` ships into the extension.
fn seed_bundle(id: &str) -> Value {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("policy-engine")
        .join("tests")
        .join("fixtures")
        .join("default_policies_v2")
        .join(id);
    let policy = std::fs::read_to_string(dir.join("policy.cedar")).expect("read seed policy.cedar");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
            .expect("seed manifest.json parses");
    json!({ "policy": policy, "manifest": manifest })
}

/// THE PROOF (entry point): a Hyperliquid SHORT order returns a `fail` verdict.
#[test]
fn hyperliquid_short_order_denied_through_entry_point() {
    let parsed = run(
        order_action(false, "0.1"),
        json!([{ "policy": DENY_SHORT, "manifest": manifest("place_order") }]),
    );
    assert_eq!(parsed["ok"], true, "envelope ok: {parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "short order must be DENIED: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/no-short",
        "the deny rule must be the matched policy: {parsed}"
    );
}

/// CONTROL: a LONG order passes the short-only deny. Also proves a fractional
/// `size` ("0.1") deserializes cleanly (Decimal, not U256) — NOT a parse-failure
/// `__system__`/`__engine` deny.
#[test]
fn hyperliquid_long_order_passes_through_entry_point() {
    let parsed = run(
        order_action(true, "0.1"),
        json!([{ "policy": DENY_SHORT, "manifest": manifest("place_order") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "a long order must PASS the short-only deny: {parsed}"
    );
}

/// CONTROL: no bundle ⇒ baseline pass (blocking requires an explicit policy).
#[test]
fn no_bundle_passes_baseline_through_entry_point() {
    let parsed = run(order_action(false, "0.1"), json!([]));
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "no bundle ⇒ baseline pass: {parsed}"
    );
}

/// SHIPPED-SEED PROOF: the actual default bundle that ships in the extension
/// (`hl-no-short-perp/{policy.cedar,manifest.json}`, copied into
/// `public/default-policies/policy-set-v2.json`) DENIES a Hyperliquid short
/// order through `evaluate_action_v2_json`. This pins the SHIPPED policy ↔ the
/// HyperliquidCore action UID wiring (regression guard for the stale-fixture bug).
#[test]
fn shipped_seed_policy_denies_hyperliquid_short() {
    let parsed = run(
        order_action(false, "0.1"),
        json!([seed_bundle("hl-no-short-perp")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "the SHIPPED seed policy must DENY a HL short: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-no-short-perp",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// SHIPPED-SEED CONTROL: the shipped short-deny does NOT block a LONG order —
/// proves the retargeted policy is conditional on side, not a blanket fail.
#[test]
fn shipped_seed_policy_allows_hyperliquid_long() {
    let parsed = run(
        order_action(true, "0.1"),
        json!([seed_bundle("hl-no-short-perp")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "the shipped short-deny must let a long order PASS: {parsed}"
    );
}

/// D4 SHIPPED-SEED PROOF: the shipped `hl-confirm-withdraw` bundle FLAGS a
/// `withdraw3` for confirmation (`warn`) through the entry point — the
/// fund-movement action class is guarded, not just orders.
#[test]
fn shipped_seed_policy_confirms_hyperliquid_withdraw() {
    let withdraw = json!({
        "domain": "hyperliquid_core",
        "action": "hl_withdraw",
        "destination": "0x000000000000000000000000000000000000dead",
        "amount": "1000.5"
    });
    let parsed = run(withdraw, json!([seed_bundle("hl-confirm-withdraw")]));
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "the shipped confirm-withdraw policy must WARN: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-withdraw",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// PHASE-2 SHIPPED-SEED PROOF (coverage preserved across a schema deletion): a
/// `usdSend` now lowers to `Token::Action::"Erc20Transfer"` (its own HL schema is
/// deleted), and the shipped `hl-confirm-usd-send` seed — rewritten onto
/// `Erc20Transfer` but KEEPING its `hl_usd_send` trigger — still FIRES `warn`
/// end-to-end. This is the firing oracle conformance tests cannot provide.
#[test]
fn shipped_seed_policy_confirms_hyperliquid_usd_send() {
    let usd_send = json!({
        "domain": "hyperliquid_core",
        "action": "hl_usd_send",
        "destination": "0x000000000000000000000000000000000000dead",
        "amount": "250"
    });
    let parsed = run(usd_send, json!([seed_bundle("hl-confirm-usd-send")]));
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "usd_send must WARN via the rewritten Erc20Transfer policy: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-usd-send",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// PHASE-2 PROOF: a `spotSend` lowers to `Token::Action::"Erc20Transfer"` and a
/// transfer-shaped policy selected by the `hl_spot_send` trigger FIRES — the
/// migrated lowering + tag-based selection preserve HL spot-send coverage.
#[test]
fn hyperliquid_spot_send_confirmed_via_erc20_transfer() {
    const CONFIRM_ERC20: &str = "\
@id(\"hl/spot-send-confirm\")\n\
@severity(\"warn\")\n\
@reason(\"Sending a spot token off Hyperliquid — confirm the destination\")\n\
forbid(principal, action == Token::Action::\"Erc20Transfer\", resource);\n";
    let spot_send = json!({
        "domain": "hyperliquid_core",
        "action": "hl_spot_send",
        "destination": "0x000000000000000000000000000000000000dead",
        "token": "USDC:0xc1fb593aeffbeb02f85e0308e9956a90",
        "amount": "500.25"
    });
    let parsed = run(
        spot_send,
        json!([{ "policy": CONFIRM_ERC20, "manifest": manifest("hl_spot_send") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "spot_send must WARN via Erc20Transfer: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/spot-send-confirm",
        "{parsed}"
    );
}

/// PHASE-3 PROOF: a `cWithdraw` lowers to `Staking::Action::"Redeem"` and a
/// staking policy selected by the `hl_c_withdraw` trigger FIRES — the HYPE
/// unstake coverage survives deleting the HL c_withdraw schema.
#[test]
fn hyperliquid_c_withdraw_confirmed_via_staking_redeem() {
    const CONFIRM_REDEEM: &str = "\
@id(\"hl/c-withdraw-confirm\")\n\
@severity(\"warn\")\n\
@reason(\"Withdrawing HYPE from Hyperliquid staking — confirm the amount\")\n\
forbid(principal, action == Staking::Action::\"Redeem\", resource);\n";
    let c_withdraw = json!({
        "domain": "hyperliquid_core",
        "action": "hl_c_withdraw",
        "wei": "100000000000"
    });
    let parsed = run(
        c_withdraw,
        json!([{ "policy": CONFIRM_REDEEM, "manifest": manifest("hl_c_withdraw") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "c_withdraw must WARN via Staking::Redeem: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/c-withdraw-confirm",
        "{parsed}"
    );
}

/// D4 SHIPPED-SEED PROOF: the shipped `hl-confirm-high-leverage` bundle FLAGS a
/// high-leverage `updateLeverage` (`warn`) — closes the D4 gap where leverage
/// changes shipped no policy.
#[test]
fn shipped_seed_policy_confirms_high_leverage() {
    let parsed = run(
        update_leverage_action(25),
        json!([seed_bundle("hl-confirm-high-leverage")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "25x leverage must WARN: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-high-leverage",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

/// CONTROL: a modest leverage change (≤ 20x) is NOT flagged by the high-leverage
/// confirm — proves the guard is threshold-conditional, not a blanket warn.
#[test]
fn shipped_seed_policy_allows_modest_leverage() {
    let parsed = run(
        update_leverage_action(10),
        json!([seed_bundle("hl-confirm-high-leverage")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "10x leverage must PASS the >20x confirm: {parsed}"
    );
}

/// CROSS-VENUE SCOPE: the same high (25x) leverage on a NON-Hyperliquid venue
/// (`gmx_v2`) PASSES — `Perp::ChangeLeverage` is generic, so the HL-confirm
/// policy's `context.venue.name == "hyperliquid"` guard keeps it HL-scoped (it
/// does not fire on other perp venues).
#[test]
fn shipped_seed_policy_high_leverage_does_not_fire_off_hyperliquid() {
    let parsed = run(
        gmx_leverage_action(25),
        json!([seed_bundle("hl-confirm-high-leverage")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "25x leverage on gmx_v2 must PASS the HL-scoped confirm: {parsed}"
    );
}

/// A HyperliquidCore `hl_unknown` catch-all action JSON — the
/// `hl-order-to-action.ts` shape for an `/exchange` action with no explicit
/// model.
fn unknown_action(action_type: &str) -> Value {
    json!({
        "domain": "hyperliquid_core",
        "action": "hl_unknown",
        "action_type": action_type
    })
}

/// COVERAGE-GAP PROOF: an `/exchange` action we do not model
/// (`convertToMultiSigUser`) reaches the engine as `hl_unknown` and a deny rule
/// BLOCKS it — proving an unmodeled action can be gated, not silently allowed.
#[test]
fn unknown_hl_action_can_be_denied() {
    const DENY_UNKNOWN: &str = "\
@id(\"hl/deny-unknown\")\n\
@severity(\"deny\")\n\
@reason(\"Unrecognized Hyperliquid action blocked by policy\")\n\
forbid(principal, action == Core::Action::\"Unknown\", resource);\n";
    let parsed = run(
        unknown_action("convertToMultiSigUser"),
        json!([{ "policy": DENY_UNKNOWN, "manifest": manifest("hl_unknown") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "an unmodeled HL action must be DENIABLE via hl_unknown: {parsed}"
    );
}

/// FUND-MOVEMENT PROOF: a modeled `sendToEvmWithData` (bridge a token to an
/// arbitrary EVM recipient with calldata — the highest-risk fund movement) is
/// DENIED by a policy scoping on the recipient. Proves the P2 fund-movement
/// surface reaches the engine with its fields intact.
#[test]
fn send_to_evm_with_data_can_be_denied_on_recipient() {
    const DENY_BRIDGE: &str = "\
@id(\"hl/deny-evm-bridge\")\n\
@severity(\"deny\")\n\
@reason(\"Bridging funds to an unapproved EVM recipient is blocked\")\n\
forbid(principal, action == Core::Action::\"Unknown\", resource)\n\
when { context.target == \"0x000000000000000000000000000000000000dead\" };\n";
    let action = json!({
        "domain": "hyperliquid_core",
        "action": "hl_send_to_evm_with_data",
        "token": "USDC",
        "amount": "1000",
        "source_dex": "",
        "destination_recipient": "0x000000000000000000000000000000000000dead",
        "data": "0xdeadbeef"
    });
    let parsed = run(
        action,
        json!([{ "policy": DENY_BRIDGE, "manifest": manifest("hl_send_to_evm_with_data") }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "fail",
        "a bridge to the denied recipient must be BLOCKED: {parsed}"
    );
}

/// SHIPPED-SEED PROOF: the shipped `hl-confirm-unknown` bundle FLAGS an unmodeled
/// action for confirmation (`warn`) through the entry point.
#[test]
fn shipped_seed_policy_confirms_unknown_hl_action() {
    let parsed = run(
        unknown_action("perpDeploy"),
        json!([seed_bundle("hl-confirm-unknown")]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "the shipped confirm-unknown policy must WARN: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl-confirm-unknown",
        "matched policy must be the shipped seed id: {parsed}"
    );
}

// ── Order-time effective leverage (host-injected `account_leverage`) ─────────
//
// The order wire carries NO leverage — it is per-(user,asset) account state the
// venue applies at fill. The SW resolves it from `activeAssetData` and injects
// `account_leverage` (market symbol → leverage); the lowering fills the
// optional `context.leverage` Long. A `context has leverage` guard keeps the
// policy DORMANT (not over-blocking) when the host could not resolve it.

// Covers BOTH plain orders AND TWAP orders: they are now the SAME
// `Perp::PlaceOrder` action (orderType limit/twap), so one rule cannot be
// evaded by routing exposure through a TWAP. `PlaceOrderContext` carries the
// optional host-enriched `leverage?: Long`.
const WARN_HIGH_LEVERAGE_ORDER: &str = "\
@id(\"hl/order-high-leverage\")\n\
@severity(\"warn\")\n\
@reason(\"Opening a Hyperliquid order at effective leverage above 20x\")\n\
forbid(principal, action == Perp::Action::\"PlaceOrder\", resource)\n\
when { context.venue.name == \"hyperliquid\" && context has leverage && context.leverage > 20 };\n";

/// A Hyperliquid TWAP order JSON — the `hl-order-to-action.ts` shape: the same
/// `Perp::PlaceOrder` body with `orderType.kind == "twap"`.
fn twap_order_action(is_buy: bool, size: &str) -> Value {
    json!({
        "domain": "perp",
        "action": "place_order",
        "venue": { "name": "hyperliquid", "chain": "hyperliquid:mainnet" },
        "market": { "symbol": "BTC", "venue": { "name": "hyperliquid" } },
        "side": if is_buy { "long" } else { "short" },
        "size": { "kind": "base_decimal", "amount": size },
        "reduce_only": false,
        "order_type": { "kind": "twap", "duration_minutes": 30, "randomize": true }
    })
}

/// Manifest triggering on `place_order` — plain orders AND TWAP orders are now
/// the same `Perp::PlaceOrder` action, so a single-tag manifest composes the one
/// schema both order kinds share (no `action in [...]` multi-tag needed).
fn order_family_manifest() -> Value {
    json!({
        "id": "hl-order-family-guard",
        "schema_version": 2,
        "trigger": { "where": { "action.tag": { "eq": "place_order" } } },
        "policy_rpc": [],
        "custom_context": { "fields": {} }
    })
}

/// Like [`run`] but with the host-injected `account_leverage` map the SW adds
/// for the venue path (market symbol → effective leverage).
fn run_with_leverage(action: Value, bundles: Value, account_leverage: Value) -> Value {
    let input = json!({
        "action": action,
        "meta": hl_meta(),
        "tx": {
            "chain_id": "hl-mainnet",
            "from": "0x1111111111111111111111111111111111111111",
            "to": "0x0000000000000000000000000000000000000000"
        },
        "bundles": bundles,
        "results": {},
        "account_leverage": account_leverage
    });
    serde_json::from_str(&evaluate_action_v2_json(input.to_string()))
        .expect("entry point returns JSON")
}

/// ORDER-TIME LEVERAGE PROOF: with injected `account_leverage` (the SW
/// `activeAssetData` lookup), an order on asset_index 0 at 26x trips the
/// order-leverage warn — closing the gap where the order wire carries no
/// leverage. This is the live-verified 26x case, now enforced at ORDER time.
#[test]
fn hl_order_high_leverage_warns_when_injected() {
    let parsed = run_with_leverage(
        order_action(true, "0.1"),
        json!([{ "policy": WARN_HIGH_LEVERAGE_ORDER, "manifest": order_family_manifest() }]),
        json!({ "BTC": 26 }),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "a 26x order must WARN at order time: {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/order-high-leverage",
        "{parsed}"
    );
}

/// CONTROL (best-effort dormancy): WITHOUT injected leverage the same policy is
/// DORMANT (the `context has leverage` guard short-circuits) — a transient
/// info-fetch miss must NOT over-block, so the order PASSES.
#[test]
fn hl_order_high_leverage_dormant_without_injection() {
    let parsed = run(
        order_action(true, "0.1"),
        json!([{ "policy": WARN_HIGH_LEVERAGE_ORDER, "manifest": order_family_manifest() }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "no leverage injected ⇒ policy dormant ⇒ pass (no over-block): {parsed}"
    );
}

/// CONTROL (threshold): injected leverage at the threshold (20x, NOT > 20) does
/// NOT warn — proves the guard is threshold-conditional, not firing on the mere
/// presence of the field.
#[test]
fn hl_order_modest_leverage_passes_when_injected() {
    let parsed = run_with_leverage(
        order_action(true, "0.1"),
        json!([{ "policy": WARN_HIGH_LEVERAGE_ORDER, "manifest": order_family_manifest() }]),
        json!({ "BTC": 20 }),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "20x (not > 20) must PASS: {parsed}"
    );
}

/// TWAP BYPASS CLOSED: the same high-leverage exposure routed through a TWAP
/// (a first-class HL UI order type) now ALSO trips the order-leverage warn —
/// a TWAP is the same `Perp::PlaceOrder` action (orderType "twap"), so it
/// cannot evade an order-leverage cap.
#[test]
fn hl_twap_high_leverage_warns_when_injected() {
    let parsed = run_with_leverage(
        twap_order_action(true, "10"),
        json!([{ "policy": WARN_HIGH_LEVERAGE_ORDER, "manifest": order_family_manifest() }]),
        json!({ "BTC": 26 }),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "warn",
        "a 26x TWAP must WARN (bypass closed): {parsed}"
    );
    assert_eq!(
        parsed["data"]["verdict"]["matched"][0]["policy_id"], "hl/order-high-leverage",
        "{parsed}"
    );
}

/// CONTROL: a TWAP with no injected leverage stays dormant (best-effort omit) —
/// proves the field is the gate, not the action type.
#[test]
fn hl_twap_high_leverage_dormant_without_injection() {
    let parsed = run(
        twap_order_action(true, "10"),
        json!([{ "policy": WARN_HIGH_LEVERAGE_ORDER, "manifest": order_family_manifest() }]),
    );
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(
        parsed["data"]["verdict"]["kind"], "pass",
        "TWAP with no injected leverage must be dormant → pass: {parsed}"
    );
}
