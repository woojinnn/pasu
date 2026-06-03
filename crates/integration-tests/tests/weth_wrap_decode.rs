//! Real-tx decode golden for the `standard/weth` wrap/unwrap adapters.
//!
//! Pins the dogfood repro: a plain ETH→WETH wrap on app.uniswap.org
//! (`deposit()`, selector `0xd0e30db0`, to canonical WETH) previously
//! route-missed (`bundle_not_installed`) → fail-closed warn, because the
//! registry had no WETH adapter and the `token` domain had no wrap action.
//!
//! After adding `Token::{WrapNative,UnwrapNative}` + the `standard/weth/{deposit,
//! withdraw}` manifests, the route must HIT and decode to the new actions with
//! `amount` bound to `msg.value` (deposit) / the `wad` arg (unwrap) — NOT miss,
//! NOT mislabel as an `erc20_transfer`/`unknown`. Mirrors
//! `v3_decode_harness::morpho_supply_market_id_is_keccak_marketparams`
//! (install + route on the same thread — WASM v3 install state is thread-local).

use policy_engine_integration_tests::harness::{self, adapters};
use serde_json::Value;

/// Canonical WETH (Ethereum mainnet).
const WETH_MAINNET: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

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
/// The test amounts fit in `u128`.
fn parse_u256_str(s: &str) -> u128 {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16).expect("hex amount parses")
    } else {
        t.parse::<u128>().expect("decimal amount parses")
    }
}

/// ETH→WETH wrap: `deposit()` takes no args; the wrapped amount is `msg.value`.
/// value = 0.0001 ETH = 100_000_000_000_000 wei — the exact dogfood-tx value.
#[test]
fn weth_deposit_decodes_wrap_native_with_msg_value() {
    let _surface = adapters::load_and_install().expect("install local surface");

    let value_wei: u128 = 100_000_000_000_000;
    let env = harness::route::route_calldata(
        1,
        WETH_MAINNET,
        "0xd0e30db0",
        "0xd0e30db0",
        "100000000000000",
    );

    assert_eq!(
        env.get("ok").and_then(Value::as_bool),
        Some(true),
        "wrap route did not succeed (regressed to a miss / bundle_not_installed?): {env}"
    );
    let action = find_string_field(&env, "action").expect("decoded body carries an action tag");
    assert_eq!(
        action, "wrap_native",
        "ETH→WETH deposit must decode to Token::WrapNative: {env}"
    );
    let amount = find_string_field(&env, "amount").expect("wrap body carries an amount");
    assert_eq!(
        parse_u256_str(&amount),
        value_wei,
        "wrap amount must equal msg.value ($tx.value): {env}"
    );
}

/// WETH→ETH unwrap: `withdraw(uint256 wad)`; the unwrapped amount is the `wad`
/// arg ($args.wad). wad = 1 WETH = 1e18.
#[test]
fn weth_withdraw_decodes_unwrap_native_with_wad() {
    let _surface = adapters::load_and_install().expect("install local surface");

    let wad: u128 = 1_000_000_000_000_000_000;
    let calldata = format!("0x2e1a7d4d{wad:064x}");
    let env = harness::route::route_calldata(1, WETH_MAINNET, "0x2e1a7d4d", &calldata, "0");

    assert_eq!(
        env.get("ok").and_then(Value::as_bool),
        Some(true),
        "unwrap route did not succeed: {env}"
    );
    let action = find_string_field(&env, "action").expect("decoded body carries an action tag");
    assert_eq!(
        action, "unwrap_native",
        "WETH withdraw must decode to Token::UnwrapNative: {env}"
    );
    let amount = find_string_field(&env, "amount").expect("unwrap body carries an amount");
    assert_eq!(
        parse_u256_str(&amount),
        wad,
        "unwrap amount must equal the wad argument ($args.wad): {env}"
    );
}
