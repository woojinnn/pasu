//! Decode golden for the new `lido/{steth,wsteth}/permit` EIP-2612 manifests.
//!
//! These manifests are the registry-side prerequisite that ACTIVATES the P3
//! `approval-drain-shield` permit policies (`steth-wsteth-unlimited-permit-deny`,
//! `steth-wsteth-permit-non-allowlisted-warn`). Before they existed the only
//! `erc20_permit` route in the registry was pinned to USDC, so a stETH/wstETH
//! permit never decoded → the permit SETs were dormant (could never match).
//!
//! Both EIP-2612 surfaces must decode to `Token::Erc20Permit` with the
//! policy-relevant fields bound:
//!   - typed-data SIGN route  (the pre-sign shield: `eth_signTypedData`)
//!   - on-chain `permit()` call route (selector 0xd505accf, the rarer tx form)
//! with `amount = $args.value` and `spender = $args.spender` so the static Cedar
//! comparisons (`amount == U256::MAX`, `spender ∉ allowlist`) can fire.
//!
//! Install + route on the same thread (WASM v3 install state is thread-local).

use policy_engine_integration_tests::harness::{self, adapters};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const STETH: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
const WSTETH: &str = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0";
/// A spender NOT in the P3 default allowlist {Permit2, WithdrawalQueue, wstETH}.
const EVIL_SPENDER: &str = "0x00000000000000000000000000000000deadbeef";
/// uint256 MAX — the canonical "unlimited" permit value the deny SET matches.
const MAX_HEX: &str = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// Recursively find the first `"<field>": "<string>"` entry in a JSON value.
fn find_string_field(v: &Value, field: &str) -> Option<String> {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(s)) = m.get(field) {
                return Some(s.clone());
            }
            m.values().find_map(|x| find_string_field(x, field))
        }
        Value::Array(a) => a.iter().find_map(|x| find_string_field(x, field)),
        _ => None,
    }
}

/// Parse a serialized U256 amount (`0x`-hex or bare decimal) to `u128`.
fn parse_u256_str(s: &str) -> u128 {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16).expect("hex amount parses")
    } else {
        t.parse::<u128>().expect("decimal amount parses")
    }
}

/// 32-byte (64-hex) left-padded form of a 20-byte address.
fn word_addr(a: &str) -> String {
    format!("{:0>64}", a.trim_start_matches("0x"))
}

/// ABI-encode `permit(owner,spender,value,deadline,v,r,s)` (selector 0xd505accf).
fn permit_calldata(owner: &str, spender: &str, value_hex64: &str, deadline: u64) -> String {
    format!(
        "0xd505accf{owner}{spender}{value}{deadline}{v}{r}{s}",
        owner = word_addr(owner),
        spender = word_addr(spender),
        value = value_hex64,
        deadline = format!("{deadline:064x}"),
        v = format!("{:064x}", 27u64),
        r = "11".repeat(32),
        s = "22".repeat(32),
    )
}

/// EIP-2612 `Permit` typed-data message (owner,spender,value,nonce,deadline).
fn permit_message(owner: &str, spender: &str, value: &str) -> Value {
    serde_json::json!({
        "owner": owner,
        "spender": spender,
        "value": value,
        "nonce": "0",
        "deadline": "9999999999"
    })
}

fn assert_erc20_permit(env: &Value, want_spender: &str, want_token: &str) {
    assert_eq!(
        env.get("ok").and_then(Value::as_bool),
        Some(true),
        "permit route did not succeed (dormant / route miss?): {env}"
    );
    let action = find_string_field(env, "action").expect("decoded body carries an action tag");
    assert_eq!(
        action, "erc20_permit",
        "must decode to Token::Erc20Permit: {env}"
    );
    let spender = find_string_field(env, "spender").expect("permit body carries a spender");
    assert_eq!(
        spender, want_spender,
        "spender must bind $args.spender: {env}"
    );
    // First "address" in the body is `token.key.address` (the permit'd token).
    let token = find_string_field(env, "address").expect("permit body carries a token address");
    assert_eq!(
        token, want_token,
        "token.key.address must be the permit'd token: {env}"
    );
}

#[test]
fn steth_permit_onchain_decodes_erc20_permit_unlimited() {
    let _surface = adapters::load_and_install().expect("install local surface");
    let calldata = permit_calldata(EVIL_SPENDER, EVIL_SPENDER, &"f".repeat(64), 9_999_999_999);
    let env = harness::route::route_calldata(1, STETH, "0xd505accf", &calldata, "0");
    assert_erc20_permit(&env, EVIL_SPENDER, STETH);
    let amount = find_string_field(&env, "amount").expect("amount present");
    assert_eq!(
        amount, MAX_HEX,
        "unlimited permit value must lower to U256::MAX hex: {env}"
    );
}

#[test]
fn steth_permit_typed_data_decodes_erc20_permit_unlimited() {
    let _surface = adapters::load_and_install().expect("install local surface");
    let message = permit_message(EVIL_SPENDER, EVIL_SPENDER, MAX_HEX);
    let env = harness::route::route_typed_data(
        1,
        STETH,
        "Permit",
        None,
        Some("Liquid staked Ether 2.0"),
        &message,
    );
    assert_erc20_permit(&env, EVIL_SPENDER, STETH);
    let amount = find_string_field(&env, "amount").expect("amount present");
    assert_eq!(
        amount, MAX_HEX,
        "unlimited permit value must lower to U256::MAX hex: {env}"
    );
}

#[test]
fn wsteth_permit_typed_data_decodes_erc20_permit_bounded() {
    let _surface = adapters::load_and_install().expect("install local surface");
    // A bounded, non-allowlisted permit → the warn SET's target (amount ∉ {0, MAX}).
    let bounded: u128 = 1_000_000_000_000_000_000; // 1 wstETH
    let message = permit_message(EVIL_SPENDER, EVIL_SPENDER, &bounded.to_string());
    let env = harness::route::route_typed_data(
        1,
        WSTETH,
        "Permit",
        None,
        Some("Wrapped liquid staked Ether 2.0"),
        &message,
    );
    assert_erc20_permit(&env, EVIL_SPENDER, WSTETH);
    let amount = find_string_field(&env, "amount").expect("amount present");
    assert_eq!(
        parse_u256_str(&amount),
        bounded,
        "bounded value must bind $args.value: {env}"
    );
    assert_ne!(
        amount, MAX_HEX,
        "bounded permit must NOT read as unlimited: {env}"
    );
}

#[test]
fn wsteth_permit_onchain_decodes_erc20_permit_unlimited() {
    let _surface = adapters::load_and_install().expect("install local surface");
    let calldata = permit_calldata(EVIL_SPENDER, EVIL_SPENDER, &"f".repeat(64), 9_999_999_999);
    let env = harness::route::route_calldata(1, WSTETH, "0xd505accf", &calldata, "0");
    assert_erc20_permit(&env, EVIL_SPENDER, WSTETH);
    let amount = find_string_field(&env, "amount").expect("amount present");
    assert_eq!(
        amount, MAX_HEX,
        "unlimited permit value must lower to U256::MAX hex: {env}"
    );
}

// ── End-to-end decode → real-Cedar verdict over the P3 preset policies ───────
//
// The decode tests above stop at the ActionBody. These two carry a stETH/wstETH
// permit ALL the way through `evaluate_action_v2_json` against the actual P3
// `approval-drain-shield` policies — proving the two formerly-DORMANT permit
// SETs now fire. The preset lives at repo root and is gitignored, so it is read
// at runtime and the test SKIPS (does not fail) when absent (mirrors
// `lido_preset_compile_gate`'s skip-if-absent).

/// Load `(policy.cedar, manifest.json)` for a P3 SET, or `None` if the gitignored
/// preset tree is absent (fresh clone) → caller skips.
fn p3_set(set: &str) -> Option<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../presets/liquid-staking-lido/p3-approval-drain-shield")
        .join(set);
    let policy = fs::read_to_string(dir.join("policy.cedar")).ok()?;
    let manifest_raw = fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: Value = serde_json::from_str(&manifest_raw).expect("P3 manifest parses");
    Some((policy, manifest))
}

/// Evaluate a decoded typed-data permit `env` against one P3 policy bundle.
fn verdict_over(env: &Value, policy: &str, manifest: &Value, verifying_contract: &str) -> Value {
    let action = env
        .pointer("/data/actions/0/body")
        .expect("route env carries data.actions[0].body");
    let meta = env
        .pointer("/data/actions/0/meta")
        .expect("route env carries data.actions[0].meta");
    let eval_input = serde_json::json!({
        "action": action,
        "meta": meta,
        "tx": {
            "chain_id": "eip155:1",
            "from": "0x000000000000000000000000000000000000aaaa",
            "to": verifying_contract,
        },
        "bundles": [{ "policy": policy, "manifest": manifest }],
        "results": {},
    });
    let verdict_env = harness::route::evaluate_action(&eval_input);
    assert_eq!(
        verdict_env.get("ok").and_then(Value::as_bool),
        Some(true),
        "evaluate_action_v2_json did not return ok: {verdict_env}"
    );
    verdict_env["data"]["verdict"].clone()
}

fn matched_has(verdict: &Value, policy_id: &str) -> bool {
    verdict["matched"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .any(|m| m.get("policy_id").and_then(Value::as_str) == Some(policy_id))
        })
        .unwrap_or(false)
}

#[test]
fn steth_unlimited_permit_yields_deny_verdict() {
    let Some((policy, manifest)) = p3_set("steth-wsteth-unlimited-permit-deny") else {
        eprintln!("P3 preset absent — skipping (gitignored, not on this clone)");
        return;
    };
    let _surface = adapters::load_and_install().expect("install local surface");
    // stETH unlimited permit to a non-allowlisted spender (typed-data sign).
    let message = permit_message(EVIL_SPENDER, EVIL_SPENDER, MAX_HEX);
    let env = harness::route::route_typed_data(
        1,
        STETH,
        "Permit",
        None,
        Some("Liquid staked Ether 2.0"),
        &message,
    );
    let verdict = verdict_over(&env, &policy, &manifest, STETH);
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("fail"),
        "an unlimited stETH permit to a non-allowlisted spender must DENY: {verdict}"
    );
    assert!(
        matched_has(&verdict, "steth-wsteth-unlimited-permit-deny"),
        "deny must be attributed to steth-wsteth-unlimited-permit-deny: {verdict}"
    );
}

#[test]
fn wsteth_bounded_permit_yields_warn_verdict() {
    let Some((policy, manifest)) = p3_set("steth-wsteth-permit-non-allowlisted-warn") else {
        eprintln!("P3 preset absent — skipping (gitignored, not on this clone)");
        return;
    };
    let _surface = adapters::load_and_install().expect("install local surface");
    // wstETH bounded permit to a non-allowlisted spender (typed-data sign).
    let message = permit_message(EVIL_SPENDER, EVIL_SPENDER, "1000000000000000000");
    let env = harness::route::route_typed_data(
        1,
        WSTETH,
        "Permit",
        None,
        Some("Wrapped liquid staked Ether 2.0"),
        &message,
    );
    let verdict = verdict_over(&env, &policy, &manifest, WSTETH);
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("warn"),
        "a bounded wstETH permit to a non-allowlisted spender must WARN: {verdict}"
    );
    assert!(
        matched_has(&verdict, "steth-wsteth-permit-non-allowlisted-warn"),
        "warn must be attributed to steth-wsteth-permit-non-allowlisted-warn: {verdict}"
    );
}

// ── The other three P3 SETs (already-deployed decode paths), verdict-locked ───
// approve×2 ride the generic `standard/erc20/approve` (tokens:erc20 enumerate,
// stETH/wstETH included); withdrawal-permit rides `requestWithdrawalsWithPermit`.
// These pin that the P3 Cedar fires over those decodes too.

/// Lido WithdrawalQueueERC721 (unstETH).
const WQ: &str = "0x889edc2edab5f40e902b864ad4d7ade8e412f9b1";

/// ABI-encode `approve(spender, amount)` (selector 0x095ea7b3).
fn approve_calldata(spender: &str, amount_hex64: &str) -> String {
    format!("0x095ea7b3{}{amount_hex64}", word_addr(spender))
}

/// ABI-encode `requestWithdrawalsWithPermit(uint256[] _amounts, address _owner,
/// (uint256 value,uint256 deadline,uint8 v,bytes32 r,bytes32 s) _permit)`
/// (selector 0xacf41e4d). Head is 7 words (offset 0xe0); tail is the amounts array.
fn req_withdrawals_with_permit_calldata(
    owner: &str,
    permit_value_hex64: &str,
    amount: u128,
) -> String {
    format!(
        "0xacf41e4d{off}{owner}{value}{deadline}{v}{r}{s}{len}{amt}",
        off = format!("{:064x}", 0xe0u64),
        owner = word_addr(owner),
        value = permit_value_hex64,
        deadline = format!("{:064x}", 9_999_999_999u64),
        v = format!("{:064x}", 27u64),
        r = "11".repeat(32),
        s = "22".repeat(32),
        len = format!("{:064x}", 1u64),
        amt = format!("{amount:064x}"),
    )
}

#[test]
fn steth_unlimited_approve_yields_deny_verdict() {
    let Some((policy, manifest)) = p3_set("steth-wsteth-unlimited-approve-deny") else {
        eprintln!("P3 preset absent — skipping (gitignored, not on this clone)");
        return;
    };
    let _surface = adapters::load_and_install().expect("install local surface");
    let calldata = approve_calldata(EVIL_SPENDER, &"f".repeat(64));
    let env = harness::route::route_calldata(1, STETH, "0x095ea7b3", &calldata, "0");
    let verdict = verdict_over(&env, &policy, &manifest, STETH);
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("fail"),
        "an unlimited stETH approve to a non-allowlisted spender must DENY: {verdict}"
    );
    assert!(
        matched_has(&verdict, "steth-wsteth-unlimited-approve-deny"),
        "deny must be attributed to steth-wsteth-unlimited-approve-deny: {verdict}"
    );
}

#[test]
fn steth_bounded_approve_yields_warn_verdict() {
    let Some((policy, manifest)) = p3_set("steth-wsteth-approve-non-allowlisted-warn") else {
        eprintln!("P3 preset absent — skipping (gitignored, not on this clone)");
        return;
    };
    let _surface = adapters::load_and_install().expect("install local surface");
    let calldata = approve_calldata(
        EVIL_SPENDER,
        &format!("{:064x}", 1_000_000_000_000_000_000u128),
    );
    let env = harness::route::route_calldata(1, STETH, "0x095ea7b3", &calldata, "0");
    let verdict = verdict_over(&env, &policy, &manifest, STETH);
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("warn"),
        "a bounded stETH approve to a non-allowlisted spender must WARN: {verdict}"
    );
    assert!(
        matched_has(&verdict, "steth-wsteth-approve-non-allowlisted-warn"),
        "warn must be attributed to steth-wsteth-approve-non-allowlisted-warn: {verdict}"
    );
}

#[test]
fn withdrawal_unlimited_embedded_permit_yields_warn_verdict() {
    let Some((policy, manifest)) = p3_set("withdrawal-permit-unlimited-warn") else {
        eprintln!("P3 preset absent — skipping (gitignored, not on this clone)");
        return;
    };
    let _surface = adapters::load_and_install().expect("install local surface");
    // requestWithdrawalsWithPermit whose embedded permit grants the queue an
    // UNLIMITED (U256::MAX) allowance → the warn SET's target.
    let calldata = req_withdrawals_with_permit_calldata(
        "0x000000000000000000000000000000000000aaaa",
        &"f".repeat(64),
        1_000_000_000_000_000_000,
    );
    let env = harness::route::route_calldata(1, WQ, "0xacf41e4d", &calldata, "0");
    let verdict = verdict_over(&env, &policy, &manifest, WQ);
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("warn"),
        "a withdrawal request with an unlimited embedded permit must WARN: {verdict}"
    );
    assert!(
        matched_has(&verdict, "withdrawal-permit-unlimited-warn"),
        "warn must be attributed to withdrawal-permit-unlimited-warn: {verdict}"
    );
}
