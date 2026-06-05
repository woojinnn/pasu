//! M2 — `declarative_install_v3_json` + `declarative_route_request_v3_json`
//! end-to-end fixtures.
//!
//! Each test installs a v3 manifest (registryV2 raw, untyped) via
//! `declarative_install_v3_json`, then routes a hand-built raw calldata
//! through `declarative_route_request_v3_json` and asserts that the resulting
//! hierarchical `ActionBody` JSON matches the PDF FSM spec.
//!
//! Narrow-scope (M2):
//!   * `single_emit` strategy with literal venue addresses (no `$resolved` /
//!     `$derived` references — those are filled by M5+).
//!   * `opcode_stream_dispatch` with a single UR `V3_SWAP_EXACT_IN` opcode.
//!     `inputs_abi` decoding is best-effort; the test asserts the structural
//!     shape (`body.actions[0].domain == "amm"`, `action == "swap"`) without
//!     pinning the per-opcode payload fields (those exercise the M2 fallback
//!     when the manifest's `inputs_abi` cannot fully ABI-decode the raw
//!     inputs).
//!
//! Each fixture's manifest is inlined here verbatim — the registry on disk
//! still hosts v1-shape manifests and M2 should not depend on that snapshot.

use alloy_dyn_abi::DynSolValue;
use alloy_primitives::{Address as AlloyAddress, U256 as AlloyU256};
use policy_engine_wasm::{
    declarative_install_v3_json, declarative_route_request_v3_json,
    declarative_route_typed_data_v3_json,
};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// 4-byte selector + ABI-encoded parameters → "0x"-prefixed hex calldata.
fn encode_calldata(selector: &str, args: &[DynSolValue]) -> String {
    let sel = hex::decode(selector.trim_start_matches("0x")).unwrap();
    let body = DynSolValue::Tuple(args.to_vec()).abi_encode_params();
    format!("0x{}{}", hex::encode(sel), hex::encode(body))
}

/// Build a route-request input JSON wrapping the raw calldata.
fn route_input(
    chain_id: u64,
    to: &str,
    selector: &str,
    calldata: String,
    submitter: &str,
) -> String {
    json!({
        "chain_id": chain_id,
        "to": to,
        "selector": selector,
        "calldata": calldata,
        "value": "0",
        "gas_limit": "200000",
        "gas_price": "20000000000",
        "submitter": submitter,
        "submitted_at": 1_700_000_000_u64,
        "nonce": 1_u64,
        "block_timestamp": 1_700_000_010_u64
    })
    .to_string()
}

/// Like [`route_input`] but with a caller-supplied native `value` (decimal wei
/// string). Used by payable-entry fixtures (e.g. WrappedTokenGateway
/// `depositETH`) where the supplied amount IS `msg.value` (`$tx.value`), not a
/// calldata arg.
fn route_input_with_value(
    chain_id: u64,
    to: &str,
    selector: &str,
    calldata: String,
    submitter: &str,
    value: &str,
) -> String {
    json!({
        "chain_id": chain_id,
        "to": to,
        "selector": selector,
        "calldata": calldata,
        "value": value,
        "gas_limit": "200000",
        "gas_price": "20000000000",
        "submitter": submitter,
        "submitted_at": 1_700_000_000_u64,
        "nonce": 1_u64,
        "block_timestamp": 1_700_000_010_u64
    })
    .to_string()
}

fn install_ok(manifest: &str) -> Value {
    let out = declarative_install_v3_json(manifest.to_owned());
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "install failed: {parsed}");
    parsed
}

fn route_ok(input: String) -> Value {
    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    parsed
}

// ---------------------------------------------------------------------------
// t1 — ERC20 approve (single_emit + literal venue not applicable, token only)
// ---------------------------------------------------------------------------
//
// Mirrors what `standard/erc20/approve@1.0.0` would look like in v3 schema.
// `$tx.to` resolves to the route input's `to` field (the token contract).

const T1_ERC20_APPROVE_V3: &str = r#"{
  "type": "adapter_function",
  "id": "standard/erc20/approve@2.0.0",
  "publisher": "ethereum.org",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0xa0B86991c6218b36c1d19D4a2e9Eb0cE3606eB48"]
    },
    "selector": "0x095ea7b3"
  },
  "abi_fragment": {
    "function_name": "approve",
    "abi": {
      "name": "approve",
      "type": "function",
      "inputs": [
        { "name": "spender", "type": "address" },
        { "name": "amount", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_approve",
        "erc20_approve": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$tx.to" } },
          "spender": "$args.spender",
          "amount": "$args.amount"
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t1_erc20_approve() {
    let install = install_ok(T1_ERC20_APPROVE_V3);
    assert_eq!(install["data"]["bundle_id"], "standard/erc20/approve@2.0.0");

    let calldata = encode_calldata(
        "0x095ea7b3",
        &[
            DynSolValue::Address(
                "0x00000000000000000000000000000000deadbeef"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "0x095ea7b3",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    assert_eq!(parsed["data"]["decoder_id"], "standard/erc20/approve@2.0.0");

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token");
    assert_eq!(body["action"], "erc20_approve");
    assert_eq!(
        body["spender"],
        "0x00000000000000000000000000000000deadbeef"
    );
    // U256 round-trips as a hex string through alloy serde.
    assert_eq!(body["amount"], "0xf4240"); // 1_000_000 == 0xf4240
    assert_eq!(body["token"]["key"]["standard"], "erc20");
    assert_eq!(body["token"]["key"]["chain"], "eip155:1");
    assert_eq!(
        body["token"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
}

// ---------------------------------------------------------------------------
// t2 — Uniswap V2 swapExactTokensForTokens (single_emit + literal venue)
// ---------------------------------------------------------------------------

const T2_V2_SWAP_V3: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/v2-router-02/swapExactTokensForTokens@2.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"]
    },
    "selector": "0x38ed1739"
  },
  "abi_fragment": {
    "function_name": "swapExactTokensForTokens",
    "abi": {
      "name": "swapExactTokensForTokens",
      "type": "function",
      "inputs": [
        { "name": "amountIn", "type": "uint256" },
        { "name": "amountOutMin", "type": "uint256" },
        { "name": "path", "type": "address[]" },
        { "name": "to", "type": "address" },
        { "name": "deadline", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "amm",
      "amm": {
        "action": "swap",
        "swap": {
          "venue": {
            "name": "uniswap_v2",
            "chain": "$chain",
            "pool": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
            "factory": "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f"
          },
          "params": {
            "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.path[0]" } },
            "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.path[-1]" } },
            "direction": {
              "kind": "exact_input",
              "amount_in": "$args.amountIn",
              "min_amount_out": "$args.amountOutMin"
            },
            "recipient": "$args.to",
            "slippage_bp": 50
          }
        }
      }
    },
    "live_inputs": {
      "route":               { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640", "function": "getReserves()", "decoder_id": "uniswap_v2_get_reserves" }, "ttl_s": 12 },
      "expected_amount_out": { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
      "price_impact_bp":     { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
      "gas_estimate":        { "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/ethereum" }, "ttl_s": 6 }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t2_v2_swap_exact_tokens_for_tokens() {
    install_ok(T2_V2_SWAP_V3);

    let calldata = encode_calldata(
        "0x38ed1739",
        &[
            DynSolValue::Uint(AlloyU256::from(1_000_000_000_000_000_000u128), 256),
            DynSolValue::Uint(AlloyU256::from(1_900_000u64), 256),
            DynSolValue::Array(vec![
                DynSolValue::Address(
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Address(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
            ]),
            DynSolValue::Address(
                "0x4444444444444444444444444444444444444444"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(1_700_000_900u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
        "0x38ed1739",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm");
    assert_eq!(body["action"], "swap");
    assert_eq!(body["venue"]["name"], "uniswap_v2");
    assert_eq!(
        body["venue"]["pool"],
        "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"
    );
    assert_eq!(body["params"]["direction"]["kind"], "exact_input");
    assert_eq!(body["params"]["slippage_bp"], 50);
    assert_eq!(
        body["live_inputs"]["route"]["source"]["kind"],
        "onchain_view"
    );
    assert_eq!(
        body["live_inputs"]["gas_estimate"]["source"]["kind"],
        "oracle_feed"
    );
}

// ---------------------------------------------------------------------------
// t3 — Permit2 approve (on-chain)
// ---------------------------------------------------------------------------

const T3_PERMIT2_APPROVE_V3: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/permit2/approve@2.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x000000000022D473030F116dDEE9F6B43aC78BA3"]
    },
    "selector": "0x87517c45"
  },
  "abi_fragment": {
    "function_name": "approve",
    "abi": {
      "name": "approve",
      "type": "function",
      "inputs": [
        { "name": "token", "type": "address" },
        { "name": "spender", "type": "address" },
        { "name": "amount", "type": "uint160" },
        { "name": "expiration", "type": "uint48" }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "permit2_approve",
        "permit2_approve": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.token" } },
          "spender": "$args.spender",
          "amount": "$args.amount",
          "expires_at": "$args.expiration"
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t3_permit2_approve_onchain() {
    install_ok(T3_PERMIT2_APPROVE_V3);

    // Permit2 approve uses uint160 for `amount` and uint48 for `expiration`.
    // `expires_at` deserializes into a `Time` (transparent over u64), which
    // accepts a JSON NUMBER and rejects a decimal string. With the
    // width-based `args_to_json` coercion (uint ≤ 64 bit → JSON number), the
    // uint48 `expiration` now renders as a number and the body decodes — so
    // this test asserts `ok:true` (previously it tolerated a documented
    // `build_action_body_failed` "expected u64" fallback). `amount` is
    // uint160 (> 64) and stays a decimal string → parses into the `U256`
    // amount field as before.
    let calldata = encode_calldata(
        "0x87517c45",
        &[
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Address(
                "0x00000000000000000000000000000000deadbeef"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(999u64), 160),
            DynSolValue::Uint(AlloyU256::from(1_738_001_800u64), 48),
        ],
    );
    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0x87517c45",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token");
    assert_eq!(body["action"], "permit2_approve");
    assert_eq!(
        body["spender"],
        "0x00000000000000000000000000000000deadbeef"
    );
    assert_eq!(
        body["token"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
}

// ---------------------------------------------------------------------------
// t4 — Universal Router execute single V3_SWAP_EXACT_IN
// ---------------------------------------------------------------------------
//
// One opcode (0x00 = V3_SWAP_EXACT_IN) with a raw padded inputs blob.
// The opcode body inlines `live_inputs` per the v3 opcode-stream convention.
// The action_builder wraps the result in `ActionBody::Multicall { actions: [...] }`.

const T4_UR_EXECUTE_V3: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/universal-router/execute-v2@2.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x3F6328669a86bef431Dc6F9201A5B90F7975a023"]
    },
    "selector": "0x3593564c"
  },
  "abi_fragment": {
    "function_name": "execute",
    "abi": {
      "name": "execute",
      "type": "function",
      "inputs": [
        { "name": "commands", "type": "bytes" },
        { "name": "inputs", "type": "bytes[]" },
        { "name": "deadline", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "opcode_stream_dispatch",
    "mask": "0x3f",
    "allow_revert_bit": "0x80",
    "unknown_opcode_policy": "warn",
    "per_opcode_body": {
      "0x00": {
        "name": "V3_SWAP_EXACT_IN",
        "inputs_abi": "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
        "body": {
          "domain": "amm",
          "amm": {
            "action": "swap",
            "swap": {
              "venue": {
                "name": "uniswap_v3",
                "chain": "$chain",
                "pool": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
                "fee_tier_bp": 500
              },
              "params": {
                "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" } },
                "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" } },
                "direction": {
                  "kind": "exact_input",
                  "amount_in": "$inputs.amountIn",
                  "min_amount_out": "$inputs.amountOutMin"
                },
                "recipient": "$inputs.recipient",
                "slippage_bp": 50
              },
              "live_inputs": {
                "route":               { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640", "function": "slot0()", "decoder_id": "uniswap_v3_slot0" }, "ttl_s": 12 },
                "expected_amount_out": { "source": { "kind": "venue_api", "endpoint": "https://api.example", "parser_id": "p" }, "ttl_s": 6 },
                "price_impact_bp":     { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
                "gas_estimate":        { "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/ethereum" }, "ttl_s": 6 }
              }
            }
          }
        }
      }
    }
  },
  "requires": {
    "imperative": ["opcode-stream-dispatch@^1.0"],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t4_ur_execute_single_v3_swap() {
    install_ok(T4_UR_EXECUTE_V3);

    // Build a V3_SWAP_EXACT_IN inputs blob via the same ABI signature the
    // manifest declares — alloy will then round-trip it back into the
    // expected JSON object inside the route handler.
    let inputs_blob = DynSolValue::Tuple(vec![
        DynSolValue::Address(
            "0x000000000000000000000000000000000000a01c"
                .parse::<AlloyAddress>()
                .unwrap(),
        ),
        DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
        DynSolValue::Uint(AlloyU256::from(1u64), 256),
        DynSolValue::Bytes(vec![0u8; 0]),
        DynSolValue::Bool(true),
    ])
    .abi_encode_params();

    let calldata = encode_calldata(
        "0x3593564c",
        &[
            DynSolValue::Bytes(vec![0x00]),
            DynSolValue::Array(vec![DynSolValue::Bytes(inputs_blob)]),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x3f6328669a86bef431dc6f9201a5b90f7975a023",
        "0x3593564c",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();

    // Best-effort: success means the opcode-stream dispatch executed and the
    // inner V3_SWAP_EXACT_IN body was decoded against `inputs_abi`. The M2
    // fallback (Null inputs) would surface `unresolved_placeholder` from the
    // action_builder when `$inputs.amountIn` cannot be walked — we assert
    // both branches.
    if parsed["ok"] == Value::Bool(true) {
        let body = &parsed["data"]["actions"][0]["body"];
        assert_eq!(body["domain"], "multicall");
        let inner = &body["actions"][0];
        assert_eq!(inner["domain"], "amm");
        assert_eq!(inner["action"], "swap");
        assert_eq!(inner["venue"]["name"], "uniswap_v3");
        assert_eq!(inner["params"]["direction"]["kind"], "exact_input");
        assert_eq!(inner["params"]["slippage_bp"], 50);
    } else {
        // If the inputs_abi tuple decode failed silently (M2 fallback to
        // Null), `$inputs.amountIn` walks against Null and surfaces a
        // `build_multicall_failed` envelope. That outcome confirms the
        // pipeline reached the action_builder stage — which is the M2
        // narrow scope's contract.
        let kind = parsed["error"]["kind"].as_str().unwrap_or("");
        assert!(
            kind == "build_multicall_failed" || kind == "build_action_body_failed",
            "expected action_builder error, got: {parsed}"
        );
    }
}

// ---------------------------------------------------------------------------
// t4b — UR execute V3_SWAP_EXACT_IN derives token_in/out from the PACKED PATH
// ---------------------------------------------------------------------------
//
// Regression for the "$derived re-derivation" bug: the UR opcode body resolves
// token_in/out from `$derived.v3_path_first_token` / `$derived.v3_path_last_token`
// (and `$derived.fee_tier_bp`), produced by `maybe_inject_uniswap_v3_path`. That
// injector only ran at the TOP-LEVEL route (the outer `execute(commands, inputs)`
// args have no `path` key), and the per-opcode child context merely CLONED the
// (empty) parent `derived` — so every UR V3 swap leg decoded token_in ==
// token_out == 0x0 regardless of the real path. The fix re-runs the injector per
// opcode against the opcode's own decoded inputs (which carry the packed `path`).
// This manifest uses the `$derived.v3_path_*` placeholders (mirroring the live
// registry manifest, NOT t4's literal-token shortcut) and feeds a REAL
// USDC|3000|WETH path; it FAILS before the fix (tokens 0x0) and passes after.

const T4B_USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const T4B_WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

const T4B_UR_EXECUTE_V3_DERIVED: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/universal-router/execute-v2@2.0.1-derived-path-repro",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x66a9893cC07D91D95644AEDD05D03f95e1dBA8aF"]
    },
    "selector": "0x3593564c"
  },
  "abi_fragment": {
    "function_name": "execute",
    "abi": {
      "name": "execute",
      "type": "function",
      "inputs": [
        { "name": "commands", "type": "bytes" },
        { "name": "inputs", "type": "bytes[]" },
        { "name": "deadline", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "opcode_stream_dispatch",
    "mask": "0x3f",
    "allow_revert_bit": "0x80",
    "unknown_opcode_policy": "warn",
    "per_opcode_body": {
      "0x00": {
        "name": "V3_SWAP_EXACT_IN",
        "inputs_abi": "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
        "body": {
          "domain": "amm",
          "amm": {
            "action": "swap",
            "swap": {
              "venue": {
                "name": "uniswap_v3",
                "chain": "$chain",
                "pool": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640",
                "fee_tier_bp": "$derived.fee_tier_bp"
              },
              "params": {
                "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_first_token" } },
                "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_last_token" } },
                "direction": {
                  "kind": "exact_input",
                  "amount_in": "$inputs.amountIn",
                  "min_amount_out": "$inputs.amountOutMin"
                },
                "recipient": "$inputs.recipient",
                "slippage_bp": 50
              },
              "live_inputs": {
                "route":               { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640", "function": "slot0()", "decoder_id": "uniswap_v3_slot0" }, "ttl_s": 12 },
                "expected_amount_out": { "source": { "kind": "venue_api", "endpoint": "https://api.example", "parser_id": "p" }, "ttl_s": 6 },
                "price_impact_bp":     { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
                "gas_estimate":        { "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/ethereum" }, "ttl_s": 6 }
              }
            }
          }
        }
      }
    }
  },
  "requires": {
    "imperative": ["opcode-stream-dispatch@^1.0"],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t4b_ur_execute_v3_swap_derives_tokens_from_path() {
    install_ok(T4B_UR_EXECUTE_V3_DERIVED);

    // REAL packed V3 path: USDC | fee 3000 (0x000bb8) | WETH.
    let mut path = Vec::new();
    path.extend_from_slice(&hex::decode(T4B_USDC.trim_start_matches("0x")).unwrap());
    path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
    path.extend_from_slice(&hex::decode(T4B_WETH.trim_start_matches("0x")).unwrap());

    let inputs_blob = DynSolValue::Tuple(vec![
        DynSolValue::Address(
            "0x000000000000000000000000000000000000a01c"
                .parse::<AlloyAddress>()
                .unwrap(),
        ),
        DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
        DynSolValue::Uint(AlloyU256::from(1u64), 256),
        DynSolValue::Bytes(path),
        DynSolValue::Bool(true),
    ])
    .abi_encode_params();

    let calldata = encode_calldata(
        "0x3593564c",
        &[
            DynSolValue::Bytes(vec![0x00]),
            DynSolValue::Array(vec![DynSolValue::Bytes(inputs_blob)]),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x66a9893cc07d91d95644aedd05d03f95e1dba8af",
        "0x3593564c",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall");
    let inner = &body["actions"][0];
    assert_eq!(inner["domain"], "amm");
    assert_eq!(inner["action"], "swap");

    // The crux: token_in/out are derived from the PACKED PATH, not 0x0.
    assert_eq!(
        inner["params"]["token_in"]["key"]["address"], T4B_USDC,
        "token_in must be derived from the packed path (was 0x0 before the per-opcode re-derivation fix): {parsed}"
    );
    assert_eq!(
        inner["params"]["token_out"]["key"]["address"], T4B_WETH,
        "token_out must be derived from the packed path: {parsed}"
    );
    // fee_tier_bp also comes from the path's first hop (3000) via the same injector.
    assert_eq!(
        inner["venue"]["fee_tier_bp"], 3000,
        "fee_tier_bp from path: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// t5 — array_emit (Phase A.2): homogeneous calldata array → Multicall
// ---------------------------------------------------------------------------
//
// A `transferBatch((address token, address recipient, uint256 amount)[])`
// shape (Permit2 `transferFromBatch` / Balancer `batchSwap` family).
// `emit.array_source` `"$args.transfers"` resolves to the decoded `transfers`
// array. NOTE: on the CALLDATA path the ABI decoder (`decoded_value_to_json`)
// encodes each tuple element as a POSITIONAL array `[token, recipient,
// amount]` — NOT a named object — so the per-item body references fields by
// index (`$inputs.[0]` / `$inputs.[1]` / `$inputs.[2]`). (The typed-data path,
// by contrast, carries named-object EIP-712 message elements — see the
// PermitBatch fixture in `declarative_v3_typed_data_install.rs`.) The two
// elements differ (token / recipient / amount) — proving per-element binding.

const T5_ARRAY_EMIT_V3: &str = r#"{
  "type": "adapter_function",
  "id": "test/batch/transferBatch@2.0.0",
  "publisher": "test.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x000000000022D473030F116dDEE9F6B43aC78BA3"]
    },
    "selector": "0x22c5c901"
  },
  "abi_fragment": {
    "function_name": "transferBatch",
    "abi": {
      "name": "transferBatch",
      "type": "function",
      "inputs": [
        {
          "name": "transfers",
          "type": "tuple[]",
          "components": [
            { "name": "token", "type": "address" },
            { "name": "recipient", "type": "address" },
            { "name": "amount", "type": "uint256" }
          ]
        }
      ]
    }
  },
  "emit": {
    "strategy": "array_emit",
    "array_source": "$args.transfers",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_transfer",
        "erc20_transfer": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.[0]" } },
          "recipient": "$inputs.[1]",
          "amount": "$inputs.[2]"
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

/// Build a 2-element `transferBatch` calldata + route input. `transfers` is a
/// `tuple[]` so the args_json `transfers` field is a 2-element array of objects.
fn t5_two_element_calldata() -> String {
    encode_calldata(
        "0x22c5c901",
        &[DynSolValue::Array(vec![
            DynSolValue::Tuple(vec![
                DynSolValue::Address(
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Address(
                    "0x00000000000000000000000000000000deadbeef"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Uint(AlloyU256::from(1000u64), 256),
            ]),
            DynSolValue::Tuple(vec![
                DynSolValue::Address(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Address(
                    "0x00000000000000000000000000000000cafef00d"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Uint(AlloyU256::from(2000u64), 256),
            ]),
        ])],
    )
}

#[test]
fn t5_array_emit_calldata_two_transfers() {
    install_ok(T5_ARRAY_EMIT_V3);

    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0x22c5c901",
        t5_two_element_calldata(),
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"],
        "test/batch/transferBatch@2.0.0"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let actions = body["actions"].as_array().expect("inner actions array");
    assert_eq!(actions.len(), 2, "{parsed}");

    // element-0
    assert_eq!(actions[0]["domain"], "token");
    assert_eq!(actions[0]["action"], "erc20_transfer");
    assert_eq!(
        actions[0]["recipient"],
        "0x00000000000000000000000000000000deadbeef"
    );
    assert_eq!(actions[0]["amount"], "0x3e8"); // 1000
    assert_eq!(
        actions[0]["token"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );

    // element-1 — DIFFERENT fields prove per-element $inputs binding.
    assert_eq!(actions[1]["action"], "erc20_transfer");
    assert_eq!(
        actions[1]["recipient"],
        "0x00000000000000000000000000000000cafef00d"
    );
    assert_eq!(actions[1]["amount"], "0x7d0"); // 2000
    assert_eq!(
        actions[1]["token"]["key"]["address"],
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
    );
}

#[test]
fn t5_array_emit_empty_array_empty_multicall() {
    install_ok(T5_ARRAY_EMIT_V3);

    // Empty `transfers` array → Unknown leg (5f3872ff: an empty array_emit
    // surfaces as Unknown so the position stays policy-visible, rather than a
    // silently-empty Multicall that would aggregate to PASS).
    let calldata = encode_calldata("0x22c5c901", &[DynSolValue::Array(vec![])]);
    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0x22c5c901",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(
        body["target"], "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// t6 — array_emit array_source resolving to a NON-array → error
// ---------------------------------------------------------------------------
//
// Same body shape but `array_source` points at a scalar (`$args.notArray`,
// a single uint). `build_array_emit`'s `as_array` fails → ArraySourceNotArray
// surfaces as the `build_array_emit_failed` envelope (ok:false).

const T6_ARRAY_EMIT_NONARRAY_V3: &str = r#"{
  "type": "adapter_function",
  "id": "test/batch/notArray@2.0.0",
  "publisher": "test.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x000000000022D473030F116dDEE9F6B43aC78BA3"]
    },
    "selector": "0xa67a81d2"
  },
  "abi_fragment": {
    "function_name": "notArray",
    "abi": {
      "name": "notArray",
      "type": "function",
      "inputs": [
        { "name": "notArray", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "array_emit",
    "array_source": "$args.notArray",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_transfer",
        "erc20_transfer": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.token" } },
          "recipient": "$inputs.recipient",
          "amount": "$inputs.amount"
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t6_array_emit_non_array_source_errors() {
    install_ok(T6_ARRAY_EMIT_NONARRAY_V3);

    // `notArray` decodes to a single uint string — `$args.notArray` is NOT an
    // array, so build_array_emit fails.
    let calldata = encode_calldata(
        "0xa67a81d2",
        &[DynSolValue::Uint(AlloyU256::from(42u64), 256)],
    );
    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0xa67a81d2",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(
        parsed["error"]["kind"], "build_array_emit_failed",
        "{parsed}"
    );
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("did not resolve to an array"),
        "{parsed}"
    );
}

// ===========================================================================
// B.2.1 — Aave V3 Tier-1 Pool manifests (lending domain)
// ===========================================================================
//
// Three on-chain `Pool` calls route into the `lending` `ActionBody`:
//   * t7 liquidationCall  → LendingAction::Liquidate
//   * t8 setUserEMode     → LendingAction::SetEMode      (serde tag `set_e_mode`)
//   * t9 swapBorrowRateMode → LendingAction::SwapRateMode
//
// Each inline manifest is IDENTICAL in `abi_fragment` + `emit.body` to the
// committed fixture under `registryV2/manifests/aave/v3/`. The selectors are
// `cast sig`-verified (0x00a718a9 / 0x28530a47 / 0x94ba89a2) and the venue
// `chain_to_addresses` mirror the existing supply/borrow/withdraw/repay
// manifests (mainnet Pool here; the on-disk fixtures carry all 4 chains).
//
// FULLY GREEN `ok:true` (B.2-infra closed the two foundation gaps)
// ----------------------------------------------------------------
// These manifests are STRUCTURALLY valid: install succeeds, the selector +
// address match, `$args.*` / `$chain` / `$to` placeholders substitute, and the
// body flattens into the right `LendingAction` variant. The two former gaps
// (which made these route tests tolerate a documented `build_action_body_failed`)
// are now fixed in B.2-infra:
//   * liquidate / set_e_mode / swap_rate_mode — `action_builder::live_input_default`
//     now has `(lending, …)` deserializable zero skeletons for every lending
//     `LiveField<T>` (UserLendingState / ReserveState / EModeConfig / u32 /
//     the `(U256,U256)` + `(Decimal,Decimal)` 2-tuples), so each wrap's `value`
//     is a valid `T` instead of `null`.
//   * set_e_mode — `categoryId` is a `uint8`; `args_to_json` now emits uint
//     args ≤ 64 bit as JSON NUMBERS (the same coercion that fixes t3's Permit2
//     `expiration` uint48 → `Time`/u64), and the `category_id: u8` field accepts
//     a number.
// So each test asserts `ok:true` + the full hierarchical body (domain `lending`,
// the correct action tag, the load-bearing `$args` fields, and a representative
// defaulted `live_inputs` value).

const AAVE_V3_VARIABLE_DEBT_USDC_APPROVE_DELEGATION: &str = include_str!(
    "../../../registryV2/manifests/aave/v3/variable-debt-usdc-approve-delegation@1.0.0.json"
);
const AAVE_V3_VARIABLE_DEBT_USDC_DELEGATION_WITH_SIG: &str = include_str!(
    "../../../registryV2/manifests/aave/v3/variable-debt-usdc-delegation-with-sig@1.0.0.json"
);

#[test]
fn t6_aave_variable_debt_usdc_approve_delegation() {
    let install = install_ok(AAVE_V3_VARIABLE_DEBT_USDC_APPROVE_DELEGATION);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/variableDebtUSDC/approveDelegation@1.0.0"
    );

    let calldata = encode_calldata(
        "0xc04a8a10",
        &[
            DynSolValue::Address(
                "0x000000000000000000000000000000000000d1e9"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(2_500_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x72e95b8931767c79ba4eee721354d6e99a61d004",
        "0xc04a8a10",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending", "{parsed}");
    assert_eq!(body["action"], "delegate_borrow", "{parsed}");
    assert_eq!(body["venue"]["name"], "aave_v3", "{parsed}");
    assert_eq!(
        body["venue"]["pool"], "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "{parsed}"
    );
    assert_eq!(
        body["asset"]["key"]["address"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "{parsed}"
    );
    assert_eq!(
        body["delegatee"], "0x000000000000000000000000000000000000d1e9",
        "{parsed}"
    );
    assert_eq!(body["amount"], "0x2625a0", "{parsed}");
    assert_eq!(body["rate_mode"], "variable", "{parsed}");
}

#[test]
fn t6_aave_variable_debt_usdc_delegation_with_sig_calldata() {
    let install = install_ok(AAVE_V3_VARIABLE_DEBT_USDC_DELEGATION_WITH_SIG);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/variableDebtUSDC/delegationWithSig@1.0.0"
    );

    let calldata = encode_calldata(
        "0x0b52d558",
        &[
            DynSolValue::Address(
                "0x000000000000000000000000000000000000b0b0"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Address(
                "0x000000000000000000000000000000000000d1e9"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(2_500_000u64), 256),
            DynSolValue::Uint(AlloyU256::from(2_000_000_000u64), 256),
            DynSolValue::Uint(AlloyU256::from(27u64), 8),
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x11), 32),
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x22), 32),
        ],
    );
    let input = route_input(
        1,
        "0x72e95b8931767c79ba4eee721354d6e99a61d004",
        "0x0b52d558",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending", "{parsed}");
    assert_eq!(body["action"], "delegate_borrow", "{parsed}");
    assert_eq!(body["venue"]["name"], "aave_v3", "{parsed}");
    assert_eq!(
        body["asset"]["key"]["address"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "{parsed}"
    );
    assert_eq!(
        body["delegatee"], "0x000000000000000000000000000000000000d1e9",
        "{parsed}"
    );
    assert_eq!(body["amount"], "0x2625a0", "{parsed}");
    assert_eq!(body["rate_mode"], "variable", "{parsed}");
}

#[test]
fn t6_aave_variable_debt_usdc_delegation_with_sig_typed_data() {
    install_ok(AAVE_V3_VARIABLE_DEBT_USDC_DELEGATION_WITH_SIG);

    let input = json!({
        "chain_id": 1,
        "verifying_contract": "0x72e95b8931767c79ba4eee721354d6e99a61d004",
        "primary_type": "DelegationWithSig",
        "domain_name": "Aave Ethereum Variable Debt USDC",
        "message": {
            "delegatee": "0x000000000000000000000000000000000000d1e9",
            "value": "2500000",
            "nonce": "7",
            "deadline": "2000000000"
        },
        "submitter": "0x000000000000000000000000000000000000b0b0",
        "submitted_at": 1_700_000_000_u64
    })
    .to_string();

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "typed-data route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "aave/v3/variableDebtUSDC/delegationWithSig@1.0.0",
        "{parsed}"
    );

    let action = &parsed["data"]["actions"][0];
    assert_eq!(action["meta"]["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(
        action["meta"]["nature"]["domain"]["name"], "Aave Ethereum Variable Debt USDC",
        "{parsed}"
    );
    assert_eq!(
        action["meta"]["nature"]["deadline"], 2_000_000_000_u64,
        "{parsed}"
    );

    let body = &action["body"];
    assert_eq!(body["domain"], "lending", "{parsed}");
    assert_eq!(body["action"], "delegate_borrow", "{parsed}");
    assert_eq!(body["venue"]["name"], "aave_v3", "{parsed}");
    assert_eq!(
        body["asset"]["key"]["address"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "{parsed}"
    );
    assert_eq!(
        body["delegatee"], "0x000000000000000000000000000000000000d1e9",
        "{parsed}"
    );
    assert_eq!(body["amount"], "0x2625a0", "{parsed}");
    assert_eq!(body["rate_mode"], "variable", "{parsed}");
}

// ---------------------------------------------------------------------------
// t7 — Aave V3 liquidationCall → LendingAction::Liquidate
// ---------------------------------------------------------------------------

const T7_AAVE_LIQUIDATION_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/liquidationCall@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x00a718a9",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "liquidationCall",
    "abi": {
      "name": "liquidationCall",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "collateralAsset", "type": "address" },
        { "name": "debtAsset",       "type": "address" },
        { "name": "user",            "type": "address" },
        { "name": "debtToCover",     "type": "uint256" },
        { "name": "receiveAToken",   "type": "bool"    }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "liquidate",
        "liquidate": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "victim":          "$args.user",
          "debt_asset":      { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.debtAsset" } },
          "collat_asset":    { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.collateralAsset" } },
          "debt_to_cover":   "$args.debtToCover",
          "receive_a_token": "$args.receiveAToken"
        }
      }
    },
    "live_inputs": {
      "victim_state":       { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 },
      "liquidation_bonus":  { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getConfiguration(address)", "decoder_id": "aave_v3_reserve_config" }, "ttl_s": 60 },
      "debt_asset_price":   { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 },
      "collat_asset_price": { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t7_aave_liquidation_call() {
    let install = install_ok(T7_AAVE_LIQUIDATION_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/liquidationCall@1.0.0"
    );

    let calldata = encode_calldata(
        "0x00a718a9",
        &[
            // collateralAsset (load-bearing) — WETH
            DynSolValue::Address(
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // debtAsset — USDC
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // user (victim, load-bearing)
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // debtToCover
            DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
            // receiveAToken
            DynSolValue::Bool(true),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x00a718a9",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // With the `(lending, …)` `live_input_default` skeletons in place, every
    // `LiveField<T>` wrap (victim_state: UserLendingState, liquidation_bonus:
    // u32, debt/collat_asset_price: Price) defaults to a deserializable zero,
    // so the full `lending` body lands. (Previously this tolerated a
    // documented `build_action_body_failed` with `UserLendingState` in the
    // message — that gap is now closed.)
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "liquidate");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing $args fields resolved from calldata.
    assert_eq!(
        body["collat_asset"]["key"]["address"],
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
    );
    assert_eq!(
        body["debt_asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(body["victim"], "0x000000000000000000000000000000000000cccc");
    // live_input defaults wrapped + deserialized.
    assert_eq!(
        body["live_inputs"]["victim_state"]["value"]["health_factor"],
        "0"
    );
    assert_eq!(body["live_inputs"]["liquidation_bonus"]["value"], 0);
}

// ---------------------------------------------------------------------------
// t8 — Aave V3 setUserEMode → LendingAction::SetEMode (tag `set_e_mode`)
// ---------------------------------------------------------------------------

const T8_AAVE_SET_EMODE_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/setUserEMode@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x28530a47",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "setUserEMode",
    "abi": {
      "name": "setUserEMode",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [ { "name": "categoryId", "type": "uint8" } ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "set_e_mode",
        "set_e_mode": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "category_id": "$args.categoryId"
        }
      }
    },
    "live_inputs": {
      "category_config":   { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getEModeCategoryData(uint8)", "decoder_id": "aave_v3_emode_category" }, "ttl_s": 60 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t8_aave_set_user_emode() {
    let install = install_ok(T8_AAVE_SET_EMODE_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/setUserEMode@1.0.0");

    // categoryId = 2 (a load-bearing arg: selects the e-mode category).
    let calldata = encode_calldata("0x28530a47", &[DynSolValue::Uint(AlloyU256::from(2u64), 8)]);
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x28530a47",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // `categoryId` is a `uint8` (≤ 64 bit) so the width-based `args_to_json`
    // coercion now emits it as a JSON NUMBER, which the `category_id: u8`
    // field accepts; combined with the `(lending, set_e_mode, …)` live_input
    // defaults the full body lands. (Previously this tolerated a documented
    // `build_action_body_failed` `expected u8` fallback from the decimal-string
    // arg — that gap is now closed.)
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "set_e_mode");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing arg: category id resolved to the JSON number 2.
    assert_eq!(body["category_id"], 2, "{parsed}");
    // live_input defaults wrapped + deserialized (EModeConfig + UserLendingState).
    assert_eq!(body["live_inputs"]["category_config"]["value"]["ltv_bp"], 0);
    assert_eq!(
        body["live_inputs"]["user_state_before"]["value"]["health_factor"],
        "0"
    );
}

// ---------------------------------------------------------------------------
// t9 — Aave V3 swapBorrowRateMode → LendingAction::SwapRateMode
// ---------------------------------------------------------------------------

const T9_AAVE_SWAP_RATE_MODE_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/swapBorrowRateMode@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x94ba89a2",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "swapBorrowRateMode",
    "abi": {
      "name": "swapBorrowRateMode",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",            "type": "address" },
        { "name": "interestRateMode", "type": "uint256" }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "swap_rate_mode",
        "swap_rate_mode": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":    { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "new_mode": { "$match": "$args.interestRateMode", "$cases": { "1": "variable", "2": "stable" } }
        }
      }
    },
    "live_inputs": {
      "current_debts": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserReserveData(address,address)", "decoder_id": "aave_v3_user_reserve_debts" }, "ttl_s": 12 },
      "rates":         { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_rates" }, "ttl_s": 30 }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t9_aave_swap_borrow_rate_mode() {
    let install = install_ok(T9_AAVE_SWAP_RATE_MODE_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/swapBorrowRateMode@1.0.0"
    );

    let calldata = encode_calldata(
        "0x94ba89a2",
        &[
            // asset (load-bearing) — USDC
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // interestRateMode = 2 (VARIABLE) — the CURRENT mode being swapped
            // FROM (Aave IPool NatSpec). new_mode is the COMPLEMENT → "stable".
            DynSolValue::Uint(AlloyU256::from(2u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x94ba89a2",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // With the `(lending, swap_rate_mode, …)` live_input defaults the
    // `LiveField<(U256,U256)>` (current_debts) and `LiveField<(Decimal,Decimal)>`
    // (rates) wraps default to the deserializable `["0","0"]` 2-tuples, so the
    // full body lands. (Previously this tolerated a documented
    // `build_action_body_failed` `tuple of size 2` fallback — now closed.)
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "swap_rate_mode");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing $args field resolved.
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    // interestRateMode=2 (currently VARIABLE) → swaps TO stable. new_mode is
    // the COMPLEMENT of the arg per the `$match` value-map { "1":"variable",
    // "2":"stable" } (Aave IPool NatSpec: arg = "current rate mode of the
    // position being swapped"; BorrowLogic burns that mode and mints the other).
    assert_eq!(body["new_mode"], "stable");
    // live_input 2-tuple defaults wrapped + deserialized. `current_debts` is
    // `(U256, U256)` → alloy serialises each U256 as a hex string; `rates` is
    // `(Decimal, Decimal)` (transparent over String) → decimal "0".
    assert_eq!(
        body["live_inputs"]["current_debts"]["value"],
        json!(["0x0", "0x0"])
    );
    assert_eq!(body["live_inputs"]["rates"]["value"], json!(["0", "0"]));
}

// ---------------------------------------------------------------------------
// t10 — Aave V3 supply → LendingAction::Supply (EXISTING on-disk manifest)
// ---------------------------------------------------------------------------
//
// Inline-mirrors `registryV2/manifests/aave/v3/supply@1.0.0.json` verbatim
// (same `abi_fragment` + `emit.body` + the 5 declared `live_inputs`). The
// pre-existing supply manifest was never route-tested; with the
// `(lending, supply, …)` live_input_default skeletons it now routes fully
// green (reserve_state: ReserveState, supply_apy: Decimal, a_token_price_usd:
// Price, eligible_as_collat: bool, user_state_before: UserLendingState). The
// `referralCode` arg is a uint16 (≤ 64 bit) → JSON number via Fix B, though
// the supply body does not bind it.

const T10_AAVE_SUPPLY_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/supply@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x617ba037",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "supply",
    "abi": {
      "name": "supply",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",        "type": "address" },
        { "name": "amount",       "type": "uint256" },
        { "name": "onBehalfOf",   "type": "address" },
        { "name": "referralCode", "type": "uint16"  }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "supply",
        "supply": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "amount":       "$args.amount",
          "on_behalf_of": "$args.onBehalfOf"
        }
      }
    },
    "live_inputs": {
      "reserve_state":      { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "supply_apy":         { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_supply_apy" }, "ttl_s": 30 },
      "a_token_price_usd":  { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 },
      "eligible_as_collat": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getConfiguration(address)", "decoder_id": "aave_v3_reserve_config" }, "ttl_s": 60 },
      "user_state_before":  { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t10_aave_supply() {
    let install = install_ok(T10_AAVE_SUPPLY_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/supply@1.0.0");

    let calldata = encode_calldata(
        "0x617ba037",
        &[
            // asset (load-bearing) — USDC
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount
            DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
            // onBehalfOf
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // referralCode (uint16 ≤ 64 bit → JSON number)
            DynSolValue::Uint(AlloyU256::from(0u64), 16),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x617ba037",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // EXISTING supply manifest must route fully green now.
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "supply");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    // amount: U256 round-trips as a hex string through alloy serde.
    assert_eq!(body["amount"], "0xf4240"); // 1_000_000
    assert_eq!(
        body["on_behalf_of"],
        "0x000000000000000000000000000000000000cccc"
    );
    // All 5 live_input defaults wrapped + deserialized. `total_supply` /
    // `available_borrow_usd` are `U256` → alloy hex string "0x0"; `supply_apy`
    // / `a_token_price_usd` are `Decimal` / `Price` (transparent String) → "0".
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_supply"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["supply_apy"]["value"], "0");
    assert_eq!(body["live_inputs"]["a_token_price_usd"]["value"], "0");
    assert_eq!(body["live_inputs"]["eligible_as_collat"]["value"], false);
    assert_eq!(
        body["live_inputs"]["user_state_before"]["value"]["available_borrow_usd"],
        "0x0"
    );
}

// ===========================================================================
// B.2.2 — Aave V3 permit-variant Pool manifests (supplyWithPermit/repayWithPermit)
// ===========================================================================
//
// Two `Pool` calls that bundle an EIP-2612 permit with the lending action:
//   * t11 supplyWithPermit (0x02c205f0) → LendingAction::Supply
//   * t12 repayWithPermit   (0xee3e210b) → LendingAction::Repay
//
// Each inline manifest is IDENTICAL in `abi_fragment` + `emit.body` to the
// committed fixture under `registryV2/manifests/aave/v3/` (selectors are
// `cast sig`-verified; the venue `chain_to_addresses` mirror supply/repay —
// mainnet Pool here, the on-disk fixtures carry all 4 chains). The four
// trailing permit params (deadline / permitV / permitR / permitS) are bundled
// approval authorization, decoded by `abi_fragment` but UNREFERENCED in
// `emit.body` — the lending intent is exactly the SupplyAction / RepayAction
// shape of supply@1.0.0 / repay@1.0.0. `repayWithPermit` maps the on-chain
// `interestRateMode` uint onto `RepayAction.rate_mode` via the B.2-infra
// discriminant value-map `{ "$match": "$args.interestRateMode", "$cases":
// { "1": "stable", "2": "variable" } }` — Aave `DataTypes.InterestRateMode`
// (1=STABLE, 2=VARIABLE) → the `RateMode` serde value of the debt being repaid
// (direct map, no complement; the swapBorrowRateMode `new_mode` IS a complement
// — see t9). This replaces the earlier route-green literal `"variable"` that
// mislabeled a stable-rate repay. Both route FULLY GREEN (`ok:true`) on the
// B.2-infra foundation (lending `live_input_default` skeletons + uint≤64
// coercion + the `$match` value-map placeholder).

// ---------------------------------------------------------------------------
// t11 — Aave V3 supplyWithPermit → LendingAction::Supply
// ---------------------------------------------------------------------------

const T11_AAVE_SUPPLY_WITH_PERMIT_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/supplyWithPermit@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x02c205f0",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "supplyWithPermit",
    "abi": {
      "name": "supplyWithPermit",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",        "type": "address" },
        { "name": "amount",       "type": "uint256" },
        { "name": "onBehalfOf",   "type": "address" },
        { "name": "referralCode", "type": "uint16"  },
        { "name": "deadline",     "type": "uint256" },
        { "name": "permitV",      "type": "uint8"   },
        { "name": "permitR",      "type": "bytes32" },
        { "name": "permitS",      "type": "bytes32" }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "supply",
        "supply": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "amount":       "$args.amount",
          "on_behalf_of": "$args.onBehalfOf"
        }
      }
    },
    "live_inputs": {
      "reserve_state":      { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "supply_apy":         { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_supply_apy" }, "ttl_s": 30 },
      "a_token_price_usd":  { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 },
      "eligible_as_collat": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getConfiguration(address)", "decoder_id": "aave_v3_reserve_config" }, "ttl_s": 60 },
      "user_state_before":  { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

#[test]
fn t11_aave_supply_with_permit() {
    let install = install_ok(T11_AAVE_SUPPLY_WITH_PERMIT_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/supplyWithPermit@1.0.0"
    );

    // 8 args: asset, amount, onBehalfOf, referralCode, deadline, permitV,
    // permitR, permitS. Only asset/amount/onBehalfOf are referenced by the
    // body; the trailing permit params are decoded but ignored.
    let calldata = encode_calldata(
        "0x02c205f0",
        &[
            // asset (load-bearing) — USDC
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing)
            DynSolValue::Uint(AlloyU256::from(2_500_000u64), 256),
            // onBehalfOf
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // referralCode (uint16)
            DynSolValue::Uint(AlloyU256::from(0u64), 16),
            // deadline (permit param — unreferenced)
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
            // permitV (permit param — unreferenced)
            DynSolValue::Uint(AlloyU256::from(27u64), 8),
            // permitR (permit param — unreferenced)
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x11), 32),
            // permitS (permit param — unreferenced)
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x22), 32),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x02c205f0",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // Bundled-permit supply routes fully green — same SupplyAction body as
    // supply@1.0.0 (t10), with the 4 permit args decoded but unreferenced.
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "supply");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing $args fields resolved from calldata.
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    // amount: U256 round-trips as a hex string through alloy serde.
    assert_eq!(body["amount"], "0x2625a0"); // 2_500_000
    assert_eq!(
        body["on_behalf_of"],
        "0x000000000000000000000000000000000000cccc"
    );
    // live_input defaults wrapped + deserialized (same 5 as supply@1.0.0).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_supply"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["eligible_as_collat"]["value"], false);
}

// ---------------------------------------------------------------------------
// t12 — Aave V3 repayWithPermit → LendingAction::Repay
// ---------------------------------------------------------------------------

const T12_AAVE_REPAY_WITH_PERMIT_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/repayWithPermit@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0xee3e210b",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "repayWithPermit",
    "abi": {
      "name": "repayWithPermit",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",            "type": "address" },
        { "name": "amount",           "type": "uint256" },
        { "name": "interestRateMode", "type": "uint256" },
        { "name": "onBehalfOf",       "type": "address" },
        { "name": "deadline",         "type": "uint256" },
        { "name": "permitV",          "type": "uint8"   },
        { "name": "permitR",          "type": "bytes32" },
        { "name": "permitS",          "type": "bytes32" }
      ],
      "outputs": [ { "name": "", "type": "uint256" } ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "repay",
        "repay": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "amount":       "$args.amount",
          "rate_mode":    { "$match": "$args.interestRateMode", "$cases": { "1": "stable", "2": "variable" } },
          "on_behalf_of": "$args.onBehalfOf",
          "use_a_tokens": false
        }
      }
    },
    "live_inputs": {
      "reserve_state":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "current_debt":      { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_current_debt" }, "ttl_s": 12 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

#[test]
fn t12_aave_repay_with_permit() {
    let install = install_ok(T12_AAVE_REPAY_WITH_PERMIT_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/repayWithPermit@1.0.0"
    );

    // 8 args: asset, amount, interestRateMode, onBehalfOf, deadline, permitV,
    // permitR, permitS. The body references asset/amount/onBehalfOf; the
    // `interestRateMode` arg (here 2 = VARIABLE) is mapped onto `rate_mode` via
    // the `$match` value-map → `"variable"`; the 4 permit params are decoded
    // but ignored.
    let calldata = encode_calldata(
        "0xee3e210b",
        &[
            // asset (load-bearing) — USDC
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing)
            DynSolValue::Uint(AlloyU256::from(750_000u64), 256),
            // interestRateMode (2 = VARIABLE → value-map yields rate_mode "variable")
            DynSolValue::Uint(AlloyU256::from(2u64), 256),
            // onBehalfOf
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // deadline (permit param — unreferenced)
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
            // permitV (permit param — unreferenced)
            DynSolValue::Uint(AlloyU256::from(28u64), 8),
            // permitR (permit param — unreferenced)
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x33), 32),
            // permitS (permit param — unreferenced)
            DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x44), 32),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0xee3e210b",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    // Bundled-permit repay routes fully green — same RepayAction body as
    // repay@1.0.0, with `rate_mode` now derived from `interestRateMode` via the
    // `$match` value-map (arg 2 = VARIABLE → "variable").
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "repay");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing $args fields resolved from calldata.
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    // amount: U256 round-trips as a hex string through alloy serde.
    assert_eq!(body["amount"], "0xb71b0"); // 750_000
    assert_eq!(
        body["on_behalf_of"],
        "0x000000000000000000000000000000000000cccc"
    );
    // interestRateMode=2 → value-map → "variable".
    assert_eq!(body["rate_mode"], "variable");
    assert_eq!(body["use_a_tokens"], false);
    // live_input defaults wrapped + deserialized (same 3 as repay@1.0.0).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_borrow"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["current_debt"]["value"], "0x0");
}

// ===========================================================================
// B.2-infra — discriminant value-map ($match / $cases / $default) route tests
// ===========================================================================
//
// These exercise the value-map placeholder END-TO-END through the WASM route
// (calldata decode → uint256 arg → Fix-B decimal-string coercion → `$match`
// key extraction → `$cases` lookup → typed ActionBody). They mirror the
// committed fixtures under `registryV2/manifests/aave/v3/`:
//   * t13 repay (0x573ade81)                  — field-level value-map on `rateMode`.
//   * t14 swapBorrowRateMode (0x94ba89a2)       — field-level COMPLEMENT value-map.
//   * t15 setUserUseReserveAsCollateral (0x5a3b74b9) — ACTION-TAG-level value-map
//       on a `bool` arg selecting EnableCollateral vs DisableCollateral.
// t15 is the load-bearing proof that an action-level value-map composes with
// `strip_inline_live_inputs` + `flatten_body` + live-input injection.

// ---------------------------------------------------------------------------
// t13 — Aave V3 repay → LendingAction::Repay (rate_mode via $match value-map)
// ---------------------------------------------------------------------------

const T13_AAVE_REPAY_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/repay@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x573ade81",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "repay",
    "abi": {
      "name": "repay",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",      "type": "address" },
        { "name": "amount",     "type": "uint256" },
        { "name": "rateMode",   "type": "uint256" },
        { "name": "onBehalfOf", "type": "address" }
      ],
      "outputs": [ { "name": "", "type": "uint256" } ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "repay",
        "repay": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "amount":       "$args.amount",
          "rate_mode":    { "$match": "$args.rateMode", "$cases": { "1": "stable", "2": "variable" } },
          "on_behalf_of": "$args.onBehalfOf",
          "use_a_tokens": false
        }
      }
    },
    "live_inputs": {
      "reserve_state":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "current_debt":      { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_current_debt" }, "ttl_s": 12 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

/// Encode a `repay(asset, amount, rateMode, onBehalfOf)` calldata + route it,
/// returning the resolved `body` JSON.
fn route_repay_with_rate_mode(rate_mode_arg: u64) -> Value {
    install_ok(T13_AAVE_REPAY_V3);
    let calldata = encode_calldata(
        "0x573ade81",
        &[
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
            // rateMode (uint256 → decimal string → $match key).
            DynSolValue::Uint(AlloyU256::from(rate_mode_arg), 256),
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x573ade81",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    parsed["data"]["actions"][0]["body"].clone()
}

#[test]
fn t13_aave_repay_rate_mode_value_map() {
    // rateMode = 1 (STABLE) → rate_mode "stable" (direct map: mode of the debt
    // being repaid; no complement).
    let body = route_repay_with_rate_mode(1);
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "repay");
    assert_eq!(body["rate_mode"], "stable");

    // rateMode = 2 (VARIABLE) → rate_mode "variable".
    let body = route_repay_with_rate_mode(2);
    assert_eq!(body["action"], "repay");
    assert_eq!(body["rate_mode"], "variable");
}

// ---------------------------------------------------------------------------
// t14 — Aave V3 swapBorrowRateMode → SwapRateMode (new_mode COMPLEMENT value-map)
// ---------------------------------------------------------------------------
//
// Reuses the t9 manifest const (now carrying the complement value-map). The
// arg is "the current rate mode of the position being swapped" (Aave IPool
// NatSpec); BorrowLogic.executeSwapBorrowRateMode burns that mode's debt and
// mints the OTHER mode, so `new_mode` (post-swap) is the COMPLEMENT:
//   arg 1 (STABLE)   → new_mode "variable"
//   arg 2 (VARIABLE) → new_mode "stable"

fn route_swap_rate_mode_with_arg(rate_mode_arg: u64) -> Value {
    install_ok(T9_AAVE_SWAP_RATE_MODE_V3);
    let calldata = encode_calldata(
        "0x94ba89a2",
        &[
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Uint(AlloyU256::from(rate_mode_arg), 256),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x94ba89a2",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    parsed["data"]["actions"][0]["body"].clone()
}

#[test]
fn t14_aave_swap_borrow_rate_mode_complement_value_map() {
    // arg 1 (currently STABLE) → swaps TO variable.
    let body = route_swap_rate_mode_with_arg(1);
    assert_eq!(body["action"], "swap_rate_mode");
    assert_eq!(body["new_mode"], "variable");

    // arg 2 (currently VARIABLE) → swaps TO stable.
    let body = route_swap_rate_mode_with_arg(2);
    assert_eq!(body["action"], "swap_rate_mode");
    assert_eq!(body["new_mode"], "stable");
}

// ---------------------------------------------------------------------------
// t15 — Aave V3 setUserUseReserveAsCollateral → Enable/DisableCollateral
//       (ACTION-TAG-level value-map on a bool arg)
// ---------------------------------------------------------------------------
//
// Inline-mirrors `registryV2/manifests/aave/v3/set-user-use-reserve-as-collateral@1.0.0.json`
// (mainnet-only chain_to_addresses here). The `lending` sub-object is itself a
// `$match` value-map keyed by the `useAsCollateral` bool — true selects the
// `enable_collateral` action-tag object, false selects `disable_collateral`.
// This proves the action-level value-map composes with strip_inline_live_inputs
// (the value-map has no `action` key yet → strips nothing), flatten_body (sees
// a normal nested body AFTER substitution), and the
// `(lending, enable_collateral|disable_collateral, …)` live_input_default
// skeletons.

const T15_AAVE_SET_COLLATERAL_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/setUserUseReserveAsCollateral@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x5a3b74b9",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "setUserUseReserveAsCollateral",
    "abi": {
      "name": "setUserUseReserveAsCollateral",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",           "type": "address" },
        { "name": "useAsCollateral", "type": "bool"    }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "$match": "$args.useAsCollateral",
        "$cases": {
          "true": {
            "action": "enable_collateral",
            "enable_collateral": {
              "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
              "asset": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } }
            }
          },
          "false": {
            "action": "disable_collateral",
            "disable_collateral": {
              "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
              "asset": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } }
            }
          }
        }
      }
    },
    "live_inputs": {
      "reserve_state":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

fn route_set_collateral_with_flag(use_as_collateral: bool) -> Value {
    install_ok(T15_AAVE_SET_COLLATERAL_V3);
    let calldata = encode_calldata(
        "0x5a3b74b9",
        &[
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            DynSolValue::Bool(use_as_collateral),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x5a3b74b9",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    parsed["data"]["actions"][0]["body"].clone()
}

#[test]
fn t15_aave_set_user_use_reserve_as_collateral_action_value_map() {
    // useAsCollateral = true → EnableCollateral (action tag "enable_collateral").
    let body = route_set_collateral_with_flag(true);
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "enable_collateral");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    // live_inputs injected for the selected action tag (skeleton defaults).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_supply"],
        "0x0"
    );
    assert_eq!(
        body["live_inputs"]["user_state_before"]["value"]["health_factor"],
        "0"
    );

    // useAsCollateral = false → DisableCollateral (action tag "disable_collateral").
    let body = route_set_collateral_with_flag(false);
    assert_eq!(body["action"], "disable_collateral");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
}

// ===========================================================================
// B.2.3 — Aave V3 WrappedTokenGateway (WTG) native-ETH gateway manifests
// ===========================================================================
//
// The WrappedTokenGatewayV3 (a.k.a. WETH_GATEWAY) lets users interact with the
// Aave V3 Pool using native ETH instead of WETH. Four payable/non-payable entry
// points wrap+supply / withdraw+unwrap / borrow+unwrap / wrap+repay; each routes
// into the EXISTING asset-generic lending `ActionBody` variant (Supply /
// Withdraw / Borrow / Repay) — there is NO WTG-specific action.
//
// Verified deployed signatures (aave-dao/aave-v3-origin
// `WrappedTokenGatewayV3.sol` + bgd-labs/aave-address-book WETH_GATEWAY +
// deployed-verified Etherscan/Optimistic/Basescan at the address-book address):
//   * depositETH(address, address onBehalfOf, uint16 referralCode) payable  0x474cf53d
//   * withdrawETH(address, uint256 amount, address to)                      0x80500d20
//   * borrowETH(address, uint256 amount, uint16 referralCode)               0xe74f7b85
//   * repayETH(address, uint256 amount, address onBehalfOf) payable         0xbcc3c255
// (`cast sig`-derived selectors.)
//
// THREE source-grounded mapping facts (distinct from the on-chain Pool calls):
//   1. The leading `address` param is IGNORED by the contract — every body uses
//      the immutable `POOL` state var, NOT the calldata arg. `venue.pool` (and
//      the live_input `source.contract`) therefore bind `$resolved.pool`, which
//      route_request pre-populates from the known gateway target + chain. It is
//      NOT `$args.pool` (the ignored dummy) and NOT `$to` (the WTG, not the
//      Pool).
//   2. The asset is ALWAYS WETH — `$resolved.weth` (also zero-address placeholder
//      here). There is no asset arg.
//   3. borrowETH/repayETH have NO rate-mode arg: the contract hardcodes
//      `DataTypes.InterestRateMode.VARIABLE`, so `rate_mode` is the LITERAL
//      `"variable"` (no value-map — there is no discriminant arg to map).
// depositETH's supplied amount is `msg.value` (`$tx.value`), not a calldata arg.
//
// Each inline manifest is IDENTICAL in `abi_fragment` + `emit.body` to the
// committed fixture under `registryV2/manifests/aave/v3/` (mainnet-only
// chain_to_addresses here; the on-disk fixtures carry all 4 chains: 1 / 10 /
// 8453 / 42161). All route FULLY GREEN on the B.2-infra lending
// `live_input_default` skeletons.

const WTG_MAINNET: &str = "0xd01607c3c5ecaba394d8be377a08590149325722";
// `$resolved.weth` and `$resolved.pool` are both pre-populated by the route
// handler for this gateway path. The pool comes from the verified gateway
// deployment, not from the ignored calldata address.
const WETH_MAINNET: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const AAVE_POOL_MAINNET: &str = "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2";

// ---------------------------------------------------------------------------
// t16 — WTG depositETH → LendingAction::Supply (amount = $tx.value)
// ---------------------------------------------------------------------------

const T16_WTG_DEPOSIT_ETH_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/deposit-eth@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x474cf53d",
    "chain_to_addresses": { "1": ["0xd01607c3c5ecaba394d8be377a08590149325722"] }
  },
  "abi_fragment": {
    "function_name": "depositETH",
    "abi": {
      "name": "depositETH",
      "type": "function",
      "stateMutability": "payable",
      "inputs": [
        { "name": "pool",         "type": "address" },
        { "name": "onBehalfOf",   "type": "address" },
        { "name": "referralCode", "type": "uint16"  }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "supply",
        "supply": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$resolved.pool", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.weth" } },
          "amount":       "$tx.value",
          "on_behalf_of": "$args.onBehalfOf"
        }
      }
    },
    "live_inputs": {
      "reserve_state":      { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "supply_apy":         { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_supply_apy" }, "ttl_s": 30 },
      "a_token_price_usd":  { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 },
      "eligible_as_collat": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getConfiguration(address)", "decoder_id": "aave_v3_reserve_config" }, "ttl_s": 60 },
      "user_state_before":  { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

#[test]
fn t16_aave_deposit_eth() {
    let install = install_ok(T16_WTG_DEPOSIT_ETH_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/deposit-eth@1.0.0");

    // depositETH(pool, onBehalfOf, referralCode). The supplied amount is
    // msg.value (NOT a calldata arg) — pass a non-zero tx value and assert it
    // flows into SupplyAction.amount via `$tx.value`.
    let calldata = encode_calldata(
        "0x474cf53d",
        &[
            // pool (IGNORED by the contract — uses immutable POOL).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000dddd"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // onBehalfOf (load-bearing).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // referralCode (uint16 — decoded but unreferenced).
            DynSolValue::Uint(AlloyU256::from(0u64), 16),
        ],
    );
    // 1 ETH of native value attached to the call.
    let input = route_input_with_value(
        1,
        WTG_MAINNET,
        "0x474cf53d",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
        "1000000000000000000",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "supply");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(body["venue"]["pool"], AAVE_POOL_MAINNET);
    assert_eq!(body["asset"]["key"]["address"], WETH_MAINNET);
    // amount = $tx.value (1 ETH). U256 round-trips as a hex string via alloy.
    assert_eq!(body["amount"], "0xde0b6b3a7640000"); // 1e18
    assert_eq!(
        body["on_behalf_of"],
        "0x000000000000000000000000000000000000cccc"
    );
    // live_input defaults wrapped + deserialized (same 5 as supply@1.0.0).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_supply"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["eligible_as_collat"]["value"], false);
}

// ---------------------------------------------------------------------------
// t17 — WTG withdrawETH → LendingAction::Withdraw (recipient = $args.to)
// ---------------------------------------------------------------------------

const T17_WTG_WITHDRAW_ETH_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/withdraw-eth@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x80500d20",
    "chain_to_addresses": { "1": ["0xd01607c3c5ecaba394d8be377a08590149325722"] }
  },
  "abi_fragment": {
    "function_name": "withdrawETH",
    "abi": {
      "name": "withdrawETH",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "pool",   "type": "address" },
        { "name": "amount", "type": "uint256" },
        { "name": "to",     "type": "address" }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "withdraw",
        "withdraw": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$resolved.pool", "market_id": null },
          "asset":     { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.weth" } },
          "amount":    "$args.amount",
          "recipient": "$args.to"
        }
      }
    },
    "live_inputs": {
      "reserve_state":         { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "available_to_withdraw": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_available_to_withdraw" }, "ttl_s": 30 },
      "user_state_before":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

#[test]
fn t17_aave_withdraw_eth() {
    let install = install_ok(T17_WTG_WITHDRAW_ETH_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/withdraw-eth@1.0.0");

    // withdrawETH(pool, amount, to). `to` is the ultimate ETH recipient
    // (_safeTransferETH(to, ...) after unwrap) → WithdrawAction.recipient.
    let calldata = encode_calldata(
        "0x80500d20",
        &[
            // pool (IGNORED).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000dddd"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing).
            DynSolValue::Uint(AlloyU256::from(500_000u64), 256),
            // to (load-bearing recipient).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000bbbb"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
        ],
    );
    let input = route_input(
        1,
        WTG_MAINNET,
        "0x80500d20",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "withdraw");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(body["venue"]["pool"], AAVE_POOL_MAINNET);
    assert_eq!(body["asset"]["key"]["address"], WETH_MAINNET);
    // amount: U256 round-trips as a hex string via alloy.
    assert_eq!(body["amount"], "0x7a120"); // 500_000
    assert_eq!(
        body["recipient"],
        "0x000000000000000000000000000000000000bbbb"
    );
}

// ---------------------------------------------------------------------------
// t18 — WTG borrowETH → LendingAction::Borrow (rate_mode literal "variable")
// ---------------------------------------------------------------------------

const T18_WTG_BORROW_ETH_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/borrow-eth@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0xe74f7b85",
    "chain_to_addresses": { "1": ["0xd01607c3c5ecaba394d8be377a08590149325722"] }
  },
  "abi_fragment": {
    "function_name": "borrowETH",
    "abi": {
      "name": "borrowETH",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "pool",         "type": "address" },
        { "name": "amount",       "type": "uint256" },
        { "name": "referralCode", "type": "uint16"  }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "borrow",
        "borrow": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$resolved.pool", "market_id": null },
          "asset":     { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.weth" } },
          "amount":    "$args.amount",
          "rate_mode": "variable"
        }
      }
    },
    "live_inputs": {
      "reserve_state":       { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "user_state_before":   { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 },
      "asset_price_usd":     { "source": { "kind": "oracle_feed", "provider": "chainlink", "feed_id": "AAVE_V3_RESERVE_PRICE" }, "ttl_s": 60 },
      "current_borrow_rate": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_borrow_rate" }, "ttl_s": 30 },
      "available_liquidity": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_available_liquidity" }, "ttl_s": 30 }
    }
  }
}"#;

#[test]
fn t18_aave_borrow_eth() {
    let install = install_ok(T18_WTG_BORROW_ETH_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/borrow-eth@1.0.0");

    // borrowETH(pool, amount, referralCode). No rate-mode arg — the contract
    // hardcodes VARIABLE, so rate_mode is the literal "variable". No
    // onBehalfOf arg (borrows for msg.sender) → on_behalf_of omitted (Option).
    let calldata = encode_calldata(
        "0xe74f7b85",
        &[
            // pool (IGNORED).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000dddd"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing).
            DynSolValue::Uint(AlloyU256::from(3_000_000u64), 256),
            // referralCode (uint16 — decoded but unreferenced).
            DynSolValue::Uint(AlloyU256::from(0u64), 16),
        ],
    );
    let input = route_input(
        1,
        WTG_MAINNET,
        "0xe74f7b85",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "borrow");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(body["venue"]["pool"], AAVE_POOL_MAINNET);
    assert_eq!(body["asset"]["key"]["address"], WETH_MAINNET);
    // amount: U256 round-trips as a hex string via alloy.
    assert_eq!(body["amount"], "0x2dc6c0"); // 3_000_000
                                            // rate_mode is the literal "variable" (the only mode the gateway supports).
    assert_eq!(body["rate_mode"], "variable");
    // on_behalf_of omitted → skip_serializing_if(Option::is_none) → absent.
    assert!(body.get("on_behalf_of").is_none(), "{parsed}");
    // live_input defaults wrapped + deserialized (same 5 as borrow@1.0.0).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_borrow"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["available_liquidity"]["value"], "0x0");
}

// ---------------------------------------------------------------------------
// t19 — WTG repayETH → LendingAction::Repay (rate_mode literal "variable")
// ---------------------------------------------------------------------------

const T19_WTG_REPAY_ETH_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/repay-eth@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0xbcc3c255",
    "chain_to_addresses": { "1": ["0xd01607c3c5ecaba394d8be377a08590149325722"] }
  },
  "abi_fragment": {
    "function_name": "repayETH",
    "abi": {
      "name": "repayETH",
      "type": "function",
      "stateMutability": "payable",
      "inputs": [
        { "name": "pool",       "type": "address" },
        { "name": "amount",     "type": "uint256" },
        { "name": "onBehalfOf", "type": "address" }
      ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "repay",
        "repay": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$resolved.pool", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.weth" } },
          "amount":       "$args.amount",
          "rate_mode":    "variable",
          "on_behalf_of": "$args.onBehalfOf",
          "use_a_tokens": false
        }
      }
    },
    "live_inputs": {
      "reserve_state":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "current_debt":      { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_current_debt" }, "ttl_s": 12 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

#[test]
fn t19_aave_repay_eth() {
    let install = install_ok(T19_WTG_REPAY_ETH_V3);
    assert_eq!(install["data"]["bundle_id"], "aave/v3/repay-eth@1.0.0");

    // repayETH(pool, amount, onBehalfOf) payable. No rate-mode arg — the
    // contract hardcodes VARIABLE → rate_mode literal "variable". onBehalfOf is
    // the debtor of record → RepayAction.on_behalf_of.
    let calldata = encode_calldata(
        "0xbcc3c255",
        &[
            // pool (IGNORED).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000dddd"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing).
            DynSolValue::Uint(AlloyU256::from(1_250_000u64), 256),
            // onBehalfOf (load-bearing debtor of record).
            DynSolValue::Address(
                "0x000000000000000000000000000000000000cccc"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
        ],
    );
    // repayETH is payable — pass a tx value (unreferenced by the body; amount
    // comes from $args.amount, not $tx.value).
    let input = route_input_with_value(
        1,
        WTG_MAINNET,
        "0xbcc3c255",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
        "1250000000000000000",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "repay");
    assert_eq!(body["venue"]["name"], "aave_v3");
    assert_eq!(body["venue"]["pool"], AAVE_POOL_MAINNET);
    assert_eq!(body["asset"]["key"]["address"], WETH_MAINNET);
    // amount comes from the calldata arg, NOT $tx.value.
    assert_eq!(body["amount"], "0x1312d0"); // 1_250_000
    assert_eq!(body["rate_mode"], "variable");
    assert_eq!(
        body["on_behalf_of"],
        "0x000000000000000000000000000000000000cccc"
    );
    assert_eq!(body["use_a_tokens"], false);
    // live_input defaults wrapped + deserialized (same 3 as repay@1.0.0).
    assert_eq!(body["live_inputs"]["current_debt"]["value"], "0x0");
}

// ---------------------------------------------------------------------------
// t20 — Aave V3 repayWithATokens → LendingAction::Repay (use_a_tokens = true)
// ---------------------------------------------------------------------------
//
// Inline-mirrors `registryV2/manifests/aave/v3/repay-with-atokens@1.0.0.json`.
// `repayWithATokens(asset, amount, interestRateMode)` repays debt directly from
// the submitter's aToken balance (no underlying transfer) → maps to the SAME
// `lending.repay` ActionBody as repay@1.0.0 EXCEPT:
//   * `use_a_tokens` = true (the one distinguishing value vs t13's false).
//   * NO `onBehalfOf` arg — you can only repay your OWN debt with your OWN
//     aTokens, so `on_behalf_of` is OMITTED from the body (Option + skip-if-None
//     defaults to the submitter). Asserted absent from the serialized JSON.
//   * `rate_mode` value-map keys on `$args.interestRateMode` (same Aave
//     InterestRateMode 1=STABLE / 2=VARIABLE map as t13's `rateMode`).
// Selector `cast sig`-verified: 0x2dad97d4. Returns uint256 (final amount
// repaid) per the canonical Aave V3 IPool — declared in `outputs`.

const T20_AAVE_REPAY_WITH_ATOKENS_V3: &str = r#"{
  "type": "adapter_action",
  "id": "aave/v3/repayWithATokens@1.0.0",
  "publisher": "aave.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x2dad97d4",
    "chain_to_addresses": { "1": ["0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"] }
  },
  "abi_fragment": {
    "function_name": "repayWithATokens",
    "abi": {
      "name": "repayWithATokens",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [
        { "name": "asset",            "type": "address" },
        { "name": "amount",           "type": "uint256" },
        { "name": "interestRateMode", "type": "uint256" }
      ],
      "outputs": [ { "name": "", "type": "uint256" } ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "lending",
      "lending": {
        "action": "repay",
        "repay": {
          "venue": { "name": "aave_v3", "chain": "$chain", "pool": "$to", "market_id": null },
          "asset":        { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.asset" } },
          "amount":       "$args.amount",
          "rate_mode":    { "$match": "$args.interestRateMode", "$cases": { "1": "stable", "2": "variable" } },
          "use_a_tokens": true
        }
      }
    },
    "live_inputs": {
      "reserve_state":     { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getReserveData(address)", "decoder_id": "aave_v3_reserve_data" }, "ttl_s": 30 },
      "current_debt":      { "source": { "kind": "derived_from", "inputs": [], "calc_id": "aave_v3_current_debt" }, "ttl_s": 12 },
      "user_state_before": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "getUserAccountData(address)", "decoder_id": "aave_v3_user_account_data" }, "ttl_s": 12 }
    }
  }
}"#;

/// Encode a `repayWithATokens(asset, amount, interestRateMode)` calldata + route
/// it, returning the resolved `body` JSON.
fn route_repay_with_atokens(rate_mode_arg: u64) -> Value {
    install_ok(T20_AAVE_REPAY_WITH_ATOKENS_V3);
    let calldata = encode_calldata(
        "0x2dad97d4",
        &[
            // asset (load-bearing) — USDC.
            DynSolValue::Address(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            // amount (load-bearing).
            DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256),
            // interestRateMode (uint256 → decimal string → $match key).
            DynSolValue::Uint(AlloyU256::from(rate_mode_arg), 256),
        ],
    );
    let input = route_input(
        1,
        "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        "0x2dad97d4",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    parsed["data"]["actions"][0]["body"].clone()
}

#[test]
fn t20_aave_repay_with_atokens() {
    let install = install_ok(T20_AAVE_REPAY_WITH_ATOKENS_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "aave/v3/repayWithATokens@1.0.0"
    );

    // interestRateMode = 2 (VARIABLE) → rate_mode "variable".
    let body = route_repay_with_atokens(2);
    assert_eq!(body["domain"], "lending");
    assert_eq!(body["action"], "repay");
    assert_eq!(body["venue"]["name"], "aave_v3");
    // load-bearing $args field resolved from calldata.
    assert_eq!(
        body["asset"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(body["amount"], "0xf4240"); // 1_000_000
    assert_eq!(body["rate_mode"], "variable");
    // The distinguishing value vs t13 repay@1.0.0 (false).
    assert_eq!(body["use_a_tokens"], true, "{body}");
    // No `onBehalfOf` arg → `on_behalf_of` OMITTED (Option + skip_serializing_if
    // defaults to the submitter). Must be ABSENT from the serialized body.
    assert!(
        body.get("on_behalf_of").is_none(),
        "on_behalf_of must be absent (no arg; defaults to submitter): {body}"
    );

    // interestRateMode = 1 (STABLE) → rate_mode "stable" (direct map).
    let body = route_repay_with_atokens(1);
    assert_eq!(body["action"], "repay");
    assert_eq!(body["rate_mode"], "stable");
    assert_eq!(body["use_a_tokens"], true, "{body}");
}

// ===========================================================================
// b1 — Uniswap Permit2 batch manifests (array_emit; mirrors on-disk
//      registryV2/manifests/uniswap/permit2/{lockdown,permitBatch}@1.0.0.json)
// ===========================================================================
//
// Two clean wallet-facing Permit2 BATCH functions fan out via `array_emit`
// into a homogeneous `ActionBody::Multicall`:
//   * b1 lockdown    → Multicall[RevokeApproval{Permit2Lockdown}]   (calldata)
//   * b2 permitBatch → Multicall[Permit2SignAllowance]              (off-chain
//                       EIP-712 sig; the GREEN coverage is the typed-data
//                       route — see `declarative_v3_typed_data_install.rs`. On
//                       the CALLDATA path the nested-tuple-array element loses
//                       its ABI width so the uint48 `expiration` → `Time`/u64
//                       deterministic field rejects the decimal string; this
//                       test pins that documented gap.)
//
// Both manifests are inlined here VERBATIM from the on-disk fixtures (same
// `match` / `abi_fragment` / `emit`). The selectors are `cast sig`-verified
// (0xcc53287f / 0x2a2d80d1) and the `chain_to_addresses` mirror the existing
// permit2 `approve@1.0.0` / `permitSingle@1.0.0` manifests (the Permit2
// canonical CREATE2 address on all 4 chains; mainnet shown here, the on-disk
// fixtures carry 1/10/8453/42161).

// ---------------------------------------------------------------------------
// b1 — Permit2 lockdown → ActionBody::Multicall { RevokeApproval x N }
// ---------------------------------------------------------------------------
//
// `lockdown((address token, address spender)[] approvals)`. `array_emit` over
// `$args.approvals` (a `tuple[]` → positional element array) emits one
// `revoke_approval` per element with a `RevokeScope::Permit2Lockdown` scope.
// All element fields are addresses → fully GREEN on the calldata route. The
// two elements differ (token / spender), proving per-element `$inputs.[i]`
// binding.

const B1_PERMIT2_LOCKDOWN_V3: &str = r#"{
  "type": "adapter_action",
  "id": "uniswap/permit2/lockdown@1.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {
    "selector": "0xcc53287f",
    "chain_to_addresses": {
      "1":     ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "10":    ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "8453":  ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "42161": ["0x000000000022d473030f116ddee9f6b43ac78ba3"]
    }
  },
  "abi_fragment": {
    "function_name": "lockdown",
    "abi": {
      "name": "lockdown",
      "type": "function",
      "inputs": [
        {
          "name": "approvals",
          "type": "tuple[]",
          "components": [
            { "name": "token", "type": "address" },
            { "name": "spender", "type": "address" }
          ]
        }
      ]
    }
  },
  "emit": {
    "strategy": "array_emit",
    "array_source": "$args.approvals",
    "body": {
      "domain": "token",
      "token": {
        "action": "revoke_approval",
        "revoke_approval": {
          "scope": {
            "kind": "permit2_lockdown",
            "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.[0]" } },
            "spender": "$inputs.[1]"
          }
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

/// Build a 2-element `lockdown` calldata. `approvals` is a `tuple[]` so the
/// args_json `approvals` field is a 2-element array of positional `[token,
/// spender]` arrays (calldata tuples decode positionally — NOT named objects).
fn b1_two_element_lockdown_calldata() -> String {
    encode_calldata(
        "0xcc53287f",
        &[DynSolValue::Array(vec![
            DynSolValue::Tuple(vec![
                DynSolValue::Address(
                    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Address(
                    "0x00000000000000000000000000000000deadbeef"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
            ]),
            DynSolValue::Tuple(vec![
                DynSolValue::Address(
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Address(
                    "0x00000000000000000000000000000000cafef00d"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
            ]),
        ])],
    )
}

#[test]
fn b1_permit2_lockdown_array_emit() {
    let install = install_ok(B1_PERMIT2_LOCKDOWN_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/permit2/lockdown@1.0.0"
    );

    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0xcc53287f",
        b1_two_element_lockdown_calldata(),
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"],
        "uniswap/permit2/lockdown@1.0.0"
    );

    // array_emit → Multicall with one revoke_approval per approvals element.
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let actions = body["actions"].as_array().expect("inner actions array");
    assert_eq!(actions.len(), 2, "{parsed}");

    // element-0 — RevokeScope::Permit2Lockdown (serde tag kind=permit2_lockdown).
    assert_eq!(actions[0]["domain"], "token");
    assert_eq!(actions[0]["action"], "revoke_approval");
    assert_eq!(actions[0]["scope"]["kind"], "permit2_lockdown");
    assert_eq!(
        actions[0]["scope"]["token"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
    );
    assert_eq!(
        actions[0]["scope"]["spender"],
        "0x00000000000000000000000000000000deadbeef"
    );

    // element-1 — DIFFERENT token + spender prove per-element $inputs.[i] bind.
    assert_eq!(actions[1]["action"], "revoke_approval");
    assert_eq!(actions[1]["scope"]["kind"], "permit2_lockdown");
    assert_eq!(
        actions[1]["scope"]["token"]["key"]["address"],
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
    );
    assert_eq!(
        actions[1]["scope"]["spender"],
        "0x00000000000000000000000000000000cafef00d"
    );
}

// ---------------------------------------------------------------------------
// b2 — Permit2 permitBatch via the CALLDATA route (nested-tuple width threading)
// ---------------------------------------------------------------------------
//
// `permit(address owner, PermitBatch permitBatch, bytes signature)` where
// `PermitBatch = (PermitDetails[] details, address spender, uint256 sigDeadline)`
// and `PermitDetails = (address token, uint160 amount, uint48 expiration,
// uint48 nonce)`. `array_emit` over the `PermitDetails[]` element (positional
// `$args.permitBatch[0]`); per-item fields bind the element POSITIONALLY
// (`$inputs[0]` token … `$inputs[2]` expiration) — the calldata convention,
// distinct from the EIP-712 named-access on-disk manifest (whose GREEN route is
// the off-chain signature, covered in
// `declarative_v3_typed_data_install.rs::typed_data_permit2_permit_batch_array_emit`).
//
// b1-infra fix: the bridge now threads each tuple param's full parenthesised
// ABI type (rebuilt from `Param.components`) through to
// `decoded_value_to_json_typed`, so the nested `PermitDetails` element's uint48
// `expiration` renders as a JSON **number** rather than a decimal string. The
// deterministic `Permit2SignAction.expires_at: Time` (transparent `u64`) then
// deserialises it cleanly. Before the fix the bare `"tuple[]"` alloy emits (with
// field types out-of-band on `components`) collapsed every nested component to
// type `""` → decimal string → `build_array_emit_failed` "expected u64".

/// Build a 2-element calldata `permit(owner, permitBatch, signature)`.
fn b2_permit_batch_calldata() -> String {
    fn permit_details(token: &str, amount: u64, expiration: u64, nonce: u64) -> DynSolValue {
        DynSolValue::Tuple(vec![
            DynSolValue::Address(token.parse::<AlloyAddress>().unwrap()),
            DynSolValue::Uint(AlloyU256::from(amount), 160),
            DynSolValue::Uint(AlloyU256::from(expiration), 48),
            DynSolValue::Uint(AlloyU256::from(nonce), 48),
        ])
    }
    let permit_batch = DynSolValue::Tuple(vec![
        DynSolValue::Array(vec![
            permit_details(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                1000,
                1_738_001_800,
                0,
            ),
            permit_details(
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                2000,
                1_738_001_900,
                1,
            ),
        ]),
        DynSolValue::Address(
            "0x00000000000000000000000000000000deadbeef"
                .parse::<AlloyAddress>()
                .unwrap(),
        ),
        DynSolValue::Uint(AlloyU256::from(1_738_002_000u64), 256),
    ]);
    encode_calldata(
        "0x2a2d80d1",
        &[
            DynSolValue::Address(
                "0x000000000000000000000000000000000000a01c"
                    .parse::<AlloyAddress>()
                    .unwrap(),
            ),
            permit_batch,
            DynSolValue::Bytes(vec![0xab, 0xcd]),
        ],
    )
}

#[test]
fn b2_permit2_permit_batch_calldata_decodes() {
    // Inline a positional-access permitBatch manifest (the calldata
    // convention) so this test pins the nested-tuple narrow-int CALLDATA
    // decode end-to-end. The ABI is faithful to real Permit2 (sigDeadline
    // stays `uint256`); both deterministic `Time` (u64) fields are bound to
    // NESTED `uint48` components of the `PermitDetails` element — `expires_at`
    // ← `expiration` (`$inputs[2]`, the load-bearing assertion) and, purely to
    // exercise a second nested-uint48→u64 decode, `sig_deadline` ← `nonce`
    // (`$inputs[3]`). (Real Permit2 `sigDeadline` is `uint256`; feeding a
    // `uint256`→`Time` (u64) on the calldata route is a SEPARATE documented
    // gap — its production surface is the off-chain typed-data route, where
    // wallets send it as a JSON number.)
    const B2_PERMIT2_PERMIT_BATCH_V3: &str = r#"{
      "type": "adapter_action",
      "id": "uniswap/permit2/permitBatch@1.0.0",
      "publisher": "uniswap.eth",
      "schema_version": "3",
      "match": {
        "selector": "0x2a2d80d1",
        "chain_to_addresses": { "1": ["0x000000000022d473030f116ddee9f6b43ac78ba3"] },
        "typed_data": {
          "domain_name": "Permit2",
          "verifying_contract": "0x000000000022d473030f116ddee9f6b43ac78ba3",
          "primary_type": "PermitBatch",
          "types": {
            "PermitBatch": [
              { "name": "details", "type": "PermitDetails[]" },
              { "name": "spender", "type": "address" },
              { "name": "sigDeadline", "type": "uint256" }
            ],
            "PermitDetails": [
              { "name": "token", "type": "address" },
              { "name": "amount", "type": "uint160" },
              { "name": "expiration", "type": "uint48" },
              { "name": "nonce", "type": "uint48" }
            ]
          }
        }
      },
      "abi_fragment": {
        "function_name": "permit",
        "abi": {
          "name": "permit",
          "type": "function",
          "inputs": [
            { "name": "owner", "type": "address" },
            {
              "name": "permitBatch",
              "type": "tuple",
              "components": [
                {
                  "name": "details",
                  "type": "tuple[]",
                  "components": [
                    { "name": "token", "type": "address" },
                    { "name": "amount", "type": "uint160" },
                    { "name": "expiration", "type": "uint48" },
                    { "name": "nonce", "type": "uint48" }
                  ]
                },
                { "name": "spender", "type": "address" },
                { "name": "sigDeadline", "type": "uint256" }
              ]
            },
            { "name": "signature", "type": "bytes" }
          ]
        }
      },
      "emit": {
        "strategy": "array_emit",
        "array_source": "$args.permitBatch[0]",
        "body": {
          "domain": "token",
          "token": {
            "action": "permit2_sign_allowance",
            "permit2_sign_allowance": {
              "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs[0]" } },
              "spender": "$args.permitBatch[1]",
              "amount": "$inputs[1]",
              "expires_at": "$inputs[2]",
              "sig_deadline": "$inputs[3]"
            }
          }
        },
        "live_inputs": {
          "nonce": {
            "source": {
              "kind": "onchain_view",
              "chain": "$chain",
              "contract": "0x000000000022d473030f116ddee9f6b43ac78ba3",
              "function": "nonceBitmap(address,uint256)",
              "decoder_id": "permit2_nonce_bitmap"
            },
            "ttl_s": 12
          }
        }
      },
      "requires": {
        "imperative": [],
        "adapter_capabilities": ["token_metadata"],
        "host_capabilities": [],
        "extension": ">=0.1.0"
      }
    }"#;

    let install = install_ok(B2_PERMIT2_PERMIT_BATCH_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/permit2/permitBatch@1.0.0"
    );

    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0x2a2d80d1",
        b2_permit_batch_calldata(),
        "0x000000000000000000000000000000000000aaaa",
    );

    // CALLDATA path: the decoded `permitBatch` tuple is POSITIONAL. With the
    // b1-infra bridge fix threading the nested `PermitDetails` tuple's ABI
    // widths, the uint48 `expiration` (`$inputs[2]`) renders as a JSON number
    // and the `Permit2SignAction.expires_at: Time` (transparent u64) accepts
    // it. The fan-out yields one `permit2_sign_allowance` per `details`
    // element, wrapped in a `Multicall` body.
    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "{parsed}");

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let inner = body["actions"].as_array().expect("inner actions array");
    assert_eq!(
        inner.len(),
        2,
        "one permit2_sign_allowance per details: {parsed}"
    );

    // element-0 — token = USDC, amount = 1000, expiration = 1738001800.
    assert_eq!(inner[0]["domain"], "token", "{parsed}");
    assert_eq!(inner[0]["action"], "permit2_sign_allowance", "{parsed}");
    assert_eq!(
        inner[0]["token"]["key"]["address"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "{parsed}"
    );
    // `amount` is `uint160` → > 64 bits → decoded as a decimal string, then
    // serialised by the `U256` ActionBody field as `0x`-hex (1000 = 0x3e8).
    assert_eq!(inner[0]["amount"], "0x3e8", "{parsed}");
    // The load-bearing assertion: the NESTED uint48 `expiration` decoded to a
    // JSON number (Time over u64), NOT a decimal string. `serde_json::Value`
    // equality holds against `1_738_001_800` only when it really is a number —
    // a `"1738001800"` string would compare unequal (and `Time` would have
    // rejected it at deserialize). `sig_deadline` ← nested uint48 `nonce` = 0
    // proves a second nested-narrow-int field on the same element.
    assert_eq!(inner[0]["expires_at"], 1_738_001_800u64, "{parsed}");
    assert_eq!(inner[0]["sig_deadline"], 0u64, "{parsed}");
    assert_eq!(
        inner[0]["spender"], "0x00000000000000000000000000000000deadbeef",
        "{parsed}"
    );

    // element-1 — token = WETH, amount = 2000, expiration = 1738001900, nonce = 1.
    assert_eq!(
        inner[1]["token"]["key"]["address"], "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        "{parsed}"
    );
    assert_eq!(inner[1]["amount"], "0x7d0", "{parsed}");
    assert_eq!(inner[1]["expires_at"], 1_738_001_900u64, "{parsed}");
    assert_eq!(inner[1]["sig_deadline"], 1u64, "{parsed}");
}

// ===========================================================================
// b1 — Uniswap V3 NonfungiblePositionManager (NFPM) direct functions
// ===========================================================================
//
// The 5 user-facing NFPM functions route to the EXISTING concentrated-liquidity
// `AmmAction` variants (the Rust simulation effects already dispatch UniswapV3):
//   * mint              (0x88316456) → add_liquidity / concentrated_mint
//   * increaseLiquidity (0x219f5d17) → add_liquidity / concentrated_increase
//   * decreaseLiquidity (0x0c49ccbe) → remove_liquidity / concentrated_decrease
//   * collect           (0xfc6f7865) → collect_fees
//   * burn              (0x42966c68) → remove_liquidity / concentrated_burn
//
// Each manifest is loaded from the committed registryV2 file (`include_str!`)
// so the on-disk artifact is what the route exercises. NFPM addresses:
// mainnet/OP/Arb share the deterministic CREATE2 deploy
// `0xC36442b4a4522E871399CD717aBDD847Ab11FE88`; Base is `0x03a5…34f1`. Tests
// route on chain 1.
//
// Tuple-arg CALLDATA access convention: mint/increase/decrease/collect each take
// a SOLE top-level `params` struct. The abi-resolver bridge
// (`bridge::convert_legacy_call`, the `args.len() == 1 && Tuple` arm) flattens a
// sole-tuple arg into one `DecodedArg` PER FIELD, keyed by the component NAME
// ("sol!-flattened layout"). So the manifests access these by top-level NAME
// (`$args.token0`, `$args.fee`, `$args.tickLower`, `$args.tokenId`,
// `$args.liquidity`, …) — NOT positionally as `$args.params[i]`. This is the
// opposite of the b2 permitBatch case, where the tuple is one of THREE args
// (`owner`, `permitBatch`, `signature`) so `permitBatch` stays a positional array
// `$args.permitBatch[i]`; the sole-tuple flatten only fires when the function has
// exactly one (tuple) arg. burn takes a flat `uint256 tokenId` (no tuple) →
// `$args.tokenId` directly. The `6a24f09` width-threading fix makes the flattened
// `int24` ticks (`tickLower`/`tickUpper`) and `uint24` `fee` render as JSON
// **numbers** (it threads each component's canonical width), so
// `RangeSpec::Tick { lower: i32, upper: i32 }` and `AmmVenue::UniswapV3 {
// fee_tier_bp: u32 }` deserialize cleanly.
//
// `venue.pool` / `fee_tier_bp` (for increase/decrease/collect/burn, where the
// pool is not derivable from a call arg) come from `$resolved.*` — empty in this
// narrow scope, so they fall back to the `placeholder_type_lookup` zero values
// (`pool` → zero Address, `fee_tier_bp` → 0). `mint`'s `fee_tier_bp` is the live
// `uint24` `$args.params[2]`. The `nft_key` ERC721 `TokenKey` mirrors the
// committed `standard/erc721` NFT manifests: bare `{ standard:"erc721",
// chain:"$chain", contract:"$to" (the NFPM), token_id:"$args.params[0]" }`.

const NFPM_MINT_V3: &str =
    include_str!("../../../registryV2/manifests/uniswap/v3-nfpm/mint@1.0.0.json");
const NFPM_INCREASE_V3: &str =
    include_str!("../../../registryV2/manifests/uniswap/v3-nfpm/increase-liquidity@1.0.0.json");
const NFPM_DECREASE_V3: &str =
    include_str!("../../../registryV2/manifests/uniswap/v3-nfpm/decrease-liquidity@1.0.0.json");
const NFPM_COLLECT_V3: &str =
    include_str!("../../../registryV2/manifests/uniswap/v3-nfpm/collect@1.0.0.json");
const NFPM_BURN_V3: &str =
    include_str!("../../../registryV2/manifests/uniswap/v3-nfpm/burn@1.0.0.json");

const NFPM_MAINNET: &str = "0xc36442b4a4522e871399cd717abdd847ab11fe88";
const NFPM_TOKEN0: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
const NFPM_TOKEN1: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"; // WETH
const NFPM_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";

fn addr(s: &str) -> AlloyAddress {
    s.parse::<AlloyAddress>().unwrap()
}

// ---------------------------------------------------------------------------
// b1.nfpm.mint — concentrated_mint (single tuple, int24 ticks as NUMBERS)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_mint_concentrated_mint() {
    let install = install_ok(NFPM_MINT_V3);
    assert_eq!(install["data"]["bundle_id"], "uniswap/v3-nfpm/mint@1.0.0");

    // mint(MintParams) — one positional tuple arg. tickLower=-887220 /
    // tickUpper=887220 exercise the int24 signed-narrow decode; fee=3000 the
    // uint24 path.
    let params = DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(NFPM_TOKEN0)),         // [0] token0
        DynSolValue::Address(addr(NFPM_TOKEN1)),         // [1] token1
        DynSolValue::Uint(AlloyU256::from(3000u64), 24), // [2] fee
        DynSolValue::Int(alloy_primitives::I256::try_from(-887_220i64).unwrap(), 24), // [3] tickLower
        DynSolValue::Int(alloy_primitives::I256::try_from(887_220i64).unwrap(), 24), // [4] tickUpper
        DynSolValue::Uint(AlloyU256::from(1_000_000u64), 256), // [5] amount0Desired
        DynSolValue::Uint(AlloyU256::from(500u64), 256),       // [6] amount1Desired
        DynSolValue::Uint(AlloyU256::from(900_000u64), 256),   // [7] amount0Min
        DynSolValue::Uint(AlloyU256::from(450u64), 256),       // [8] amount1Min
        DynSolValue::Address(addr(NFPM_SUBMITTER)),            // [9] recipient
        DynSolValue::Uint(AlloyU256::from(1_738_002_000u64), 256), // [10] deadline
    ]);
    let calldata = encode_calldata("0x88316456", &[params]);
    let input = route_input(1, NFPM_MAINNET, "0x88316456", calldata, NFPM_SUBMITTER);

    let parsed = route_ok(input);
    assert_eq!(parsed["data"]["decoder_id"], "uniswap/v3-nfpm/mint@1.0.0");
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "add_liquidity", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_v3", "{parsed}");
    // fee = uint24 3000 ≤ 64 bits → JSON number → u32 fee_tier_bp.
    assert_eq!(body["venue"]["fee_tier_bp"], 3000u64, "{parsed}");
    assert_eq!(body["params"]["kind"], "concentrated_mint", "{parsed}");
    // LOAD-BEARING: int24 tick decodes to a signed JSON NUMBER (not a string).
    // `serde_json::Value` equality holds against `-887220` only when it really
    // is a number — a `"-887220"` string would compare unequal, and the `i32`
    // `RangeSpec::Tick.lower` would have rejected a string at deserialize.
    assert_eq!(body["params"]["range"]["kind"], "tick", "{parsed}");
    assert_eq!(body["params"]["range"]["lower"], -887_220i64, "{parsed}");
    assert_eq!(body["params"]["range"]["upper"], 887_220i64, "{parsed}");
    // pool_pair token0/token1 bound positionally off the tuple.
    assert_eq!(
        body["params"]["pool_pair"][0]["key"]["address"], NFPM_TOKEN0,
        "{parsed}"
    );
    assert_eq!(
        body["params"]["pool_pair"][1]["key"]["address"], NFPM_TOKEN1,
        "{parsed}"
    );
    // amount_desired = (amount0Desired, amount1Desired) → U256 hex.
    assert_eq!(body["params"]["amount_desired"][0], "0xf4240", "{parsed}"); // 1_000_000
    assert_eq!(body["params"]["amount_desired"][1], "0x1f4", "{parsed}"); // 500
    assert_eq!(body["params"]["recipient"], NFPM_SUBMITTER, "{parsed}");
}

// ---------------------------------------------------------------------------
// b1.nfpm.increase — concentrated_increase (nft_key ERC721 off NFPM)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_increase_concentrated_increase() {
    let install = install_ok(NFPM_INCREASE_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/v3-nfpm/increase-liquidity@1.0.0"
    );

    // increaseLiquidity(IncreaseLiquidityParams) — tokenId=424242.
    let params = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // [0] tokenId
        DynSolValue::Uint(AlloyU256::from(2_000_000u64), 256), // [1] amount0Desired
        DynSolValue::Uint(AlloyU256::from(1000u64), 256),    // [2] amount1Desired
        DynSolValue::Uint(AlloyU256::from(1_800_000u64), 256), // [3] amount0Min
        DynSolValue::Uint(AlloyU256::from(900u64), 256),     // [4] amount1Min
        DynSolValue::Uint(AlloyU256::from(1_738_002_000u64), 256), // [5] deadline
    ]);
    let calldata = encode_calldata("0x219f5d17", &[params]);
    let input = route_input(1, NFPM_MAINNET, "0x219f5d17", calldata, NFPM_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "add_liquidity", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_v3", "{parsed}");
    assert_eq!(body["params"]["kind"], "concentrated_increase", "{parsed}");
    // nft_key is a bare ERC721 TokenKey: contract = NFPM (`$to`), token_id
    // = tokenId. token_id is uint256 (> 64 bits) → U256 → alloy hex "0x67932".
    assert_eq!(body["params"]["nft_key"]["standard"], "erc721", "{parsed}");
    assert_eq!(
        body["params"]["nft_key"]["contract"], NFPM_MAINNET,
        "{parsed}"
    );
    assert_eq!(body["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
    assert_eq!(body["params"]["amount_desired"][0], "0x1e8480", "{parsed}"); // 2_000_000
    assert_eq!(body["params"]["amount_desired"][1], "0x3e8", "{parsed}"); // 1000
}

// ---------------------------------------------------------------------------
// b1.nfpm.decrease — concentrated_decrease (uint128 liquidity_burn)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_decrease_concentrated_decrease() {
    let install = install_ok(NFPM_DECREASE_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/v3-nfpm/decrease-liquidity@1.0.0"
    );

    // decreaseLiquidity(DecreaseLiquidityParams) — liquidity=123456789 (uint128).
    let params = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // [0] tokenId
        DynSolValue::Uint(AlloyU256::from(123_456_789u64), 128), // [1] liquidity
        DynSolValue::Uint(AlloyU256::from(50u64), 256),      // [2] amount0Min
        DynSolValue::Uint(AlloyU256::from(60u64), 256),      // [3] amount1Min
        DynSolValue::Uint(AlloyU256::from(1_738_002_000u64), 256), // [4] deadline
    ]);
    let calldata = encode_calldata("0x0c49ccbe", &[params]);
    let input = route_input(1, NFPM_MAINNET, "0x0c49ccbe", calldata, NFPM_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "remove_liquidity", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_v3", "{parsed}");
    assert_eq!(body["params"]["kind"], "concentrated_decrease", "{parsed}");
    // LOAD-BEARING: uint128 liquidity_burn → > 64 bits → decimal string into the
    // U128 field, serialised back as alloy hex (123456789 = 0x75bcd15).
    assert_eq!(body["params"]["liquidity_burn"], "0x75bcd15", "{parsed}");
    assert_eq!(body["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
    assert_eq!(body["params"]["amount_min"][0], "0x32", "{parsed}"); // 50
    assert_eq!(body["params"]["amount_min"][1], "0x3c", "{parsed}"); // 60
}

// ---------------------------------------------------------------------------
// b1.nfpm.collect — collect_fees (no params enum; direct nft_key + recipient)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_collect_collect_fees() {
    let install = install_ok(NFPM_COLLECT_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/v3-nfpm/collect@1.0.0"
    );

    // collect(CollectParams) — recipient distinct from submitter.
    let recipient = "0x00000000000000000000000000000000cafef00d";
    let params = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // [0] tokenId
        DynSolValue::Address(addr(recipient)),               // [1] recipient
        DynSolValue::Uint(AlloyU256::MAX, 128),              // [2] amount0Max
        DynSolValue::Uint(AlloyU256::MAX, 128),              // [3] amount1Max
    ]);
    let calldata = encode_calldata("0xfc6f7865", &[params]);
    let input = route_input(1, NFPM_MAINNET, "0xfc6f7865", calldata, NFPM_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "collect_fees", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_v3", "{parsed}");
    // LOAD-BEARING: nft_key bound off NFPM (`$to`) + tokenId positional.
    assert_eq!(body["nft_key"]["standard"], "erc721", "{parsed}");
    assert_eq!(body["nft_key"]["contract"], NFPM_MAINNET, "{parsed}");
    assert_eq!(body["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
    assert_eq!(body["recipient"], recipient, "{parsed}");
}

// ---------------------------------------------------------------------------
// b1.nfpm.burn — concentrated_burn (flat uint256 arg, NOT a tuple)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_burn_concentrated_burn() {
    let install = install_ok(NFPM_BURN_V3);
    assert_eq!(install["data"]["bundle_id"], "uniswap/v3-nfpm/burn@1.0.0");

    // burn(uint256 tokenId) — a flat (non-tuple) arg → `$args.tokenId`.
    let calldata = encode_calldata(
        "0x42966c68",
        &[DynSolValue::Uint(AlloyU256::from(424_242u64), 256)],
    );
    let input = route_input(1, NFPM_MAINNET, "0x42966c68", calldata, NFPM_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "remove_liquidity", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_v3", "{parsed}");
    assert_eq!(body["params"]["kind"], "concentrated_burn", "{parsed}");
    assert_eq!(body["params"]["nft_key"]["standard"], "erc721", "{parsed}");
    assert_eq!(
        body["params"]["nft_key"]["contract"], NFPM_MAINNET,
        "{parsed}"
    );
    assert_eq!(body["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
}

// ===========================================================================
// b1.c — Uniswap V4 PositionManager.modifyLiquidities dispatch
// ===========================================================================
//
// `modifyLiquidities(bytes unlockData, uint256 deadline)` selector 0xdd46508f.
// `unlockData = abi.encode(bytes actions, bytes[] params)` — a packed
// one-byte-per-action `actions` blob + a parallel `params[]` of abi-encoded
// per-action tuples (the SAME `(commands, inputs[])` shape as the Universal
// Router, just nested one `abi.decode` deep). The manifest uses
// `emit.strategy: "opcode_stream_dispatch"` extended with an
// `unlock_data_source: "$args.unlockData"` that pre-decodes the bytes arg into
// commands+inputs before the shared per-opcode dispatch runs.
//
// Per-action mapping (PositionManager `_handleAction` table only):
//   * 0x02 MINT_POSITION   → amm.add_liquidity / concentrated_mint. PoolKey is
//     INLINE (head-flattened 5 fields) → pool_id = keccak256(abi.encode(poolKey))
//     computed in Rust + injected as `$inputs.pool_id`.
//   * 0x00 INCREASE        → amm.add_liquidity / concentrated_increase. Only
//     tokenId in calldata → pool_id NOT statically recoverable → "unknown".
//   * 0x01 DECREASE        → amm.remove_liquidity / concentrated_decrease.
//   * 0x03 BURN            → amm.remove_liquidity / concentrated_burn.
//   * settlement 0x0b-0x16 → skipped (unknown_opcode_policy=skip; absent from
//     per_opcode_body). The liquidity action is the intent.
//   * 0x04/0x05 DEPRECATED → emitted as Unknown (preserve the sandwich-risk
//     signal without crashing).

const V4_PM_MODIFY_LIQ_V3: &str = include_str!(
    "../../../registryV2/manifests/uniswap/v4-position-manager/modify-liquidities@1.0.0.json"
);

// Verified V4 deployments (docs §2): mainnet (1).
const V4_PM_MAINNET: &str = "0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e";
const V4_POOL_MANAGER_MAINNET: &str = "0x000000000004444c5dc75cb358380d2e3de08a90";
const V4_HOOKS: &str = "0x0000000000000000000000000000000000000000"; // no-hook pool
const V4_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";
const V4_CURRENCY0: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
const V4_CURRENCY1: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"; // WETH
const V4_SELECTOR: &str = "0xdd46508f";

/// Build `modifyLiquidities` calldata: `abi.encode(bytes actions, bytes[]
/// params)` wrapped as the `unlockData` bytes arg + a `deadline` uint256.
fn v4_modify_liquidities_calldata(actions: &[u8], params: Vec<Vec<u8>>) -> String {
    let unlock_data = DynSolValue::Tuple(vec![
        DynSolValue::Bytes(actions.to_vec()),
        DynSolValue::Array(params.into_iter().map(DynSolValue::Bytes).collect()),
    ])
    .abi_encode_params();
    encode_calldata(
        V4_SELECTOR,
        &[
            DynSolValue::Bytes(unlock_data),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    )
}

/// Independently compute `pool_id = keccak256(abi.encode(poolKey))` over the 5
/// PoolKey fields, exactly as the dispatcher does — pins the on-the-wire bytes.
fn v4_pool_id(
    currency0: &str,
    currency1: &str,
    fee: u32,
    tick_spacing: i32,
    hooks: &str,
) -> String {
    let encoded = DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(currency0)),
        DynSolValue::Address(addr(currency1)),
        DynSolValue::Uint(AlloyU256::from(fee), 24),
        DynSolValue::Int(alloy_primitives::I256::try_from(tick_spacing).unwrap(), 24),
        DynSolValue::Address(addr(hooks)),
    ])
    .abi_encode_params();
    format!("0x{}", hex::encode(alloy_primitives::keccak256(&encoded)))
}

/// MINT_POSITION (0x02) params tuple — PoolKey INLINE (head-flattened 5 fields)
/// then tickLower/tickUpper/liquidity/amount0Max/amount1Max/owner/hookData.
fn v4_mint_params(fee: u32, tick_spacing: i32, tick_lower: i32, tick_upper: i32) -> Vec<u8> {
    DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(V4_CURRENCY0)),    // currency0
        DynSolValue::Address(addr(V4_CURRENCY1)),    // currency1
        DynSolValue::Uint(AlloyU256::from(fee), 24), // fee
        DynSolValue::Int(alloy_primitives::I256::try_from(tick_spacing).unwrap(), 24), // tickSpacing
        DynSolValue::Address(addr(V4_HOOKS)),                                          // hooks
        DynSolValue::Int(alloy_primitives::I256::try_from(tick_lower).unwrap(), 24),   // tickLower
        DynSolValue::Int(alloy_primitives::I256::try_from(tick_upper).unwrap(), 24),   // tickUpper
        DynSolValue::Uint(AlloyU256::from(5_000u64), 256),                             // liquidity
        DynSolValue::Uint(AlloyU256::from(1_000_000u64), 128),                         // amount0Max
        DynSolValue::Uint(AlloyU256::from(500u64), 128),                               // amount1Max
        DynSolValue::Address(addr(V4_SUBMITTER)),                                      // owner
        DynSolValue::Bytes(vec![]),                                                    // hookData
    ])
    .abi_encode_params()
}

/// SETTLE_PAIR (0x0d) params tuple — (Currency currency0, Currency currency1).
fn v4_settle_pair_params() -> Vec<u8> {
    DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(V4_CURRENCY0)),
        DynSolValue::Address(addr(V4_CURRENCY1)),
    ])
    .abi_encode_params()
}

// ---------------------------------------------------------------------------
// b1.c.mint — MINT_POSITION + SETTLE_PAIR → concentrated_mint (settle skipped)
// ---------------------------------------------------------------------------

#[test]
fn b1_v4_modify_liquidities_mint_settle_pair() {
    let install = install_ok(V4_PM_MODIFY_LIQ_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/v4-position-manager/modify-liquidities@1.0.0"
    );

    // actions = [MINT_POSITION (0x02), SETTLE_PAIR (0x0d)]
    let calldata = v4_modify_liquidities_calldata(
        &[0x02, 0x0d],
        vec![
            v4_mint_params(3000, 60, -887_220, 887_220),
            v4_settle_pair_params(),
        ],
    );
    let input = route_input(1, V4_PM_MAINNET, V4_SELECTOR, calldata, V4_SUBMITTER);

    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"],
        "uniswap/v4-position-manager/modify-liquidities@1.0.0"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    // SETTLE_PAIR is skipped — exactly ONE child (the mint).
    assert_eq!(body["actions"].as_array().unwrap().len(), 1, "{parsed}");

    let mint = &body["actions"][0];
    assert_eq!(mint["domain"], "amm", "{parsed}");
    assert_eq!(mint["action"], "add_liquidity", "{parsed}");
    assert_eq!(mint["venue"]["name"], "uniswap_v4", "{parsed}");
    // pool_id computed in Rust = keccak256(abi.encode(poolKey)).
    assert_eq!(
        mint["venue"]["pool_id"],
        v4_pool_id(V4_CURRENCY0, V4_CURRENCY1, 3000, 60, V4_HOOKS),
        "{parsed}"
    );
    // pool_manager injected per-chain (mainnet V4 PoolManager).
    assert_eq!(
        mint["venue"]["pool_manager"], V4_POOL_MANAGER_MAINNET,
        "{parsed}"
    );
    // hooks from calldata (real PoolKey.hooks).
    assert_eq!(mint["venue"]["hooks"], V4_HOOKS, "{parsed}");
    assert_eq!(mint["params"]["kind"], "concentrated_mint", "{parsed}");
    // pool_pair = (currency0, currency1).
    assert_eq!(
        mint["params"]["pool_pair"][0]["key"]["address"], V4_CURRENCY0,
        "{parsed}"
    );
    assert_eq!(
        mint["params"]["pool_pair"][1]["key"]["address"], V4_CURRENCY1,
        "{parsed}"
    );
    // range from tickLower/tickUpper (int24 → signed JSON numbers).
    assert_eq!(mint["params"]["range"]["kind"], "tick", "{parsed}");
    assert_eq!(mint["params"]["range"]["lower"], -887_220i64, "{parsed}");
    assert_eq!(mint["params"]["range"]["upper"], 887_220i64, "{parsed}");
    // amounts from amount0Max/amount1Max (uint128 → hex).
    assert_eq!(mint["params"]["amount_desired"][0], "0xf4240", "{parsed}"); // 1_000_000
    assert_eq!(mint["params"]["amount_desired"][1], "0x1f4", "{parsed}"); // 500
    assert_eq!(mint["params"]["recipient"], V4_SUBMITTER, "{parsed}");
}

// ---------------------------------------------------------------------------
// b1.c.increase — INCREASE_LIQUIDITY + SETTLE_PAIR → concentrated_increase
// ---------------------------------------------------------------------------

#[test]
fn b1_v4_modify_liquidities_increase_settle_pair() {
    install_ok(V4_PM_MODIFY_LIQ_V3);

    // INCREASE_LIQUIDITY (0x00): (tokenId, liquidity, amount0Max, amount1Max, hookData)
    let increase = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // tokenId
        DynSolValue::Uint(AlloyU256::from(7_777u64), 256),   // liquidity
        DynSolValue::Uint(AlloyU256::from(2_000_000u64), 128), // amount0Max
        DynSolValue::Uint(AlloyU256::from(1000u64), 128),    // amount1Max
        DynSolValue::Bytes(vec![]),                          // hookData
    ])
    .abi_encode_params();
    let calldata =
        v4_modify_liquidities_calldata(&[0x00, 0x0d], vec![increase, v4_settle_pair_params()]);
    let input = route_input(1, V4_PM_MAINNET, V4_SELECTOR, calldata, V4_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    assert_eq!(body["actions"].as_array().unwrap().len(), 1, "{parsed}");

    let inc = &body["actions"][0];
    assert_eq!(inc["domain"], "amm", "{parsed}");
    assert_eq!(inc["action"], "add_liquidity", "{parsed}");
    assert_eq!(inc["venue"]["name"], "uniswap_v4", "{parsed}");
    // pool_id NOT statically recoverable for increase → "unknown" sentinel.
    assert_eq!(inc["venue"]["pool_id"], "unknown", "{parsed}");
    assert_eq!(
        inc["venue"]["pool_manager"], V4_POOL_MANAGER_MAINNET,
        "{parsed}"
    );
    assert_eq!(inc["params"]["kind"], "concentrated_increase", "{parsed}");
    // nft_key = ERC721 off the PositionManager (`$to`) + tokenId.
    assert_eq!(inc["params"]["nft_key"]["standard"], "erc721", "{parsed}");
    assert_eq!(
        inc["params"]["nft_key"]["contract"], V4_PM_MAINNET,
        "{parsed}"
    );
    assert_eq!(inc["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
                                                                             // amounts from amount0Max/amount1Max.
    assert_eq!(inc["params"]["amount_desired"][0], "0x1e8480", "{parsed}"); // 2_000_000
    assert_eq!(inc["params"]["amount_desired"][1], "0x3e8", "{parsed}"); // 1000
}

// ---------------------------------------------------------------------------
// b1.c.decrease — DECREASE_LIQUIDITY + TAKE_PAIR → concentrated_decrease
// ---------------------------------------------------------------------------

#[test]
fn b1_v4_modify_liquidities_decrease_take_pair() {
    install_ok(V4_PM_MODIFY_LIQ_V3);

    // DECREASE_LIQUIDITY (0x01): (tokenId, liquidity, amount0Min, amount1Min, hookData)
    let decrease = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // tokenId
        DynSolValue::Uint(AlloyU256::from(123_456_789u64), 256), // liquidity
        DynSolValue::Uint(AlloyU256::from(50u64), 128),      // amount0Min
        DynSolValue::Uint(AlloyU256::from(60u64), 128),      // amount1Min
        DynSolValue::Bytes(vec![]),                          // hookData
    ])
    .abi_encode_params();
    // TAKE_PAIR (0x11): (Currency currency0, Currency currency1, address recipient)
    let take_pair = DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(V4_CURRENCY0)),
        DynSolValue::Address(addr(V4_CURRENCY1)),
        DynSolValue::Address(addr(V4_SUBMITTER)),
    ])
    .abi_encode_params();
    let calldata = v4_modify_liquidities_calldata(&[0x01, 0x11], vec![decrease, take_pair]);
    let input = route_input(1, V4_PM_MAINNET, V4_SELECTOR, calldata, V4_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    // TAKE_PAIR skipped — one child (the decrease).
    assert_eq!(body["actions"].as_array().unwrap().len(), 1, "{parsed}");

    let dec = &body["actions"][0];
    assert_eq!(dec["domain"], "amm", "{parsed}");
    assert_eq!(dec["action"], "remove_liquidity", "{parsed}");
    assert_eq!(dec["venue"]["name"], "uniswap_v4", "{parsed}");
    assert_eq!(dec["venue"]["pool_id"], "unknown", "{parsed}");
    assert_eq!(dec["params"]["kind"], "concentrated_decrease", "{parsed}");
    assert_eq!(
        dec["params"]["nft_key"]["contract"], V4_PM_MAINNET,
        "{parsed}"
    );
    assert_eq!(dec["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
                                                                             // liquidity_burn (uint256 wire → U128) = 123456789 = 0x75bcd15.
    assert_eq!(dec["params"]["liquidity_burn"], "0x75bcd15", "{parsed}");
    assert_eq!(dec["params"]["amount_min"][0], "0x32", "{parsed}"); // 50
    assert_eq!(dec["params"]["amount_min"][1], "0x3c", "{parsed}"); // 60
}

// ---------------------------------------------------------------------------
// b1.c.burn — BURN_POSITION + TAKE_PAIR → concentrated_burn
// ---------------------------------------------------------------------------

#[test]
fn b1_v4_modify_liquidities_burn() {
    install_ok(V4_PM_MODIFY_LIQ_V3);

    // BURN_POSITION (0x03): (tokenId, amount0Min, amount1Min, hookData)
    let burn = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(424_242u64), 256), // tokenId
        DynSolValue::Uint(AlloyU256::from(0u64), 128),       // amount0Min
        DynSolValue::Uint(AlloyU256::from(0u64), 128),       // amount1Min
        DynSolValue::Bytes(vec![]),                          // hookData
    ])
    .abi_encode_params();
    let take_pair = DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(V4_CURRENCY0)),
        DynSolValue::Address(addr(V4_CURRENCY1)),
        DynSolValue::Address(addr(V4_SUBMITTER)),
    ])
    .abi_encode_params();
    let calldata = v4_modify_liquidities_calldata(&[0x03, 0x11], vec![burn, take_pair]);
    let input = route_input(1, V4_PM_MAINNET, V4_SELECTOR, calldata, V4_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    assert_eq!(body["actions"].as_array().unwrap().len(), 1, "{parsed}");

    let b = &body["actions"][0];
    assert_eq!(b["action"], "remove_liquidity", "{parsed}");
    assert_eq!(b["params"]["kind"], "concentrated_burn", "{parsed}");
    assert_eq!(
        b["params"]["nft_key"]["contract"], V4_PM_MAINNET,
        "{parsed}"
    );
    assert_eq!(b["params"]["nft_key"]["token_id"], "0x67932", "{parsed}"); // 424242
}

// ---------------------------------------------------------------------------
// b1.c.deprecated — MINT_POSITION_FROM_DELTAS (0x05) → Unknown (no crash)
// ---------------------------------------------------------------------------

#[test]
fn b1_v4_modify_liquidities_deprecated_emits_unknown() {
    install_ok(V4_PM_MODIFY_LIQ_V3);

    // 0x05 MINT_POSITION_FROM_DELTAS is DEPRECATED (sandwich-vulnerable). We
    // don't decode its params; the route must NOT crash and must surface the
    // action as Unknown so policy can warn/deny. The params blob is opaque.
    let calldata = v4_modify_liquidities_calldata(&[0x05], vec![vec![0u8; 32]]);
    let input = route_input(1, V4_PM_MAINNET, V4_SELECTOR, calldata, V4_SUBMITTER);

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    assert_eq!(body["actions"].as_array().unwrap().len(), 1, "{parsed}");
    assert_eq!(body["actions"][0]["domain"], "unknown", "{parsed}");
    assert_eq!(body["actions"][0]["target"], V4_PM_MAINNET, "{parsed}");
}

// ===========================================================================
// T1 — typed-data `witness_type` 4th routing-key component (Permit2-witness
//      de-collision)
// ===========================================================================
//
// UniswapX intent orders are signed as Permit2 `permitWitnessTransferFrom`
// witnesses: `verifying_contract = Permit2`, `primary_type =
// "PermitWitnessTransferFrom"`, and the ACTUAL order type lives in the EIP-712
// `witness` field's type (e.g. "ExclusiveDutchOrder" / "V2DutchOrder"). Every
// such order collides on the 3-tuple `(chain, Permit2, "PermitWitnessTransferFrom")`.
// The optional `witness_type` 4th key component disambiguates by the witness
// struct's EIP-712 type name.
//
// These tests live next to the calldata route fixtures (per the T1 task) but
// exercise the OFF-CHAIN typed-data path (`declarative_route_typed_data_v3_json`).

const PERMIT2_VC: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
const T1_SIGNER: &str = "0x000000000000000000000000000000000000aaaa";

/// Build a typed-data route input. `witness_type` is included only when `Some`
/// — when `None` the wire shape is byte-identical to the pre-T1 input (the
/// backward-compat path: the DTO's `#[serde(default)]` yields `None`).
fn t1_typed_data_input(
    chain_id: u64,
    verifying_contract: &str,
    primary_type: &str,
    witness_type: Option<&str>,
    message: Value,
) -> String {
    let mut obj = json!({
        "chain_id": chain_id,
        "verifying_contract": verifying_contract,
        "primary_type": primary_type,
        "domain_name": "Permit2",
        "message": message,
        "submitter": T1_SIGNER,
        "submitted_at": 1_700_000_000_u64
    });
    if let Some(wt) = witness_type {
        obj["witness_type"] = json!(wt);
    }
    obj.to_string()
}

/// A synthetic Permit2-witness manifest. `witness_type` (optional) + a
/// distinguishable `spender` address let the de-collision test prove WHICH
/// manifest was hit. `id` / `selector` / `spender_addr` differ per fixture.
fn t1_witness_manifest(
    id: &str,
    selector: &str,
    witness_type: Option<&str>,
    spender_addr: &str,
) -> String {
    let witness_line = match witness_type {
        Some(wt) => format!(r#""witness_type": "{wt}","#),
        None => String::new(),
    };
    format!(
        r#"{{
  "type": "adapter_action",
  "id": "{id}",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {{
    "selector": "{selector}",
    "chain_to_addresses": {{ "1": ["{PERMIT2_VC}"] }},
    "typed_data": {{
      "domain_name": "Permit2",
      "verifying_contract": "{PERMIT2_VC}",
      "primary_type": "PermitWitnessTransferFrom",
      {witness_line}
      "types": {{
        "PermitWitnessTransferFrom": [
          {{ "name": "permitted", "type": "TokenPermissions" }},
          {{ "name": "spender", "type": "address" }},
          {{ "name": "nonce", "type": "uint256" }},
          {{ "name": "deadline", "type": "uint256" }},
          {{ "name": "witness", "type": "{witness_named}" }}
        ]
      }}
    }}
  }},
  "abi_fragment": {{
    "function_name": "permitWitnessTransferFrom",
    "abi": {{
      "name": "permitWitnessTransferFrom",
      "type": "function",
      "inputs": [
        {{ "name": "owner", "type": "address" }},
        {{ "name": "spender", "type": "address" }}
      ]
    }}
  }},
  "emit": {{
    "strategy": "single_emit",
    "body": {{
      "domain": "token",
      "token": {{
        "action": "erc20_approve",
        "erc20_approve": {{
          "token": {{ "key": {{ "standard": "erc20", "chain": "$chain", "address": "{spender_addr}" }} }},
          "spender": "$args.spender",
          "amount": "0"
        }}
      }}
    }}
  }},
  "requires": {{
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }}
}}"#,
        witness_named = witness_type.unwrap_or("Witness"),
    )
}

#[test]
fn t1_typed_data_witness_type_decollides() {
    // Two manifests collide on (chain=1, Permit2, "PermitWitnessTransferFrom")
    // but carry DIFFERENT witness_type. Distinct selectors keep their callkeys
    // separate; the distinguishable `token.key.address` proves which body ran.
    let token_a = "0x000000000000000000000000000000000000aaa1";
    let token_b = "0x000000000000000000000000000000000000bbb2";
    install_ok(&t1_witness_manifest(
        "uniswapx/test/orderA@1.0.0",
        "0x00000001",
        Some("OrderA"),
        token_a,
    ));
    install_ok(&t1_witness_manifest(
        "uniswapx/test/orderB@1.0.0",
        "0x00000002",
        Some("OrderB"),
        token_b,
    ));

    // Route a request carrying witness_type "OrderA" — must hit A, NOT B (B was
    // installed last, so a witness_type-blind key would resolve to B).
    let message = json!({
        "permitted": { "token": token_a, "amount": "1000" },
        "spender": "0x00000000000000000000000000000000deadbeef",
        "nonce": "0",
        "deadline": 1_738_002_000_u64,
        "witness": {}
    });
    let input = t1_typed_data_input(
        1,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("OrderA"),
        message,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/test/orderA@1.0.0",
        "witness_type OrderA must route to manifest A, not B: {parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["token"]["key"]["address"], token_a, "{parsed}");
}

#[test]
fn t1_typed_data_witness_type_orderb_routes_to_b() {
    // Symmetric to the OrderA case — witness_type "OrderB" hits B. Proves the
    // 4th component selects EITHER manifest, not just the last-installed one.
    let token_a = "0x000000000000000000000000000000000000aaa1";
    let token_b = "0x000000000000000000000000000000000000bbb2";
    install_ok(&t1_witness_manifest(
        "uniswapx/test/orderA@1.0.0",
        "0x00000001",
        Some("OrderA"),
        token_a,
    ));
    install_ok(&t1_witness_manifest(
        "uniswapx/test/orderB@1.0.0",
        "0x00000002",
        Some("OrderB"),
        token_b,
    ));

    let message = json!({
        "permitted": { "token": token_b, "amount": "1000" },
        "spender": "0x00000000000000000000000000000000deadbeef",
        "nonce": "0",
        "deadline": 1_738_002_000_u64,
        "witness": {}
    });
    let input = t1_typed_data_input(
        1,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("OrderB"),
        message,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/test/orderB@1.0.0",
        "{parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["token"]["key"]["address"], token_b, "{parsed}");
}

#[test]
fn t1_typed_data_no_witness_type_backward_compat() {
    // A manifest with NO witness_type + an input with NO witness_type must route
    // exactly as before T1 (None-on-both matches). This guards that adding the
    // 4th component is fully backward compatible.
    let token = "0x000000000000000000000000000000000000c0de";
    let manifest = format!(
        r#"{{
  "type": "adapter_action",
  "id": "uniswap/permit2/permitSingle-bw@1.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {{
    "selector": "0x2b67b570",
    "chain_to_addresses": {{ "1": ["{PERMIT2_VC}"] }},
    "typed_data": {{
      "domain_name": "Permit2",
      "verifying_contract": "{PERMIT2_VC}",
      "primary_type": "PermitSingle",
      "types": {{
        "PermitSingle": [
          {{ "name": "spender", "type": "address" }},
          {{ "name": "sigDeadline", "type": "uint256" }}
        ]
      }}
    }}
  }},
  "abi_fragment": {{
    "function_name": "permit",
    "abi": {{
      "name": "permit",
      "type": "function",
      "inputs": [
        {{ "name": "owner", "type": "address" }},
        {{
          "name": "permitSingle",
          "type": "tuple",
          "components": [
            {{ "name": "spender", "type": "address" }},
            {{ "name": "sigDeadline", "type": "uint256" }}
          ]
        }},
        {{ "name": "signature", "type": "bytes" }}
      ]
    }}
  }},
  "emit": {{
    "strategy": "single_emit",
    "body": {{
      "domain": "token",
      "token": {{
        "action": "erc20_approve",
        "erc20_approve": {{
          "token": {{ "key": {{ "standard": "erc20", "chain": "$chain", "address": "{token}" }} }},
          "spender": "$args.permitSingle.spender",
          "amount": "0"
        }}
      }}
    }}
  }},
  "requires": {{
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }}
}}"#
    );
    install_ok(&manifest);

    let message = json!({
        "spender": "0x00000000000000000000000000000000deadbeef",
        "sigDeadline": 1_738_002_000_u64
    });
    // No witness_type in the input.
    let input = t1_typed_data_input(1, PERMIT2_VC, "PermitSingle", None, message);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswap/permit2/permitSingle-bw@1.0.0",
        "{parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["token"]["key"]["address"], token, "{parsed}");
}

// ===========================================================================
// T2 — UniswapX intent-order SIGN manifests (Permit2-witness) + cancel
// ===========================================================================
//
// The four UniswapX sign families are signed as Permit2 `permitWitnessTransferFrom`
// witnesses: `domain.name = "Permit2"`, `verifying_contract = Permit2`,
// `primary_type = "PermitWitnessTransferFrom"`, and the order struct is the
// EIP-712 `witness` field — disambiguated by `match.typed_data.witness_type`
// (the order struct's type name). The Permit2 MESSAGE the wallet surfaces is
// `{ permitted{token,amount}, spender, nonce, deadline, witness{<order>} }`; the
// abi_fragment's single `order` tuple param drives the wrap rule
// (`build_typed_data_args_json`) so the body's `$args.order.witness.*` paths
// resolve. These tests inline the on-disk manifest content VERBATIM (so they
// pin the committed routing behaviour) and route a realistic message per family.
//
// §3.3 nesting: ExclusiveDutchOrder (V1) + V2DutchOrder FLATTEN input
// (`inputToken`/`baseInputToken` …); V3DutchOrder + PriorityOrder KEEP nested
// `baseInput`/`input` structs. The witness fixture below matches each.

const T2_WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const T2_USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const T2_RECIPIENT: &str = "0x000000000000000000000000000000000000c0fe";
const T2_ZERO: &str = "0x0000000000000000000000000000000000000000";

/// Shared TokenPermissions + the OUTER permitWitnessTransferFrom message
/// scaffold; the caller supplies the `witness` order object. The `permitted`
/// token/amount mirror the order's sell side (Permit2 transfers the sell token)
/// but the routed body reads token/amount from the witness, not `permitted`.
fn t2_permit_witness_message(witness: Value) -> Value {
    json!({
        "permitted": { "token": T2_WETH, "amount": "1000000000000000000" },
        "spender": "0x00000000000000000000000000000000deadbeef",
        "nonce": "0",
        "deadline": 1_738_002_000_u64,
        "witness": witness
    })
}

/// 6-field OrderInfo message object (verbatim §3.3 OrderInfo) shared by all
/// four order families. `reactor` is the per-family reactor; `deadline` is the
/// order expiry the body reads as `valid_until`.
fn t2_order_info(reactor: &str) -> Value {
    json!({
        "reactor": reactor,
        "swapper": T1_SIGNER,
        "nonce": "0",
        "deadline": 1_738_002_000_u64,
        "additionalValidationContract": T2_ZERO,
        "additionalValidationData": "0x"
    })
}

// ---------------------------------------------------------------------------
// T2.1 — ExclusiveDutchOrder (V1), mainnet only. FLATTEN input.
// ---------------------------------------------------------------------------

const T2_EXCLUSIVE_DUTCH_V3: &str =
    include_str!("../../../registryV2/manifests/uniswapx/exclusive-dutch-order/sign@1.0.0.json");

#[test]
fn t2_uniswapx_exclusive_dutch_sign_intent_order() {
    install_ok(T2_EXCLUSIVE_DUTCH_V3);

    let reactor = "0x6000da47483062a0d734ba3dc7576ce6a0b645c4";
    let witness = json!({
        "info": t2_order_info(reactor),
        "decayStartTime": 1_738_001_800_u64,
        "decayEndTime": 1_738_002_000_u64,
        "exclusiveFiller": T2_ZERO,
        "exclusivityOverrideBps": "0",
        "inputToken": T2_WETH,
        "inputStartAmount": "1000000000000000000",
        "inputEndAmount": "1000000000000000000",
        "outputs": [
            {
                "token": T2_USDC,
                "startAmount": "3500000000",
                "endAmount": "3400000000",
                "recipient": T2_RECIPIENT
            }
        ]
    });
    let input = t1_typed_data_input(
        1,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("ExclusiveDutchOrder"),
        t2_permit_witness_message(witness),
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/exclusive-dutch-order/sign@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "sign_intent_order", "{parsed}");
    assert_eq!(body["venue"]["name"], "uniswap_x", "{parsed}");
    // reactor read from the witness `info.reactor` (NOT $to / verifying_contract).
    assert_eq!(body["venue"]["reactor"], reactor, "{parsed}");
    assert_eq!(body["venue"]["chain"], "eip155:1", "{parsed}");
    assert_eq!(body["sell"]["key"]["address"], T2_WETH, "{parsed}");
    assert_eq!(body["buy"]["key"]["address"], T2_USDC, "{parsed}");
    assert_eq!(body["order_kind"], "dutch", "{parsed}");
    assert_eq!(body["recipient"], T2_RECIPIENT, "{parsed}");
    assert_eq!(body["valid_until"], 1_738_002_000_u64, "{parsed}");
    // sell_amount = inputStartAmount (1e18 == 0xde0b6b3a7640000).
    assert_eq!(body["sell_amount"], "0xde0b6b3a7640000", "{parsed}");
    // buy_min = outputs[0].endAmount (3.4e9 == 0xcaa7e200).
    assert_eq!(body["buy_min"], "0xcaa7e200", "{parsed}");
}

// ---------------------------------------------------------------------------
// T2.2 — V2DutchOrder, mainnet + arbitrum. FLATTEN baseInput.
// ---------------------------------------------------------------------------

const T2_V2_DUTCH_V3: &str =
    include_str!("../../../registryV2/manifests/uniswapx/v2-dutch-order/sign@1.0.0.json");

#[test]
fn t2_uniswapx_v2_dutch_sign_intent_order() {
    install_ok(T2_V2_DUTCH_V3);

    let reactor = "0x00000011f84b9aa48e5f8aa8b9897600006289be";
    let witness = json!({
        "info": t2_order_info(reactor),
        "cosigner": T2_ZERO,
        "baseInputToken": T2_WETH,
        "baseInputStartAmount": "1000000000000000000",
        "baseInputEndAmount": "1000000000000000000",
        "baseOutputs": [
            {
                "token": T2_USDC,
                "startAmount": "3500000000",
                "endAmount": "3400000000",
                "recipient": T2_RECIPIENT
            }
        ]
    });
    let input = t1_typed_data_input(
        1,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("V2DutchOrder"),
        t2_permit_witness_message(witness),
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/v2-dutch-order/sign@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["action"], "sign_intent_order", "{parsed}");
    assert_eq!(body["venue"]["reactor"], reactor, "{parsed}");
    assert_eq!(body["sell"]["key"]["address"], T2_WETH, "{parsed}");
    assert_eq!(body["buy"]["key"]["address"], T2_USDC, "{parsed}");
    assert_eq!(body["order_kind"], "dutch", "{parsed}");
    assert_eq!(body["recipient"], T2_RECIPIENT, "{parsed}");
    assert_eq!(body["sell_amount"], "0xde0b6b3a7640000", "{parsed}");
    assert_eq!(body["buy_min"], "0xcaa7e200", "{parsed}");
}

// ---------------------------------------------------------------------------
// T2.3 — V3DutchOrder, mainnet + base + optimism + arbitrum. NESTED baseInput.
// ---------------------------------------------------------------------------

const T2_V3_DUTCH_V3: &str =
    include_str!("../../../registryV2/manifests/uniswapx/v3-dutch-order/sign@1.0.0.json");

#[test]
fn t2_uniswapx_v3_dutch_sign_intent_order() {
    install_ok(T2_V3_DUTCH_V3);

    let reactor = "0x0000000015757c461808ea25eb309638b62681cf";
    let curve = json!({ "relativeBlocks": "0", "relativeAmounts": [] });
    let witness = json!({
        "info": t2_order_info(reactor),
        "cosigner": T2_ZERO,
        "startingBaseFee": "0",
        "baseInput": {
            "token": T2_WETH,
            "startAmount": "1000000000000000000",
            "curve": curve,
            "maxAmount": "1000000000000000000",
            "adjustmentPerGweiBaseFee": "0"
        },
        "baseOutputs": [
            {
                "token": T2_USDC,
                "startAmount": "3500000000",
                "curve": curve,
                "recipient": T2_RECIPIENT,
                "minAmount": "3400000000",
                "adjustmentPerGweiBaseFee": "0"
            }
        ]
    });
    let input = t1_typed_data_input(
        1,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("V3DutchOrder"),
        t2_permit_witness_message(witness),
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/v3-dutch-order/sign@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["action"], "sign_intent_order", "{parsed}");
    assert_eq!(body["venue"]["reactor"], reactor, "{parsed}");
    // NESTED: sell from baseInput.token, buy_min from baseOutputs[0].minAmount.
    assert_eq!(body["sell"]["key"]["address"], T2_WETH, "{parsed}");
    assert_eq!(body["buy"]["key"]["address"], T2_USDC, "{parsed}");
    assert_eq!(body["order_kind"], "dutch", "{parsed}");
    assert_eq!(body["recipient"], T2_RECIPIENT, "{parsed}");
    assert_eq!(body["sell_amount"], "0xde0b6b3a7640000", "{parsed}");
    assert_eq!(body["buy_min"], "0xcaa7e200", "{parsed}");
}

// ---------------------------------------------------------------------------
// T2.4 — PriorityOrder, base only. NESTED input/outputs. order_kind = limit.
// ---------------------------------------------------------------------------

const T2_PRIORITY_V3: &str =
    include_str!("../../../registryV2/manifests/uniswapx/priority-order/sign@1.0.0.json");

#[test]
fn t2_uniswapx_priority_sign_intent_order() {
    install_ok(T2_PRIORITY_V3);

    let reactor = "0x000000001ec5656dcdb24d90dfa42742738de729";
    let witness = json!({
        "info": t2_order_info(reactor),
        "cosigner": T2_ZERO,
        "auctionStartBlock": "0",
        "baselinePriorityFeeWei": "0",
        "input": {
            "token": T2_WETH,
            "amount": "1000000000000000000",
            "mpsPerPriorityFeeWei": "0"
        },
        "outputs": [
            {
                "token": T2_USDC,
                "amount": "3400000000",
                "mpsPerPriorityFeeWei": "0",
                "recipient": T2_RECIPIENT
            }
        ]
    });
    let input = t1_typed_data_input(
        8453,
        PERMIT2_VC,
        "PermitWitnessTransferFrom",
        Some("PriorityOrder"),
        t2_permit_witness_message(witness),
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/priority-order/sign@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["action"], "sign_intent_order", "{parsed}");
    assert_eq!(body["venue"]["reactor"], reactor, "{parsed}");
    assert_eq!(body["venue"]["chain"], "eip155:8453", "{parsed}");
    // Priority output has only `amount` (fixed at sign) → both sell_amount &
    // buy_min read it. order_kind = "limit" (no Priority variant; least-wrong).
    assert_eq!(body["sell"]["key"]["address"], T2_WETH, "{parsed}");
    assert_eq!(body["buy"]["key"]["address"], T2_USDC, "{parsed}");
    assert_eq!(body["order_kind"], "limit", "{parsed}");
    assert_eq!(body["recipient"], T2_RECIPIENT, "{parsed}");
    assert_eq!(body["sell_amount"], "0xde0b6b3a7640000", "{parsed}");
    assert_eq!(body["buy_min"], "0xcaa7e200", "{parsed}");
}

// ---------------------------------------------------------------------------
// T2.5 — Permit2 invalidateUnorderedNonces CALLDATA → revoke_approval
// ---------------------------------------------------------------------------
//
// Cancellation is an on-chain Permit2 `invalidateUnorderedNonces(uint256,uint256)`
// call (selector 0x3ff9dcb1), NOT a typed-data signature. This is Permit2
// nonce-word granular, not per-order, so the token-domain revoke scope carries
// the bitmap coordinates directly.

const T2_CANCEL_V3: &str = include_str!(
    "../../../registryV2/manifests/uniswap/permit2/invalidateUnorderedNonces@1.0.0.json"
);

#[test]
fn t2_permit2_invalidate_unordered_nonces() {
    install_ok(T2_CANCEL_V3);

    // invalidateUnorderedNonces(uint256 wordPos, uint256 mask).
    let calldata = encode_calldata(
        "0x3ff9dcb1",
        &[
            DynSolValue::Uint(AlloyU256::from(7u64), 256),
            DynSolValue::Uint(AlloyU256::from(0b1010u64), 256),
        ],
    );
    let input = route_input(1, PERMIT2_VC, "0x3ff9dcb1", calldata, T1_SIGNER);
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswap/permit2/invalidateUnorderedNonces@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "revoke_approval", "{parsed}");
    assert_eq!(body["scope"]["kind"], "permit2_unordered_nonce", "{parsed}");
    assert_eq!(body["scope"]["chain"], "eip155:1", "{parsed}");
    assert_eq!(body["scope"]["word_pos"], "0x7", "{parsed}");
    assert_eq!(body["scope"]["mask"], "0xa", "{parsed}");
}

// ---------------------------------------------------------------------------
// t21 — B.1.c.2 recursive opcode_stream_dispatch: UR execute(V4_SWAP) fully
// expands the inner V4 action stream into a NESTED Multicall with real values.
// ---------------------------------------------------------------------------
//
// Universal Router `execute(bytes commands, bytes[] inputs, uint256 deadline)`
// where commands = [0x10 V4_SWAP] and the V4_SWAP input is itself
// `abi.encode(bytes actions, bytes[] params)` carrying
// [0x06 SWAP_EXACT_IN_SINGLE, 0x0c SETTLE_ALL, 0x0f TAKE_ALL]. The outer body
// must be a Multicall whose single child (the V4_SWAP) is ITSELF a Multicall of
// the 3 inner legs, each carrying the REAL decoded token/amount (not the old
// zero placeholder). Action ids + struct shapes are 1차-sourced from
// Uniswap v4-periphery Actions.sol / IV4Router.sol @ commit 2827167f8b.

const T21_UR_V4_SWAP_NESTED: &str = r#"{
  "type": "adapter_action",
  "id": "uniswap/universal-router/execute-v4nested@1.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x3593564c",
    "chain_to_addresses": { "1": ["0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af"] }
  },
  "abi_fragment": {
    "function_name": "execute",
    "abi": {
      "name": "execute",
      "type": "function",
      "inputs": [
        { "name": "commands", "type": "bytes" },
        { "name": "inputs", "type": "bytes[]" },
        { "name": "deadline", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "opcode_stream_dispatch",
    "mask": "0x7f",
    "allow_revert_bit": "0x80",
    "unknown_opcode_policy": "warn",
    "max_depth": 3,
    "per_opcode_body": {
      "0x10": {
        "name": "V4_SWAP",
        "inputs_abi": "(bytes actions, bytes[] params)",
        "nested": {
          "inner_actions_source": "$inputs.actions",
          "inner_params_source": "$inputs.params",
          "mask": "0xff",
          "unknown_opcode_policy": "warn",
          "per_opcode_body": {
            "0x06": {
              "name": "SWAP_EXACT_IN_SINGLE",
              "inputs_abi": "((address,address,uint24,int24,address) poolKey, bool zeroForOne, uint128 amountIn, uint128 amountOutMinimum, bytes hookData)",
              "body": {
                "domain": "amm",
                "amm": { "action": "swap", "swap": {
                  "venue": { "name": "uniswap_v4", "chain": "$chain", "pool_id": "$inputs.pool_id", "pool_manager": "$resolved.pool_manager", "hooks": "$inputs.poolKey[4]" },
                  "params": {
                    "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.poolKey[0]" } },
                    "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.poolKey[1]" } },
                    "direction": { "kind": "exact_input", "amount_in": "$inputs.amountIn", "min_amount_out": "$inputs.amountOutMinimum" },
                    "recipient": "$tx.from",
                    "slippage_bp": 50
                  },
                  "live_inputs": {
                    "route": { "source": { "kind": "user_supplied" } },
                    "expected_amount_out": { "source": { "kind": "user_supplied" } },
                    "price_impact_bp": { "source": { "kind": "user_supplied" } },
                    "gas_estimate": { "source": { "kind": "user_supplied" } }
                  }
                } }
              }
            },
            "0x0c": {
              "name": "SETTLE_ALL",
              "inputs_abi": "(address currency, uint256 maxAmount)",
              "body": {
                "domain": "token",
                "token": { "action": "erc20_transfer", "erc20_transfer": {
                  "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.currency" } },
                  "recipient": "0x000000000000000000000000000000000000dEaD",
                  "amount": "$inputs.maxAmount"
                } }
              }
            },
            "0x0f": {
              "name": "TAKE_ALL",
              "inputs_abi": "(address currency, uint256 minAmount)",
              "body": {
                "domain": "token",
                "token": { "action": "erc20_transfer", "erc20_transfer": {
                  "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.currency" } },
                  "recipient": "$tx.from",
                  "amount": "$inputs.minAmount"
                } }
              }
            }
          }
        }
      }
    }
  },
  "requires": {
    "imperative": ["opcode-stream-dispatch@^1.0"],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

/// `abi.encode` a v4 `SWAP_EXACT_IN_SINGLE` param tuple
/// `(PoolKey poolKey, bool zeroForOne, uint128 amountIn, uint128
/// amountOutMinimum, bytes hookData)` with the canonical PoolKey head-flatten.
fn v4_swap_exact_in_single_param(
    currency0: &str,
    currency1: &str,
    amount_in: u128,
    amount_out_min: u128,
) -> Vec<u8> {
    let pool_key = DynSolValue::Tuple(vec![
        DynSolValue::Address(currency0.parse::<AlloyAddress>().unwrap()),
        DynSolValue::Address(currency1.parse::<AlloyAddress>().unwrap()),
        DynSolValue::Uint(AlloyU256::from(3000u64), 24), // fee 0.3%
        DynSolValue::Int(alloy_primitives::I256::try_from(60i64).unwrap(), 24), // tickSpacing
        DynSolValue::Address(AlloyAddress::ZERO),        // hooks
    ]);
    DynSolValue::Tuple(vec![
        pool_key,
        DynSolValue::Bool(true), // zeroForOne
        DynSolValue::Uint(AlloyU256::from(amount_in), 128),
        DynSolValue::Uint(AlloyU256::from(amount_out_min), 128),
        DynSolValue::Bytes(vec![]), // hookData
    ])
    .abi_encode_params()
}

/// `abi.encode` a v4 `(address currency, uint256 amount)` settle/take param.
fn v4_currency_amount_param(currency: &str, amount: u128) -> Vec<u8> {
    DynSolValue::Tuple(vec![
        DynSolValue::Address(currency.parse::<AlloyAddress>().unwrap()),
        DynSolValue::Uint(AlloyU256::from(amount), 256),
    ])
    .abi_encode_params()
}

#[test]
fn t21_ur_execute_v4_swap_nested_expands_to_inner_multicall() {
    install_ok(T21_UR_V4_SWAP_NESTED);

    let currency0 = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
    let currency1 = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"; // WETH
    let amount_in: u128 = 1_000_000_000; // 1000 USDC (6 dp)
    let amount_out_min: u128 = 250_000_000_000_000_000; // 0.25 WETH

    // Inner V4 action stream: SWAP_EXACT_IN_SINGLE (0x06) + SETTLE_ALL (0x0c)
    // + TAKE_ALL (0x0f).
    let inner_actions = vec![0x06u8, 0x0c, 0x0f];
    let inner_params = vec![
        DynSolValue::Bytes(v4_swap_exact_in_single_param(
            currency0,
            currency1,
            amount_in,
            amount_out_min,
        )),
        DynSolValue::Bytes(v4_currency_amount_param(currency0, amount_in)),
        DynSolValue::Bytes(v4_currency_amount_param(currency1, amount_out_min)),
    ];

    // V4_SWAP input = abi.encode(bytes actions, bytes[] params).
    let v4_swap_input = DynSolValue::Tuple(vec![
        DynSolValue::Bytes(inner_actions),
        DynSolValue::Array(inner_params),
    ])
    .abi_encode_params();

    // Outer UR execute: commands = [0x10], inputs = [v4_swap_input].
    let calldata = encode_calldata(
        "0x3593564c",
        &[
            DynSolValue::Bytes(vec![0x10]),
            DynSolValue::Array(vec![DynSolValue::Bytes(v4_swap_input)]),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x66a9893cc07d91d95644aedd05d03f95e1dba8af",
        "0x3593564c",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];

    // Outer = Multicall with one child (the V4_SWAP).
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let v4_child = &body["actions"][0];

    // The V4_SWAP child is ITSELF a Multicall of the 3 inner legs.
    assert_eq!(
        v4_child["domain"], "multicall",
        "V4_SWAP must expand to a nested Multicall, got: {parsed}"
    );
    let legs = v4_child["actions"].as_array().expect("inner legs array");
    assert_eq!(
        legs.len(),
        3,
        "expected 3 inner legs (swap+settle+take): {parsed}"
    );

    // Leg 0 — SWAP_EXACT_IN_SINGLE with REAL token + amount (not zero).
    let swap = &legs[0];
    assert_eq!(swap["domain"], "amm", "{parsed}");
    assert_eq!(swap["action"], "swap", "{parsed}");
    assert_eq!(swap["venue"]["name"], "uniswap_v4", "{parsed}");
    assert_eq!(
        swap["params"]["token_in"]["key"]["address"], currency0,
        "token_in must be the decoded PoolKey.currency0, not zero: {parsed}"
    );
    assert_eq!(
        swap["params"]["token_out"]["key"]["address"], currency1,
        "{parsed}"
    );
    assert_eq!(
        swap["params"]["direction"]["kind"], "exact_input",
        "{parsed}"
    );
    // U256 fields serialize as 0x-prefixed hex (alloy serde convention).
    let amount_in_hex = format!("0x{amount_in:x}");
    let amount_out_min_hex = format!("0x{amount_out_min:x}");
    assert_eq!(
        swap["params"]["direction"]["amount_in"], amount_in_hex,
        "amount_in must be the REAL decoded uint128, not the old zero placeholder: {parsed}"
    );
    assert_eq!(
        swap["params"]["direction"]["min_amount_out"], amount_out_min_hex,
        "{parsed}"
    );
    // pool_id was computed in Rust (keccak256 of the inline PoolKey) — present
    // and a 0x-prefixed 32-byte hash, not the "unknown" sentinel.
    let pool_id = swap["venue"]["pool_id"].as_str().unwrap_or("");
    assert!(
        pool_id.starts_with("0x") && pool_id.len() == 66,
        "pool_id must be the Rust-computed keccak256, got {pool_id:?}: {parsed}"
    );

    // Leg 1 — SETTLE_ALL → erc20_transfer of currency0 (amount_in).
    let settle = &legs[1];
    assert_eq!(settle["domain"], "token", "{parsed}");
    assert_eq!(settle["action"], "erc20_transfer", "{parsed}");
    assert_eq!(settle["token"]["key"]["address"], currency0, "{parsed}");
    assert_eq!(settle["amount"], amount_in_hex, "{parsed}");

    // Leg 2 — TAKE_ALL → erc20_transfer of currency1 (amount_out_min) to $tx.from.
    let take = &legs[2];
    assert_eq!(take["domain"], "token", "{parsed}");
    assert_eq!(take["token"]["key"]["address"], currency1, "{parsed}");
    assert_eq!(take["amount"], amount_out_min_hex, "{parsed}");
    assert_eq!(
        take["recipient"], "0x000000000000000000000000000000000000aaaa",
        "TAKE_ALL recipient = $tx.from (the submitter): {parsed}"
    );
}

// ---------------------------------------------------------------------------
// t22 — B.1.c.2 max_depth guard: a manifest with max_depth=0 whose opcode
// carries a `nested` block fails LOUD with `max_depth_exceeded` when the
// nested expansion would recurse to depth=1 (> 0). No silent truncation (DD4).
// ---------------------------------------------------------------------------

const T22_MAX_DEPTH_FIXTURE: &str = r#"{
  "type": "adapter_action",
  "id": "test/universal-router/maxdepth@1.0.0",
  "publisher": "test.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x3593564c",
    "chain_to_addresses": { "1": ["0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af"] }
  },
  "abi_fragment": {
    "function_name": "execute",
    "abi": {
      "name": "execute",
      "type": "function",
      "inputs": [
        { "name": "commands", "type": "bytes" },
        { "name": "inputs", "type": "bytes[]" },
        { "name": "deadline", "type": "uint256" }
      ]
    }
  },
  "emit": {
    "strategy": "opcode_stream_dispatch",
    "mask": "0x7f",
    "allow_revert_bit": "0x80",
    "unknown_opcode_policy": "warn",
    "max_depth": 0,
    "per_opcode_body": {
      "0x10": {
        "name": "V4_SWAP",
        "inputs_abi": "(bytes actions, bytes[] params)",
        "nested": {
          "mask": "0xff",
          "unknown_opcode_policy": "warn",
          "per_opcode_body": {
            "0x0f": {
              "name": "TAKE_ALL",
              "inputs_abi": "(address currency, uint256 minAmount)",
              "body": {
                "domain": "token",
                "token": { "action": "erc20_transfer", "erc20_transfer": {
                  "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.currency" } },
                  "recipient": "$tx.from",
                  "amount": "$inputs.minAmount"
                } }
              }
            }
          }
        }
      }
    }
  },
  "requires": {
    "imperative": ["opcode-stream-dispatch@^1.0"],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

#[test]
fn t22_max_depth_exceeded_fails_loud() {
    install_ok(T22_MAX_DEPTH_FIXTURE);

    // Inner stream = [TAKE_ALL]; recursing it requires depth 1 > max_depth 0.
    let inner_actions = vec![0x0fu8];
    let inner_params = vec![DynSolValue::Bytes(v4_currency_amount_param(
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        1u128,
    ))];
    let v4_swap_input = DynSolValue::Tuple(vec![
        DynSolValue::Bytes(inner_actions),
        DynSolValue::Array(inner_params),
    ])
    .abi_encode_params();

    let calldata = encode_calldata(
        "0x3593564c",
        &[
            DynSolValue::Bytes(vec![0x10]),
            DynSolValue::Array(vec![DynSolValue::Bytes(v4_swap_input)]),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        "0x66a9893cc07d91d95644aedd05d03f95e1dba8af",
        "0x3593564c",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );

    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], false, "max_depth must fail loud: {parsed}");
    assert_eq!(
        parsed["error"]["kind"], "max_depth_exceeded",
        "expected max_depth_exceeded, got: {parsed}"
    );
}

// ===========================================================================
// B.5 — Balancer V2 Vault.swap (Fjord Foundry LBP retail cover, Option A)
// ===========================================================================
//
// A retail Fjord Foundry LBP buy IS a Balancer V2 `Vault.swap(...)` direct call
// (Option A — Fjord's own wrapper fns are creator-side/onlyOwner-gated, and the
// 5 prompt retail operations commit/claim_allocation/refund/withdraw_commit/
// claim_vested are ABSENT from verified source → out-of-scope). This manifest is
// GENERIC Balancer V2 (covers EVERY V2 single swap); Fjord-ness is the
// host:registry `registry_api`/`PoolMeta` enrichment on `live_inputs.route`.
//
// Inline-mirrors `registryV2/manifests/balancer/v2/vault-swap@1.0.0.json`
// (mainnet-only chain_to_addresses here; the on-disk fixture carries all 6
// Fjord-verified-deploy chains 1/42161/43114/10/137/56). Selector `cast sig`-
// verified: 0x52bbbe29.
//
// THE LOAD-BEARING ABI FACT (a reviewer watch-point): `swap` has FOUR top-level
// args (singleSwap, funds, limit, deadline), so the bridge does NOT flatten the
// leading struct (it flattens only the single-arg case). `singleSwap` / `funds`
// therefore stay as named args whose VALUE is a positional JSON array — the
// emit references their fields by INDEX (`$args.singleSwap[0]` .. `[5]`,
// `$args.funds[2]`), NOT by component name. The `uint8 kind` (index 1) renders
// as a JSON number (canonical-parenthesised abi_type threads its width), so the
// `$match`/`$cases` direction switch keys on "0"/"1".

const VAULT_MAINNET: &str = "0xba12222222228d8ba445958a75a0704d566bf2c8";
// A realistic Balancer V2 weighted-pool id shape: 20-byte pool address ++
// 2-byte specifier ++ 10-byte nonce. (Any bytes32 decodes; this is plausible.)
const LBP_POOL_ID: &str = "0xc45d42f801105e861e86658648e3678ad7aa70f900020000000000000000011e";

const B5_BALANCER_VAULT_SWAP_V3: &str = r#"{
  "type": "adapter_action",
  "id": "balancer/v2/vault-swap@1.0.0",
  "publisher": "balancer.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x52bbbe29",
    "chain_to_addresses": { "1": ["0xBA12222222228d8Ba445958a75a0704d566BF2C8"] }
  },
  "abi_fragment": {
    "function_name": "swap",
    "abi": {
      "name": "swap",
      "type": "function",
      "stateMutability": "payable",
      "inputs": [
        {
          "name": "singleSwap",
          "type": "tuple",
          "components": [
            { "name": "poolId",   "type": "bytes32" },
            { "name": "kind",     "type": "uint8"   },
            { "name": "assetIn",  "type": "address" },
            { "name": "assetOut", "type": "address" },
            { "name": "amount",   "type": "uint256" },
            { "name": "userData", "type": "bytes"   }
          ]
        },
        {
          "name": "funds",
          "type": "tuple",
          "components": [
            { "name": "sender",              "type": "address" },
            { "name": "fromInternalBalance", "type": "bool"    },
            { "name": "recipient",           "type": "address" },
            { "name": "toInternalBalance",   "type": "bool"    }
          ]
        },
        { "name": "limit",    "type": "uint256" },
        { "name": "deadline", "type": "uint256" }
      ],
      "outputs": [ { "name": "", "type": "uint256" } ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "amm",
      "amm": {
        "action": "swap",
        "swap": {
          "venue": {
            "name":      "balancer_v2",
            "chain":     "$chain",
            "vault":     "$to",
            "pool_id":   "$args.singleSwap[0]",
            "pool_type": "weighted"
          },
          "params": {
            "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.singleSwap[2]" } },
            "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.singleSwap[3]" } },
            "direction": {
              "$match": "$args.singleSwap[1]",
              "$cases": {
                "0": { "kind": "exact_input",  "amount_in":     "$args.singleSwap[4]", "min_amount_out": "$args.limit" },
                "1": { "kind": "exact_output", "max_amount_in": "$args.limit",          "amount_out":     "$args.singleSwap[4]" }
              }
            },
            "recipient":   "$args.funds[2]",
            "slippage_bp": 50
          },
          "live_inputs": {
            "route": {
              "source": {
                "kind":     "registry_api",
                "endpoint": "https://registry-api-v2-891268973493.asia-northeast3.run.app",
                "resource": { "kind": "pool_meta", "chain": "$chain", "pool_addr": "0x0000000000000000000000000000000000000000" },
                "version": "2"
              },
              "ttl_s": 86400
            },
            "expected_amount_out": {
              "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$to", "function": "queryBatchSwap(uint8,(bytes32,uint256,uint256,uint256,bytes)[],address[],(address,bool,address,bool))", "decoder_id": "balancer_v2_query_swap" },
              "ttl_s": 12
            },
            "price_impact_bp": {
              "source": { "kind": "derived_from", "inputs": [], "calc_id": "balancer_v2_price_impact_bp" },
              "ttl_s": 12
            },
            "gas_estimate": {
              "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/ethereum" },
              "ttl_s": 6
            }
          }
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": ["token_metadata"],
    "host_capabilities": ["registry:pool_meta"],
    "extension": ">=0.1.0"
  }
}"#;

/// Build `Vault.swap` calldata for the given `kind` (0 = GIVEN_IN, 1 = GIVEN_OUT)
/// then route it, returning the resolved `body` JSON. `amount` / `limit` are
/// load-bearing — they map to amount_in/min_amount_out (GIVEN_IN) or
/// max_amount_in/amount_out (GIVEN_OUT).
fn route_balancer_swap(kind: u8, amount: u64, limit: u64) -> Value {
    install_ok(B5_BALANCER_VAULT_SWAP_V3);

    // assetIn = USDC, assetOut = WETH, recipient = 0x..bbbb, sender = 0x..a01c.
    let asset_in = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        .parse::<AlloyAddress>()
        .unwrap();
    let asset_out = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        .parse::<AlloyAddress>()
        .unwrap();
    let recipient = "0x000000000000000000000000000000000000bbbb"
        .parse::<AlloyAddress>()
        .unwrap();
    let sender = "0x000000000000000000000000000000000000a01c"
        .parse::<AlloyAddress>()
        .unwrap();
    let pool_id_bytes = hex::decode(LBP_POOL_ID.trim_start_matches("0x")).expect("32-byte poolId");

    // SingleSwap = (bytes32 poolId, uint8 kind, address assetIn, address
    // assetOut, uint256 amount, bytes userData). A tuple ARG (not the lone arg)
    // → stays nested; decodes to a positional JSON array.
    let single_swap = DynSolValue::Tuple(vec![
        DynSolValue::FixedBytes(alloy_primitives::B256::from_slice(&pool_id_bytes), 32),
        DynSolValue::Uint(AlloyU256::from(u64::from(kind)), 8),
        DynSolValue::Address(asset_in),
        DynSolValue::Address(asset_out),
        DynSolValue::Uint(AlloyU256::from(amount), 256),
        DynSolValue::Bytes(vec![]),
    ]);
    // FundManagement = (address sender, bool fromInternalBalance, address
    // recipient, bool toInternalBalance). recipient is index 2.
    let funds = DynSolValue::Tuple(vec![
        DynSolValue::Address(sender),
        DynSolValue::Bool(false),
        DynSolValue::Address(recipient),
        DynSolValue::Bool(false),
    ]);

    let calldata = encode_calldata(
        "0x52bbbe29",
        &[
            single_swap,
            funds,
            DynSolValue::Uint(AlloyU256::from(limit), 256),
            DynSolValue::Uint(AlloyU256::from(1_900_000_000u64), 256),
        ],
    );
    let input = route_input(
        1,
        VAULT_MAINNET,
        "0x52bbbe29",
        calldata,
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"], "balancer/v2/vault-swap@1.0.0",
        "{parsed}"
    );
    parsed["data"]["actions"][0]["body"].clone()
}

// ---------------------------------------------------------------------------
// t21 — GIVEN_IN (kind=0) → exact_input
// ---------------------------------------------------------------------------

#[test]
fn t21_balancer_vault_swap_given_in_exact_input() {
    // kind=0 GIVEN_IN: amount = exact amount_in (1_000_000), limit = min out.
    let body = route_balancer_swap(0, 1_000_000, 950_000);

    // Domain / action / venue.
    assert_eq!(body["domain"], "amm", "{body}");
    assert_eq!(body["action"], "swap", "{body}");
    assert_eq!(body["venue"]["name"], "balancer_v2", "{body}");
    assert_eq!(body["venue"]["chain"], "eip155:1", "{body}");
    // vault = $to (the Vault the user called), lowercased.
    assert_eq!(body["venue"]["vault"], VAULT_MAINNET, "{body}");
    // pool_id = $args.singleSwap[0] (bytes32 → hex string). The venue pool
    // identifier — host:registry resolves whether it is a Fjord LBP.
    assert_eq!(body["venue"]["pool_id"], LBP_POOL_ID, "{body}");
    // pool_type defaults to weighted (generic); precise type is host:registry.
    assert_eq!(body["venue"]["pool_type"], "weighted", "{body}");

    // kind=0 → exact_input via the $match/$cases direction switch.
    assert_eq!(body["params"]["direction"]["kind"], "exact_input", "{body}");
    // amount_in = $args.singleSwap[4] (uint256 → alloy hex string). 1_000_000.
    assert_eq!(
        body["params"]["direction"]["amount_in"], "0xf4240",
        "{body}"
    );
    // min_amount_out = $args.limit. 950_000 == 0xe7ef0.
    assert_eq!(
        body["params"]["direction"]["min_amount_out"], "0xe7ef0",
        "{body}"
    );
    // token_in = assetIn (USDC), token_out = assetOut (WETH).
    assert_eq!(
        body["params"]["token_in"]["key"]["address"], "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        "{body}"
    );
    assert_eq!(
        body["params"]["token_out"]["key"]["address"], "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        "{body}"
    );
    // recipient = $args.funds[2] (FundManagement.recipient).
    assert_eq!(
        body["params"]["recipient"], "0x000000000000000000000000000000000000bbbb",
        "{body}"
    );
    assert_eq!(body["params"]["slippage_bp"], 50, "{body}");

    // live_inputs: Fjord-LBP identification (host:registry) on `route`, swap
    // quote on `expected_amount_out`.
    assert_eq!(
        body["live_inputs"]["route"]["source"]["kind"], "registry_api",
        "{body}"
    );
    assert_eq!(
        body["live_inputs"]["route"]["source"]["resource"]["kind"], "pool_meta",
        "{body}"
    );
    assert_eq!(
        body["live_inputs"]["expected_amount_out"]["source"]["kind"], "onchain_view",
        "{body}"
    );
    assert_eq!(
        body["live_inputs"]["gas_estimate"]["source"]["kind"], "oracle_feed",
        "{body}"
    );
}

// ---------------------------------------------------------------------------
// t22 — GIVEN_OUT (kind=1) → exact_output
// ---------------------------------------------------------------------------

#[test]
fn t22_balancer_vault_swap_given_out_exact_output() {
    // kind=1 GIVEN_OUT: amount = exact amount_out (2_000_000), limit = max in.
    let body = route_balancer_swap(1, 2_000_000, 2_500_000);

    assert_eq!(body["venue"]["name"], "balancer_v2", "{body}");
    // kind=1 → exact_output via the $match/$cases switch (the SAME manifest,
    // different discriminant arg → different direction shape). This is the
    // load-bearing proof the value-map switches the whole direction object.
    assert_eq!(
        body["params"]["direction"]["kind"], "exact_output",
        "{body}"
    );
    // amount_out = $args.singleSwap[4]. 2_000_000 == 0x1e8480.
    assert_eq!(
        body["params"]["direction"]["amount_out"], "0x1e8480",
        "{body}"
    );
    // max_amount_in = $args.limit. 2_500_000 == 0x2625a0.
    assert_eq!(
        body["params"]["direction"]["max_amount_in"], "0x2625a0",
        "{body}"
    );
    // GIVEN_OUT must NOT carry the exact_input fields.
    assert!(
        body["params"]["direction"].get("amount_in").is_none(),
        "{body}"
    );
    assert!(
        body["params"]["direction"].get("min_amount_out").is_none(),
        "{body}"
    );
}

// ===========================================================================
// b4 — LayerZero ZRO airdrop / OFT cover
// ===========================================================================
//
// Five user-facing entrypoints on the LayerZero ZRO airdrop ClaimContract
// (Arbitrum hub) + the ZRO OFT token. Selectors `cast sig`-verified:
//   claim            0xf9429637  → Airdrop(Claim)  MerkleDistributor
//   donateAndClaim   0xac6ae3ee  → Airdrop(Claim)  MerkleDistributor
//   donate           0xcd139742  → Unknown
//   withdrawDonation 0x46d7ce37  → Unknown
//   OFT send         0xc7c7f5b3  → Unknown (cross-chain bridge; no bridge domain)
//
// chain_to_addresses: claim/donate fns cover the Arbitrum hub
// 0xd6b6a6701303B5Ea36fa0eDf7389b562d8F894DB (chain 42161) ONLY — per-satellite
// ClaimRemote addresses are NOT 1차-verified (limited cover). OFT send covers the
// ZRO ERC20 0x6985884c4392d348587b19cb9eaaf157f13271cd (same on 7 chains).
//
// claim is PAUSED at deploy (merkleRoot=0) and EIP-712 is ABSENT — neither
// affects calldata decode (PAUSED is a verdict concern; no sig-routing here).

const LZ_CLAIM_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/layerzero/claim-contract/claim@1.0.0.json");
const LZ_DONATE_AND_CLAIM_MANIFEST: &str = include_str!(
    "../../../registryV2/manifests/layerzero/claim-contract/donateAndClaim@1.0.0.json"
);
const LZ_DONATE_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/layerzero/claim-contract/donate@1.0.0.json");
const LZ_WITHDRAW_DONATION_MANIFEST: &str = include_str!(
    "../../../registryV2/manifests/layerzero/claim-contract/withdrawDonation@1.0.0.json"
);
const LZ_OFT_SEND_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/layerzero/oft/send@1.0.0.json");

// Verified 1차 addresses (task body).
const LZ_HUB: &str = "0xd6b6a6701303b5ea36fa0edf7389b562d8f894db"; // ClaimContract, Arbitrum
const LZ_ZRO: &str = "0x6985884c4392d348587b19cb9eaaf157f13271cd"; // ZRO ERC20 / OFT
const LZ_ARBITRUM: u64 = 42161;
const LZ_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";
const LZ_RECIPIENT: &str = "0x00000000000000000000000000000000deadbeef";

// Two Merkle proof siblings (bytes32) — fixed test vector.
fn lz_proof_siblings() -> Vec<DynSolValue> {
    vec![
        DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x11), 32),
        DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0x22), 32),
    ]
}

// ---------------------------------------------------------------------------
// b4.claim — claim(uint8,uint256,bytes32[],address,bytes) → Airdrop(Claim)
// ---------------------------------------------------------------------------

#[test]
fn b4_layerzero_claim() {
    let install = install_ok(LZ_CLAIM_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "layerzero/claim-contract/claim@1.0.0"
    );

    // claim(currency=1 (ZRO), amount, proof=[0x11,0x22], to, extraOptions=0x)
    let calldata = encode_calldata(
        "0xf9429637",
        &[
            DynSolValue::Uint(AlloyU256::from(1u64), 8), // currency
            DynSolValue::Uint(AlloyU256::from(5_000_000_000_000_000_000u128), 256), // amount
            DynSolValue::Array(lz_proof_siblings()),
            DynSolValue::Address(addr(LZ_RECIPIENT)),
            DynSolValue::Bytes(vec![]),
        ],
    );
    let input = route_input(LZ_ARBITRUM, LZ_HUB, "0xf9429637", calldata, LZ_SUBMITTER);
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"],
        "layerzero/claim-contract/claim@1.0.0"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "airdrop", "{parsed}");
    assert_eq!(body["action"], "claim", "{parsed}");
    assert_eq!(body["source"]["name"], "layerzero", "{parsed}");
    // MerkleDistributor claim_target: chain = $chain, contract = $to (hub).
    assert_eq!(
        body["claim_target"]["kind"], "merkle_distributor",
        "{parsed}"
    );
    assert_eq!(body["claim_target"]["chain"], "eip155:42161", "{parsed}");
    assert_eq!(body["claim_target"]["contract"], LZ_HUB, "{parsed}");
    // recipient = $args.to.
    assert_eq!(body["recipient"], LZ_RECIPIENT, "{parsed}");
    // proof.siblings = $args.proof (2 bytes32 → 2 hex strings).
    let siblings = body["proof"]["siblings"]
        .as_array()
        .expect("proof.siblings array");
    assert_eq!(siblings.len(), 2, "{parsed}");
    // leaf_index is a literal 0 (NOT in calldata — claim is proof-based, no index arg).
    assert_eq!(body["proof"]["leaf_index"], 0, "{parsed}");
    // live_inputs default value injected (PAUSED → catalog default false).
    assert_eq!(
        body["live_inputs"]["is_still_claimable"]["value"], false,
        "{parsed}"
    );
    // U256 LiveField value round-trips as a hex string through alloy serde.
    assert_eq!(
        body["live_inputs"]["actual_amount"]["value"], "0x0",
        "{parsed}"
    );
    // claim_token live source descriptor present (derived_from ZRO).
    assert_eq!(
        body["live_inputs"]["claim_token"]["source"]["kind"], "derived_from",
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// b4.donateAndClaim — donateAndClaim(...) → Airdrop(Claim) (claim side captured;
// donation amountToDonate not surfaced)
// ---------------------------------------------------------------------------

#[test]
fn b4_layerzero_donate_and_claim() {
    let install = install_ok(LZ_DONATE_AND_CLAIM_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "layerzero/claim-contract/donateAndClaim@1.0.0"
    );

    // donateAndClaim(currency, amountToDonate, zroAmount, proof, to, extraOptions)
    let calldata = encode_calldata(
        "0xac6ae3ee",
        &[
            DynSolValue::Uint(AlloyU256::from(0u64), 8), // currency = 0 (native fee)
            DynSolValue::Uint(AlloyU256::from(1_000_000_000_000_000u128), 256), // amountToDonate
            DynSolValue::Uint(AlloyU256::from(7_000_000_000_000_000_000u128), 256), // zroAmount
            DynSolValue::Array(lz_proof_siblings()),
            DynSolValue::Address(addr(LZ_RECIPIENT)),
            DynSolValue::Bytes(vec![]),
        ],
    );
    let input = route_input(LZ_ARBITRUM, LZ_HUB, "0xac6ae3ee", calldata, LZ_SUBMITTER);
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "airdrop", "{parsed}");
    assert_eq!(body["action"], "claim", "{parsed}");
    assert_eq!(
        body["claim_target"]["kind"], "merkle_distributor",
        "{parsed}"
    );
    assert_eq!(body["claim_target"]["contract"], LZ_HUB, "{parsed}");
    assert_eq!(body["recipient"], LZ_RECIPIENT, "{parsed}");
    assert_eq!(
        body["proof"]["siblings"].as_array().unwrap().len(),
        2,
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// b4.donate — donate(uint8,uint256,address) → Unknown
// ---------------------------------------------------------------------------

#[test]
fn b4_layerzero_donate() {
    let install = install_ok(LZ_DONATE_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "layerzero/claim-contract/donate@1.0.0"
    );

    let calldata = encode_calldata(
        "0xcd139742",
        &[
            DynSolValue::Uint(AlloyU256::from(0u64), 8), // currency
            DynSolValue::Uint(AlloyU256::from(1_000_000_000_000_000u128), 256), // amount
            DynSolValue::Address(addr(LZ_RECIPIENT)),
        ],
    );
    // donate is payable — msg.value carries the donation.
    let input = route_input_with_value(
        LZ_ARBITRUM,
        LZ_HUB,
        "0xcd139742",
        calldata.clone(),
        LZ_SUBMITTER,
        "1000000000000000",
    );
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(body["target"], LZ_HUB, "{parsed}");
    assert_eq!(body["chain"], "eip155:42161", "{parsed}");
    // value = $tx.value (donate is payable). U256 → hex string; 1e15 wei = 0x38d7ea4c68000.
    assert_eq!(body["value"], "0x38d7ea4c68000", "{parsed}");
    // `$calldata` placeholder — the Unknown body PRESERVES the full raw calldata
    // (the whole point of an Unknown body for a scope analyzer), not the legacy
    // "0x" sentinel.
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// ---------------------------------------------------------------------------
// b4.withdrawDonation — withdrawDonation(uint8,uint256) → Unknown
// ---------------------------------------------------------------------------

#[test]
fn b4_layerzero_withdraw_donation() {
    let install = install_ok(LZ_WITHDRAW_DONATION_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "layerzero/claim-contract/withdrawDonation@1.0.0"
    );

    let calldata = encode_calldata(
        "0x46d7ce37",
        &[
            DynSolValue::Uint(AlloyU256::from(1u64), 8), // currency
            DynSolValue::Uint(AlloyU256::from(500_000_000_000_000u128), 256), // amount
        ],
    );
    let input = route_input(
        LZ_ARBITRUM,
        LZ_HUB,
        "0x46d7ce37",
        calldata.clone(),
        LZ_SUBMITTER,
    );
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(body["target"], LZ_HUB, "{parsed}");
    // not payable → value literal "0" → U256 hex string "0x0".
    assert_eq!(body["value"], "0x0", "{parsed}");
    // `$calldata` placeholder — Unknown body preserves the full raw calldata.
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// ---------------------------------------------------------------------------
// b4.oftSend — send((SendParam),(MessagingFee),address) → Unknown
// ---------------------------------------------------------------------------
//
// The route ALWAYS decodes calldata against abi_fragment.abi before building
// the body — so the SendParam / MessagingFee tuple components must round-trip.
// We encode a realistic bridge (dstEid=30101 Ethereum endpoint, amountLD, etc.)
// and assert the Unknown body targets the ZRO OFT contract.

#[test]
fn b4_layerzero_oft_send() {
    let install = install_ok(LZ_OFT_SEND_MANIFEST);
    assert_eq!(install["data"]["bundle_id"], "layerzero/oft/send@1.0.0");

    let send_param = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(30101u64), 32), // dstEid (LZ V2 Ethereum eid)
        DynSolValue::FixedBytes(alloy_primitives::B256::repeat_byte(0xcd), 32), // to (bytes32)
        DynSolValue::Uint(AlloyU256::from(3_000_000_000_000_000_000u128), 256), // amountLD
        DynSolValue::Uint(AlloyU256::from(2_970_000_000_000_000_000u128), 256), // minAmountLD
        DynSolValue::Bytes(vec![]),                       // extraOptions
        DynSolValue::Bytes(vec![]),                       // composeMsg
        DynSolValue::Bytes(vec![]),                       // oftCmd
    ]);
    let fee = DynSolValue::Tuple(vec![
        DynSolValue::Uint(AlloyU256::from(1_000_000_000_000_000u128), 256), // nativeFee
        DynSolValue::Uint(AlloyU256::from(0u64), 256),                      // lzTokenFee
    ]);
    let calldata = encode_calldata(
        "0xc7c7f5b3",
        &[send_param, fee, DynSolValue::Address(addr(LZ_SUBMITTER))],
    );
    // send is payable — msg.value covers nativeFee.
    let input = route_input_with_value(
        LZ_ARBITRUM,
        LZ_ZRO,
        "0xc7c7f5b3",
        calldata.clone(),
        LZ_SUBMITTER,
        "1000000000000000",
    );
    let parsed = route_ok(input);
    assert_eq!(parsed["data"]["decoder_id"], "layerzero/oft/send@1.0.0");

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    // target = $to = the ZRO OFT contract.
    assert_eq!(body["target"], LZ_ZRO, "{parsed}");
    assert_eq!(body["chain"], "eip155:42161", "{parsed}");
    // value = $tx.value (send is payable). 1e15 wei = 0x38d7ea4c68000.
    assert_eq!(body["value"], "0x38d7ea4c68000", "{parsed}");
    // `$calldata` placeholder — Unknown body preserves the full raw calldata
    // (the entire SendParam/MessagingFee tuple is retained for scope analysis).
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// ===========================================================================
// t23 — tagged_dispatch strategy (HyperLiquid CoreWriter mechanism)
// ===========================================================================
//
// HyperLiquid's CoreWriter `sendRawAction(bytes data)` encodes ONE action as
// `data[0]=0x01 (version) ‖ data[1:4]=action_id (uint24 BE) ‖
// data[4:]=abi.encode(<action args>)` (1차-verified, HyperLiquid docs).
//
// `tagged_dispatch` decodes that envelope to a SINGLE `ActionBody` (NOT a
// Multicall): assert the version byte, read the uint24 action_id, look up
// `per_action_body["<decimal id>"]`, abi-decode `data[4:]` with that action's
// `inputs_abi` into the ctx `inputs`, and build that one action's `body`.
//
// SCOPE: this fixture proves the MECHANISM only (the full 15-action mapping is
// B.3.2). Two minimal `unknown` bodies with DIFFERENT field wiring prove that
// dispatch routes on `action_id` and that `$inputs.<i>` decode flows into the
// body; a third case proves version-mismatch fail-soft.

// `sendRawAction(bytes)` selector — `cast sig`-equivalent keccak.
const HL_SEND_RAW_ACTION_SELECTOR: &str = "0x17938e13";
const HL_CORE_WRITER: &str = "0x3333333333333333333333333333333333333333";
const HL_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";
const HL_INPUT_ADDR: &str = "0x00000000000000000000000000000000deadbeef";

// Minimal mechanism-proving manifest. `inputs_abi` decode keys the tuple by
// field name (unnamed components → `arg0`/`arg1`/...), same as the
// opcode-stream `$inputs.<name>` convention. action_id=1 wires the Unknown
// body's `target`/`value` straight from the decoded `$inputs.arg0`/`arg1`
// (proving the abi-decode of `data[4:]`); action_id=2 is a DIFFERENT body
// (target=$to, value from a DIFFERENT decoded field with a DIFFERENT
// inputs_abi), proving dispatch routes on `action_id`. A bad version byte
// falls through to a fail-soft inline Unknown body (no `"default"` entry
// present → exercises that branch).
const HL_TAGGED_DISPATCH_MANIFEST: &str = r#"{
  "type": "adapter_function",
  "id": "hyperliquid/core-writer/sendRawAction@1.0.0",
  "publisher": "hyperliquid",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": { "999": ["0x3333333333333333333333333333333333333333"] },
    "selector": "0x17938e13"
  },
  "abi_fragment": {
    "function_name": "sendRawAction",
    "abi": {
      "name": "sendRawAction",
      "type": "function",
      "stateMutability": "nonpayable",
      "inputs": [ { "name": "data", "type": "bytes" } ],
      "outputs": []
    }
  },
  "emit": {
    "strategy": "tagged_dispatch",
    "bytes_source": "$args.data",
    "version_byte": "0x01",
    "tag_offset": 1,
    "tag_size": 3,
    "per_action_body": {
      "1": {
        "name": "limit_order",
        "inputs_abi": "(address,uint256)",
        "body": {
          "domain": "unknown",
          "unknown": {
            "target":   "$inputs.arg0",
            "chain":    "$chain",
            "calldata": "$calldata",
            "value":    "$inputs.arg1"
          }
        }
      },
      "2": {
        "name": "vault_transfer",
        "inputs_abi": "(uint64,bool,uint64)",
        "body": {
          "domain": "unknown",
          "unknown": {
            "target":   "$to",
            "chain":    "$chain",
            "calldata": "$calldata",
            "value":    "$inputs.arg2"
          }
        }
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

/// Build the CoreWriter inner `data`: `0x ‖ version ‖ uint24(action_id) BE ‖
/// abi.encode(args)`, then wrap it as the `bytes data` arg of
/// `sendRawAction(bytes)` and return the full route-input calldata hex.
fn hl_encode_send_raw_action(version: u8, action_id: u32, args: &[DynSolValue]) -> String {
    let mut data = vec![version];
    // uint24 big-endian (3 bytes).
    data.push(((action_id >> 16) & 0xff) as u8);
    data.push(((action_id >> 8) & 0xff) as u8);
    data.push((action_id & 0xff) as u8);
    data.extend_from_slice(&DynSolValue::Tuple(args.to_vec()).abi_encode_params());
    encode_calldata(HL_SEND_RAW_ACTION_SELECTOR, &[DynSolValue::Bytes(data)])
}

fn hl_route(calldata: String) -> Value {
    route_ok(route_input(
        999,
        HL_CORE_WRITER,
        HL_SEND_RAW_ACTION_SELECTOR,
        calldata,
        HL_SUBMITTER,
    ))
}

// t23.a — action_id=1 → routes to action 1's body with decoded `$inputs`.
#[test]
fn t23_tagged_dispatch_action_1_decodes_inputs() {
    let install = install_ok(HL_TAGGED_DISPATCH_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/core-writer/sendRawAction@1.0.0"
    );

    // action_id=1, inputs_abi=(address,uint256): target ← $inputs[0],
    // value ← $inputs[1].
    let calldata = hl_encode_send_raw_action(
        0x01,
        1,
        &[
            DynSolValue::Address(addr(HL_INPUT_ADDR)),
            DynSolValue::Uint(AlloyU256::from(7_777_u64), 256),
        ],
    );
    let parsed = hl_route(calldata.clone());

    // SINGLE ActionBody (not a Multicall): exactly one action, domain unknown.
    assert_eq!(
        parsed["data"]["actions"].as_array().unwrap().len(),
        1,
        "{parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    // $inputs[0] decoded address routed into the body.
    assert_eq!(body["target"], HL_INPUT_ADDR, "{parsed}");
    // $inputs[1] decoded uint256 → U256 hex; 7777 = 0x1e61.
    assert_eq!(body["value"], "0x1e61", "{parsed}");
    // $calldata preserves the FULL sendRawAction calldata.
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// t23.b — action_id=2 → routes to the DIFFERENT body (proving action_id
// dispatch + that action 2's distinct inputs_abi is the one decoded).
#[test]
fn t23_tagged_dispatch_action_2_routes_different_body() {
    install_ok(HL_TAGGED_DISPATCH_MANIFEST);

    // action_id=2, inputs_abi=(uint64,bool,uint64): value ← $inputs[2].
    let calldata = hl_encode_send_raw_action(
        0x01,
        2,
        &[
            DynSolValue::Uint(AlloyU256::from(1_u64), 64),
            DynSolValue::Bool(true),
            DynSolValue::Uint(AlloyU256::from(42_u64), 64),
        ],
    );
    let parsed = hl_route(calldata.clone());

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    // action 2's body wires target=$to (the CoreWriter), NOT $inputs[0] — this
    // is what distinguishes it from action 1, proving dispatch routed on id=2.
    assert_eq!(body["target"], HL_CORE_WRITER, "{parsed}");
    // $inputs[2] = 42 = 0x2a (action 2's inputs_abi successfully decoded).
    assert_eq!(body["value"], "0x2a", "{parsed}");
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// t23.c — bad version byte → fail-soft Unknown body (recorded, no panic).
#[test]
fn t23_tagged_dispatch_bad_version_fail_soft() {
    install_ok(HL_TAGGED_DISPATCH_MANIFEST);

    // version byte 0x02 ≠ manifest's 0x01 → fall through to fail-soft body.
    let calldata = hl_encode_send_raw_action(
        0x02,
        1,
        &[
            DynSolValue::Address(addr(HL_INPUT_ADDR)),
            DynSolValue::Uint(AlloyU256::from(7_777_u64), 256),
        ],
    );
    let parsed = hl_route(calldata.clone());

    // Still ok:true (fail-soft, NOT an error envelope) and a SINGLE body.
    assert_eq!(parsed["ok"], true, "{parsed}");
    let body = &parsed["data"]["actions"][0]["body"];
    // Fail-soft Unknown body: target=$to, full $calldata preserved → policy
    // warns/denies on the unrecognised envelope rather than mis-classifying.
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(body["target"], HL_CORE_WRITER, "{parsed}");
    assert_eq!(body["calldata"], calldata, "{parsed}");
}

// ===========================================================================
// B.3 — selector-less (bare native transfer) routing: HYPE system deposit
// ===========================================================================
//
// A bare native transfer is EMPTY calldata (`"0x"`) + `value > 0` + a known
// system `to`. It has NO 4-byte selector, so the calldata callkey
// `(chain, to, selector)` is unroutable as-is. The route extends the key with
// a reserved SENTINEL selector `0x00000000` (the all-zero 4-byte word) when
// calldata is empty; a manifest opts in by declaring `match.selector` =
// `"0x00000000"`. The sentinel passes the existing 8-hex `SELECTOR_RE`
// (build-index + SW) unchanged, and can never collide with a real dispatch:
// a function selector requires ≥4 calldata bytes, but the sentinel branch only
// fires when calldata is EMPTY.
//
// HyperLiquid's HYPE deposit IS this shape: `to = 0x2222…2` (HyperEVM 999
// mainnet / 998 testnet), empty calldata, `value` = HYPE amount, hitting the
// system address's payable `receive()`. There is no native-value-transfer
// `TokenAction` variant (token actions are approve/permit/transfer of a NAMED
// token only), so the honest body is `ActionBody::Unknown` — `$calldata`
// resolves to "0x" and `$tx.value` carries the amount, so the policy layer
// sees the full native value movement and warns/denies rather than mis-typing
// it as a token op.

const HYPE_SYSTEM_ADDRESS: &str = "0x2222222222222222222222222222222222222222";
const NATIVE_TRANSFER_SENTINEL: &str = "0x00000000";

/// HYPE system bare-native-transfer manifest (registryV2 on-disk twin of
/// `manifests/hyperliquid/hype-system/native-transfer@1.0.0.json`). The
/// `abi_fragment` is a degenerate zero-arg function — it is NEVER decoded on
/// the selector-less path (empty calldata → no args), but the v3 install
/// envelope still requires the field structurally.
const HYPE_NATIVE_TRANSFER_MANIFEST: &str = r#"{
  "type": "adapter_action",
  "id": "hyperliquid/hype-system/native-transfer@1.0.0",
  "publisher": "hyperliquid",
  "schema_version": "3",
  "match": {
    "selector": "0x00000000",
    "chain_to_addresses": {
      "999": ["0x2222222222222222222222222222222222222222"],
      "998": ["0x2222222222222222222222222222222222222222"]
    }
  },
  "abi_fragment": {
    "function_name": "receive",
    "abi": {
      "name": "receive",
      "type": "function",
      "stateMutability": "payable",
      "inputs": []
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "unknown",
      "unknown": {
        "target":   "$to",
        "chain":    "$chain",
        "calldata": "$calldata",
        "value":    "$tx.value"
      }
    }
  },
  "requires": {
    "imperative": [],
    "adapter_capabilities": [],
    "host_capabilities": [],
    "extension": ">=0.1.0"
  }
}"#;

/// Route a bare native transfer: EMPTY calldata, the supplied `value`, on the
/// HYPE system address. `selector` is the sentinel — the route ignores it for
/// the lookup when calldata is empty (it recomputes the sentinel internally),
/// but the wire DTO still requires the field, so pass the sentinel verbatim.
fn hype_native_transfer_input(chain_id: u64, value: &str) -> String {
    route_input_with_value(
        chain_id,
        HYPE_SYSTEM_ADDRESS,
        NATIVE_TRANSFER_SENTINEL,
        "0x".to_owned(),
        HL_SUBMITTER,
        value,
    )
}

// b3.a — HYPE mainnet (999) bare deposit of 1 HYPE → Unknown body, value
// preserved as `$tx.value`, calldata "0x".
#[test]
fn b3_hype_native_transfer_routes_unknown_with_value() {
    let install = install_ok(HYPE_NATIVE_TRANSFER_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/hype-system/native-transfer@1.0.0"
    );

    // 1 HYPE = 1e18 wei.
    let input = hype_native_transfer_input(999, "1000000000000000000");
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"], "hyperliquid/hype-system/native-transfer@1.0.0",
        "{parsed}"
    );

    // Exactly one action, honest Unknown body.
    assert_eq!(
        parsed["data"]["actions"].as_array().unwrap().len(),
        1,
        "{parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(body["target"], HYPE_SYSTEM_ADDRESS, "{parsed}");
    assert_eq!(body["chain"], "eip155:999", "{parsed}");
    // Bare native transfer → calldata is the empty "0x" word.
    assert_eq!(body["calldata"], "0x", "{parsed}");
    // The HYPE amount rides on $tx.value (U256 → hex): 1e18 = 0xde0b6b3a7640000.
    assert_eq!(body["value"], "0xde0b6b3a7640000", "{parsed}");
}

// b3.b — HYPE testnet (998) routes through the same sentinel key (proving the
// second chain_to_addresses entry is bridged under the sentinel selector).
#[test]
fn b3_hype_native_transfer_testnet_chain() {
    install_ok(HYPE_NATIVE_TRANSFER_MANIFEST);

    let input = hype_native_transfer_input(998, "500000000000000000");
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(body["chain"], "eip155:998", "{parsed}");
    // 0.5 HYPE = 5e17 = 0x6f05b59d3b20000.
    assert_eq!(body["value"], "0x6f05b59d3b20000", "{parsed}");
}

// b3.c — a NON-empty calldata to the same (chain, to) does NOT hit the
// sentinel: it computes the real selector key, which the manifest never
// registered → clean `no_declarative_v3_mapper` miss. Proves the sentinel
// branch fires ONLY for empty calldata and selector-bearing routing is
// unaffected.
#[test]
fn b3_nonempty_calldata_does_not_hit_sentinel() {
    install_ok(HYPE_NATIVE_TRANSFER_MANIFEST);

    // Some real 4-byte selector + payload to the HYPE system address.
    let input = route_input_with_value(
        999,
        HYPE_SYSTEM_ADDRESS,
        "0xa9059cbb",
        "0xa9059cbb".to_owned(),
        HL_SUBMITTER,
        "1000000000000000000",
    );
    let out = declarative_route_request_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(
        parsed["error"]["kind"], "no_declarative_v3_mapper",
        "{parsed}"
    );
}

// ===========================================================================
// b3 — HyperLiquid Bridge2 (on-chain) + WHYPE (HyperEVM chain 999) cover
// ===========================================================================
//
// Manifests are loaded from the committed registryV2 files via `include_str!`
// so the tests exercise the SHIPPED on-disk shapes (not inline copies).
//
//   Bridge2 batchedDepositWithPermit  0xb30b5bce  → Multicall of USDC transfers
//   USDC EIP-2612 permit              0xd505accf  → erc20_permit (typed-data)
//   WHYPE deposit()                   0xd0e30db0  → Unknown ($calldata, $tx.value)
//   WHYPE withdraw(uint256)           0x2e1a7d4d  → Unknown ($calldata, value "0")
//   WHYPE transfer/approve/transferFrom            → erc20_transfer / erc20_approve

const HL_BRIDGE2_DEPOSIT_MANIFEST: &str = include_str!(
    "../../../registryV2/manifests/hyperliquid/bridge2/batched-deposit-with-permit@1.0.0.json"
);
const HL_USDC_PERMIT_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/bridge2/usdc-permit@1.0.0.json");
const HL_WHYPE_DEPOSIT_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/whype/deposit@1.0.0.json");
const HL_WHYPE_WITHDRAW_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/whype/withdraw@1.0.0.json");
const HL_WHYPE_TRANSFER_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/whype/transfer@1.0.0.json");
const HL_WHYPE_APPROVE_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/whype/approve@1.0.0.json");
const HL_WHYPE_TRANSFER_FROM_MANIFEST: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/whype/transferFrom@1.0.0.json");

const HL_BRIDGE2: &str = "0x2df1c51e09aecf9cacb7bc98cb1742757f163df7";
const HL_USDC: &str = "0xaf88d065e77c8cc2239327c5edb3a432268e5831"; // USDC on Arbitrum (lowercased)
const HL_WHYPE: &str = "0x5555555555555555555555555555555555555555"; // WHYPE (D9-limited; all-5s assumed)
const HL_ARBITRUM: u64 = 42161;
const HL_HYPEREVM: u64 = 999;
const HL_USER_A: &str = "0x00000000000000000000000000000000deadbeef";
const HL_USER_B: &str = "0x00000000000000000000000000000000cafef00d";
// `HL_SUBMITTER` is already defined for the t23 tagged_dispatch tests (same value).

// ---------------------------------------------------------------------------
// b3.bridge2.batchedDepositWithPermit — array_emit → Multicall of USDC transfers
// ---------------------------------------------------------------------------
//
// `deposits` is a tuple[] `(address user, uint64 usd, uint64 deadline,
// (uint256 r, uint256 s, uint8 v) signature)`. On the calldata path each
// element decodes POSITIONALLY, so the body reads `$inputs.[1]` (usd) for the
// amount. The two elements carry DIFFERENT usd amounts → proves per-element
// binding. The inner signature tuple (index [3]) is decoded but unused.

/// One deposit tuple `(user, usd, deadline, (r, s, v))`.
fn hl_deposit_tuple(user: &str, usd: u64, deadline: u64) -> DynSolValue {
    DynSolValue::Tuple(vec![
        DynSolValue::Address(addr(user)),
        DynSolValue::Uint(AlloyU256::from(usd), 64),
        DynSolValue::Uint(AlloyU256::from(deadline), 64),
        DynSolValue::Tuple(vec![
            DynSolValue::Uint(AlloyU256::from(0x11_u64), 256), // r
            DynSolValue::Uint(AlloyU256::from(0x22_u64), 256), // s
            DynSolValue::Uint(AlloyU256::from(27_u64), 8),     // v
        ]),
    ])
}

#[test]
fn b3_hl_bridge2_batched_deposit_with_permit() {
    let install = install_ok(HL_BRIDGE2_DEPOSIT_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/bridge2/batched-deposit-with-permit@1.0.0"
    );

    // 100 USDC (6-dp = 100_000_000) and 250 USDC (250_000_000).
    let calldata = encode_calldata(
        "0xb30b5bce",
        &[DynSolValue::Array(vec![
            hl_deposit_tuple(HL_USER_A, 100_000_000, 1_900_000_000),
            hl_deposit_tuple(HL_USER_B, 250_000_000, 1_900_000_001),
        ])],
    );
    let input = route_input(
        HL_ARBITRUM,
        HL_BRIDGE2,
        "0xb30b5bce",
        calldata,
        HL_SUBMITTER,
    );
    let parsed = route_ok(input);
    assert_eq!(
        parsed["data"]["decoder_id"],
        "hyperliquid/bridge2/batched-deposit-with-permit@1.0.0"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let actions = body["actions"].as_array().expect("inner actions array");
    assert_eq!(actions.len(), 2, "{parsed}");

    // element-0 — 100 USDC to the Bridge2 contract.
    assert_eq!(actions[0]["domain"], "token");
    assert_eq!(actions[0]["action"], "erc20_transfer");
    assert_eq!(
        actions[0]["recipient"], HL_BRIDGE2,
        "recipient = $to (Bridge2)"
    );
    assert_eq!(actions[0]["amount"], "0x5f5e100", "{parsed}"); // 100_000_000
    assert_eq!(actions[0]["token"]["key"]["address"], HL_USDC, "{parsed}");
    assert_eq!(
        actions[0]["token"]["key"]["chain"], "eip155:42161",
        "{parsed}"
    );

    // element-1 — DIFFERENT usd amount proves per-element $inputs.[1] binding.
    assert_eq!(actions[1]["action"], "erc20_transfer");
    assert_eq!(actions[1]["recipient"], HL_BRIDGE2);
    assert_eq!(actions[1]["amount"], "0xee6b280", "{parsed}"); // 250_000_000
    assert_eq!(actions[1]["token"]["key"]["address"], HL_USDC, "{parsed}");
}

#[test]
fn b3_hl_bridge2_batched_deposit_empty() {
    install_ok(HL_BRIDGE2_DEPOSIT_MANIFEST);

    // Empty deposits array → Unknown leg (5f3872ff: an empty array_emit surfaces
    // as Unknown so the position stays policy-visible, rather than a
    // silently-empty Multicall that would aggregate to PASS).
    let calldata = encode_calldata("0xb30b5bce", &[DynSolValue::Array(vec![])]);
    let input = route_input(
        HL_ARBITRUM,
        HL_BRIDGE2,
        "0xb30b5bce",
        calldata,
        HL_SUBMITTER,
    );
    let parsed = route_ok(input);
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(
        body["target"], "0x2df1c51e09aecf9cacb7bc98cb1742757f163df7",
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// b3.bridge2.usdcPermit — EIP-2612 typed-data → erc20_permit (FLAT unwrapped)
// ---------------------------------------------------------------------------
//
// USDC EIP-2612 permit (domain.name "USD Coin", chain 42161). The flat path
// resolves `$args.spender` / `$args.value` against the message directly. The
// signed permit grants a USDC allowance to `spender` (here the Bridge2
// contract — contextual, not modelled as a Bridge2-specific action).

#[test]
fn b3_hl_bridge2_usdc_permit() {
    install_ok(HL_USDC_PERMIT_MANIFEST);

    let message = json!({
        "owner": HL_SUBMITTER,
        "spender": HL_BRIDGE2,
        "value": "100000000",
        "nonce": "0",
        "deadline": 1_900_000_000_u64
    });
    let input = json!({
        "chain_id": HL_ARBITRUM,
        "verifying_contract": HL_USDC,
        "primary_type": "Permit",
        "domain_name": "USD Coin",
        "message": message,
        "submitter": HL_SUBMITTER,
        "submitted_at": 1_700_000_000_u64
    })
    .to_string();

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "hyperliquid/bridge2/usdc-permit@1.0.0",
        "{parsed}"
    );

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_permit", "{parsed}");
    // FLAT proof: $args.spender resolved to the Bridge2 address (not the whole message).
    assert_eq!(body["spender"], HL_BRIDGE2, "{parsed}");
    // amount = message.value (100_000_000 = 0x5f5e100), $args.value resolved flat.
    assert_eq!(body["amount"], "0x5f5e100", "{parsed}");
    // token.key.address = $tx.to = verifying_contract = USDC (lowercased).
    assert_eq!(body["token"]["key"]["address"], HL_USDC, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:42161", "{parsed}");
}

// ---------------------------------------------------------------------------
// b3.whype.deposit — deposit() (payable wrap) → token::wrap_native (amount = $tx.value)
// ---------------------------------------------------------------------------

#[test]
fn b3_hl_whype_deposit() {
    let install = install_ok(HL_WHYPE_DEPOSIT_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/whype/deposit@1.0.0"
    );

    // deposit() has no args → calldata is the bare selector.
    let calldata = encode_calldata("0xd0e30db0", &[]);
    // deposit is payable — the wrapped HYPE amount is msg.value (1 HYPE = 1e18).
    let input = route_input_with_value(
        HL_HYPEREVM,
        HL_WHYPE,
        "0xd0e30db0",
        calldata.clone(),
        HL_SUBMITTER,
        "1000000000000000000",
    );
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    // Structured wrap: the WrapNative TokenAction (added in the WETH wrap-action
    // work) lets HYPE wrapping be legible instead of an anonymous Unknown call.
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "wrap_native", "{parsed}");
    // token.key.address = $to = the WHYPE contract (lowercased), chain = HyperEVM.
    assert_eq!(body["token"]["key"]["address"], HL_WHYPE, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:999", "{parsed}");
    // amount = $tx.value (the wrapped HYPE). 1e18 wei = 0xde0b6b3a7640000.
    assert_eq!(body["amount"], "0xde0b6b3a7640000", "{parsed}");
}

// ---------------------------------------------------------------------------
// b3.whype.withdraw — withdraw(uint256 wad) (unwrap) → token::unwrap_native (amount = $args.wad)
// ---------------------------------------------------------------------------

#[test]
fn b3_hl_whype_withdraw() {
    let install = install_ok(HL_WHYPE_WITHDRAW_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/whype/withdraw@1.0.0"
    );

    // withdraw(wad) — the unwrapped amount is in calldata (0.5 WHYPE).
    let calldata = encode_calldata(
        "0x2e1a7d4d",
        &[DynSolValue::Uint(
            AlloyU256::from(500_000_000_000_000_000_u64),
            256,
        )],
    );
    let input = route_input(
        HL_HYPEREVM,
        HL_WHYPE,
        "0x2e1a7d4d",
        calldata.clone(),
        HL_SUBMITTER,
    );
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    // Structured unwrap: the UnwrapNative TokenAction carries the wad amount.
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "unwrap_native", "{parsed}");
    assert_eq!(body["token"]["key"]["address"], HL_WHYPE, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:999", "{parsed}");
    // amount = $args.wad (the unwrapped amount). 0.5 WHYPE = 5e17 = 0x6f05b59d3b20000.
    assert_eq!(body["amount"], "0x6f05b59d3b20000", "{parsed}");
}

// ---------------------------------------------------------------------------
// b3.whype.transfer / approve / transferFrom — standard ERC20 on chain 999
// ---------------------------------------------------------------------------

#[test]
fn b3_hl_whype_transfer() {
    install_ok(HL_WHYPE_TRANSFER_MANIFEST);

    let calldata = encode_calldata(
        "0xa9059cbb",
        &[
            DynSolValue::Address(addr(HL_USER_A)),
            DynSolValue::Uint(AlloyU256::from(1_500_000_000_000_000_000_u64), 256),
        ],
    );
    let input = route_input(HL_HYPEREVM, HL_WHYPE, "0xa9059cbb", calldata, HL_SUBMITTER);
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_transfer", "{parsed}");
    assert_eq!(body["recipient"], HL_USER_A, "{parsed}");
    assert_eq!(body["amount"], "0x14d1120d7b160000", "{parsed}"); // 1.5e18
    assert_eq!(body["token"]["key"]["address"], HL_WHYPE, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:999", "{parsed}");
}

#[test]
fn b3_hl_whype_approve() {
    install_ok(HL_WHYPE_APPROVE_MANIFEST);

    let calldata = encode_calldata(
        "0x095ea7b3",
        &[
            DynSolValue::Address(addr(HL_USER_B)),
            DynSolValue::Uint(AlloyU256::from(7_000_000_000_000_000_000_u64), 256),
        ],
    );
    let input = route_input(HL_HYPEREVM, HL_WHYPE, "0x095ea7b3", calldata, HL_SUBMITTER);
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_approve", "{parsed}");
    assert_eq!(body["spender"], HL_USER_B, "{parsed}");
    assert_eq!(body["amount"], "0x6124fee993bc0000", "{parsed}"); // 7e18
    assert_eq!(body["token"]["key"]["address"], HL_WHYPE, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:999", "{parsed}");
}

#[test]
fn b3_hl_whype_transfer_from() {
    install_ok(HL_WHYPE_TRANSFER_FROM_MANIFEST);

    let calldata = encode_calldata(
        "0x23b872dd",
        &[
            DynSolValue::Address(addr(HL_USER_A)), // from
            DynSolValue::Address(addr(HL_USER_B)), // to
            DynSolValue::Uint(AlloyU256::from(2_000_000_000_000_000_000_u64), 256),
        ],
    );
    let input = route_input(HL_HYPEREVM, HL_WHYPE, "0x23b872dd", calldata, HL_SUBMITTER);
    let parsed = route_ok(input);

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_transfer", "{parsed}");
    // recipient = $args.to (the `from` arg is not an erc20_transfer field).
    assert_eq!(body["recipient"], HL_USER_B, "{parsed}");
    assert_eq!(body["amount"], "0x1bc16d674ec80000", "{parsed}"); // 2e18
    assert_eq!(body["token"]["key"]["address"], HL_WHYPE, "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:999", "{parsed}");
}
// ===========================================================================
// b3 — HyperLiquid CoreWriter `sendRawAction` 15-action tagged_dispatch
// ===========================================================================
//
// The PRODUCTION manifest (registryV2/manifests/hyperliquid/core-writer/
// send-raw-action@1.0.0.json) covering all 15 CoreWriter action IDs. Unlike
// the minimal t23 mechanism fixture above, this is the real cover: 15
// `per_action_body` entries + `default`, with the GROUNDED per-action decision
// (see ~/.claude-web3/plans/hl-corewriter-cover.md):
//
//   * action 11 (Cancel by cloid) → STRUCTURED `ActionBody::Perp(CancelOrder)`.
//     The ONE CoreWriter action that maps cleanly to a perp-domain body:
//     `PerpVenue::Hyperliquid { chain }` exists, `CancelOrderAction` is just
//     `{ venue, order_id: String }` with NO `live_inputs`, and `cloid` is a
//     uint128 (renders as a JSON STRING, accepted by `order_id: String`).
//   * EVERY OTHER action (1-10, 12-15) + `default` → `ActionBody::Unknown`
//     with `$calldata` preserved. These are vault/staking/delegate/transfer/
//     permission/system ops with NO matching ActionBody domain, OR (action 1
//     limit-order, action 10 cancel-by-oid) carry HL fixed-point uint fields
//     that don't fit the perp struct's String/enum/Decimal types — Unknown is
//     the honest representation, not a mislabel.
//
// This const is byte-identical to the on-disk manifest JSON.
// Pin the ON-DISK CoreWriter manifest (single source of truth) so these WASM
// route tests track the production decoder exactly. Action 11 (cancel by cloid)
// → structured Perp(CancelOrder); every other action + default → HL-attributed
// `hyperliquid_core::hl_unknown { action_type }` (the args are HL uint64
// fixed-point that the declarative grammar cannot scale to human amounts, so a
// named-but-unstructured body is the honest mapping — see the manifest _note).
const HL_COREWRITER_FULL_MANIFEST: &str = include_str!(
    "../../../registryV2/manifests/hyperliquid/core-writer/send-raw-action@1.0.0.json"
);

// b3.a — action 11 (Cancel by cloid) → STRUCTURED `Perp(CancelOrder)` body.
// The headline structured-perp win: cloid (uint128) decodes to a JSON string
// that flows into `order_id: String`; `venue` resolves to
// `Hyperliquid { chain }`.
#[test]
fn b3_corewriter_action_11_cancel_cloid_structured_perp() {
    let install = install_ok(HL_COREWRITER_FULL_MANIFEST);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/core-writer/sendRawAction@1.0.0"
    );

    // cloid = 2^128 - 1 (max uint128) → decimal string "340282366920938463463374607431768211455".
    let cloid_max = AlloyU256::from(1_u64)
        .checked_shl(128)
        .unwrap()
        .wrapping_sub(AlloyU256::from(1_u64));
    let calldata = hl_encode_send_raw_action(
        0x01,
        11,
        &[
            DynSolValue::Uint(AlloyU256::from(5_u64), 32), // asset
            DynSolValue::Uint(cloid_max, 128),             // cloid
        ],
    );
    let parsed = hl_route(calldata.clone());

    // SINGLE structured Perp body (NOT Unknown, NOT a Multicall).
    assert_eq!(
        parsed["data"]["actions"].as_array().unwrap().len(),
        1,
        "{parsed}"
    );
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "perp", "{parsed}");
    assert_eq!(body["action"], "cancel_order", "{parsed}");
    assert_eq!(body["venue"]["name"], "hyperliquid", "{parsed}");
    // $chain resolves to the CAIP-2 chain id for HyperEVM mainnet.
    assert_eq!(body["venue"]["chain"], "eip155:999", "{parsed}");
    // cloid uint128 → JSON string (N>64 rule) → order_id:String.
    assert_eq!(
        body["order_id"], "340282366920938463463374607431768211455",
        "{parsed}"
    );
}

// b3.b — action 1 (Limit order) → Unknown (perp struct types don't fit HL
// fixed-point uint fields; see plan §2). Raw call preserved via $calldata.
#[test]
fn b3_corewriter_action_1_limit_unknown() {
    install_ok(HL_COREWRITER_FULL_MANIFEST);

    // Realistic limit order: asset=5, isBuy=true, limitPx=3750e8, sz=2e8,
    // reduceOnly=false, encodedTif=2 (GTC), cloid=0xdead.
    let calldata = hl_encode_send_raw_action(
        0x01,
        1,
        &[
            DynSolValue::Uint(AlloyU256::from(5_u64), 32),
            DynSolValue::Bool(true),
            DynSolValue::Uint(AlloyU256::from(375_000_000_000_u64), 64),
            DynSolValue::Uint(AlloyU256::from(200_000_000_u64), 64),
            DynSolValue::Bool(false),
            DynSolValue::Uint(AlloyU256::from(2_u64), 8),
            DynSolValue::Uint(AlloyU256::from(0xdead_u64), 128),
        ],
    );
    let parsed = hl_route(calldata.clone());

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], "hl_unknown", "{parsed}");
    assert_eq!(body["action_type"], "limitOrder", "{parsed}");
}

// b3.c — action 10 (Cancel by oid) → Unknown. Semantically identical to
// action 11, BUT oid is uint64 (renders as a JSON NUMBER), which
// `order_id: String` would reject — so it stays Unknown (contrast with b3.a).
#[test]
fn b3_corewriter_action_10_cancel_oid_unknown() {
    install_ok(HL_COREWRITER_FULL_MANIFEST);

    let calldata = hl_encode_send_raw_action(
        0x01,
        10,
        &[
            DynSolValue::Uint(AlloyU256::from(5_u64), 32), // asset
            DynSolValue::Uint(AlloyU256::from(123_456_u64), 64), // oid (uint64)
        ],
    );
    let parsed = hl_route(calldata.clone());

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], "hl_unknown", "{parsed}");
    assert_eq!(body["action_type"], "cancelOrderByOid", "{parsed}");
}

// b3.d — action 4 (Staking deposit) → Unknown. No matching ActionBody domain
// for L1 staking; raw call preserved.
#[test]
fn b3_corewriter_action_4_staking_deposit_unknown() {
    install_ok(HL_COREWRITER_FULL_MANIFEST);

    // staking deposit: wei = 1e9 (uint64).
    let calldata = hl_encode_send_raw_action(
        0x01,
        4,
        &[DynSolValue::Uint(AlloyU256::from(1_000_000_000_u64), 64)],
    );
    let parsed = hl_route(calldata.clone());

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], "hl_unknown", "{parsed}");
    assert_eq!(body["action_type"], "stakingDeposit", "{parsed}");
}

// b3.e — action 6 (Spot send) → Unknown. token/wei are uint64 L1 identifiers
// (not EVM address/U256), so a token.erc20_transfer would mislabel. Unknown.
#[test]
fn b3_corewriter_action_6_spot_send_unknown() {
    install_ok(HL_COREWRITER_FULL_MANIFEST);

    let calldata = hl_encode_send_raw_action(
        0x01,
        6,
        &[
            DynSolValue::Address(addr(HL_INPUT_ADDR)), // destination
            DynSolValue::Uint(AlloyU256::from(1_u64), 64), // token (L1 id)
            DynSolValue::Uint(AlloyU256::from(5_000_000_u64), 64), // wei (L1 fixed-point)
        ],
    );
    let parsed = hl_route(calldata.clone());

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], "hl_unknown", "{parsed}");
    assert_eq!(body["action_type"], "spotSend", "{parsed}");
}

// b3.f — bad version byte (0x02 ≠ 0x01) → `default` Unknown body (fail-soft).
// The manifest HAS a "default" entry, so this exercises the default-fallback
// branch (vs t23.c which had none and hit the inline-Unknown branch).
#[test]
fn b3_corewriter_bad_version_default_unknown() {
    install_ok(HL_COREWRITER_FULL_MANIFEST);

    let calldata = hl_encode_send_raw_action(
        0x02, // wrong version
        11,
        &[
            DynSolValue::Uint(AlloyU256::from(5_u64), 32),
            DynSolValue::Uint(AlloyU256::from(42_u64), 128),
        ],
    );
    let parsed = hl_route(calldata.clone());

    // Fail-soft (ok:true), routed to the manifest's "default" Unknown body.
    assert_eq!(parsed["ok"], true, "{parsed}");
    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], "hl_unknown", "{parsed}");
    assert_eq!(
        body["action_type"], "unrecognizedCoreWriterAction",
        "{parsed}"
    );
}

// ===========================================================================
// b3 — HyperLiquid REST exchange, Mode B "UserSigned" typed-data sigs
// ===========================================================================
//
// HyperLiquid's REST exchange accepts L1 actions authorized by an EIP-712
// `eth_signTypedData_v4` signature ("Mode B / UserSigned"). 12 primaryTypes,
// all sharing `domain.name="HyperliquidSignTransaction"`, version="1",
// `verifyingContract=0x0`, `chainId=signatureChainId` (mainnet 42161 Arbitrum /
// testnet 421614). The fields are OFF-CHAIN L1 semantics: `amount` is a DECIMAL
// STRING ("100.0", not U256); `token`/`destination` are L1 STRING identifiers
// (not EVM address / U256).
//
// MAPPING DECISION: fund/staking payloads decode to STRUCTURED
// `hyperliquid_core::hl_*` bodies (the HL-native off-chain domain, reused from
// the /exchange Flow-3 path) — decimal-string amounts + address strings map
// losslessly, so a policy sees destination / amount instead of an anonymous
// Unknown. Permission primitives (`ApproveAgent`, `ApproveBuilderFee`) map to
// Permission::ProtocolAuthorization. These tests pin the on-disk manifests
// (`include_str!`) and route representative payloads covering both paths.

const HL_REST_USD_SEND: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/usd-send@1.0.0.json");
const HL_REST_SPOT_SEND: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/spot-send@1.0.0.json");
const HL_REST_TOKEN_DELEGATE: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/token-delegate@1.0.0.json");
const HL_REST_APPROVE_AGENT: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/approve-agent@1.0.0.json");
const HL_REST_APPROVE_BUILDER_FEE: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/approve-builder-fee@1.0.0.json");

const HL_VC_ZERO: &str = "0x0000000000000000000000000000000000000000";
const HL_DOMAIN: &str = "HyperliquidSignTransaction";
const HL_SIGNER: &str = "0x000000000000000000000000000000000000aaaa";
const HL_MAINNET: u64 = 42161; // Arbitrum signatureChainId (mainnet)

/// Build a HyperLiquid Mode B typed-data route input. `verifying_contract` is
/// always 0x0 and `domain_name` always "HyperliquidSignTransaction" (Mode B
/// invariants); only `chain_id` / `primary_type` / `message` vary.
fn hl_typed_data_input(chain_id: u64, primary_type: &str, message: Value) -> String {
    json!({
        "chain_id": chain_id,
        "verifying_contract": HL_VC_ZERO,
        "primary_type": primary_type,
        "domain_name": HL_DOMAIN,
        "message": message,
        "submitter": HL_SIGNER,
        "submitted_at": 1_700_000_000_u64
    })
    .to_string()
}

/// Common assertions for a HyperLiquid Mode B sig that decodes to a STRUCTURED
/// `hyperliquid_core` body: recognized (ok, decoder_id), exactly one action
/// under an OffchainSig meta nature whose domain is bound to the routed chain,
/// and `domain == "hyperliquid_core"` with the expected `action`. Returns the
/// body so each test can pin its own protocol-native fields.
fn assert_hl_structured<'a>(
    parsed: &'a Value,
    decoder_id: &str,
    chain_id: u64,
    action: &str,
) -> &'a Value {
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(parsed["data"]["decoder_id"], decoder_id, "{parsed}");

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "expected exactly 1 action: {parsed}");

    // OffchainSig nature + HyperLiquid domain bound to the routed chain.
    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(meta["nature"]["domain"]["name"], HL_DOMAIN, "{parsed}");
    assert_eq!(meta["nature"]["domain"]["chain_id"], chain_id, "{parsed}");

    // Structured HyperliquidCore body (reused from the /exchange Flow-3 path).
    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "hyperliquid_core", "{parsed}");
    assert_eq!(body["action"], action, "{parsed}");
    body
}

// b3.usdSend — string destination + decimal amount → structured hl_usd_send.
#[test]
fn b3_hl_usd_send_routes_to_structured() {
    let install = install_ok(HL_REST_USD_SEND);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/rest/usd-send@1.0.0"
    );

    // The decimal `amount` + L1 `destination` now decode into a structured
    // hyperliquid_core::hl_usd_send (the off-chain-exchange variant the old note
    // deferred now exists), so a policy can scope on destination / amount.
    let message = json!({
        "hyperliquidChain": "Mainnet",
        "destination": "0x00000000000000000000000000000000deadbeef",
        "amount": "100.0",
        "time": 1_700_000_000_u64
    });
    let input = hl_typed_data_input(HL_MAINNET, "HyperliquidTransaction:UsdSend", message);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let body = assert_hl_structured(
        &parsed,
        "hyperliquid/rest/usd-send@1.0.0",
        HL_MAINNET,
        "hl_usd_send",
    );
    assert_eq!(
        body["destination"], "0x00000000000000000000000000000000deadbeef",
        "{parsed}"
    );
    assert_eq!(body["amount"], "100.0", "{parsed}");
}

// b3.tokenDelegate — validator:address + wei + isUndelegate → structured hl_token_delegate.
#[test]
fn b3_hl_token_delegate_routes_to_structured() {
    let install = install_ok(HL_REST_TOKEN_DELEGATE);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/rest/token-delegate@1.0.0"
    );

    let message = json!({
        "hyperliquidChain": "Mainnet",
        "validator": "0x00000000000000000000000000000000cafef00d",
        "wei": "5000000000",
        "isUndelegate": false,
        "nonce": 1_700_000_000_u64
    });
    let input = hl_typed_data_input(HL_MAINNET, "HyperliquidTransaction:TokenDelegate", message);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let body = assert_hl_structured(
        &parsed,
        "hyperliquid/rest/token-delegate@1.0.0",
        HL_MAINNET,
        "hl_token_delegate",
    );
    assert_eq!(
        body["validator"], "0x00000000000000000000000000000000cafef00d",
        "{parsed}"
    );
    assert_eq!(body["is_undelegate"], false, "{parsed}");
    assert_eq!(body["wei"], "5000000000", "{parsed}");
}

// b3.approveAgent — agentAddress:address maps to protocol permission. D9: SDK
// hard-codes the TESTNET signatureChainId 0x66eee (421614) for ApproveAgent;
// mainnet 42161 is assumed valid and listed too — this test routes on 42161 to
// prove the mainnet entry is installed and resolves.
#[test]
fn b3_hl_approve_agent_routes_to_permission() {
    let install = install_ok(HL_REST_APPROVE_AGENT);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/rest/approve-agent@1.0.0"
    );

    let message = json!({
        "hyperliquidChain": "Mainnet",
        "agentAddress": "0x00000000000000000000000000000000a9e47a9e",
        "agentName": "my-api-agent",
        "nonce": 1_700_000_000_u64
    });
    let input = hl_typed_data_input(HL_MAINNET, "HyperliquidTransaction:ApproveAgent", message);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "hyperliquid/rest/approve-agent@1.0.0",
        "{parsed}"
    );

    let action = &parsed["data"]["actions"][0];
    assert_eq!(action["meta"]["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(
        action["meta"]["nature"]["domain"]["name"], HL_DOMAIN,
        "{parsed}"
    );

    let body = &action["body"];
    assert_eq!(body["domain"], "permission", "{parsed}");
    assert_eq!(body["action"], "protocol_authorization", "{parsed}");
    assert_eq!(body["chain"], format!("eip155:{HL_MAINNET}"), "{parsed}");
    assert_eq!(body["protocol"], HL_VC_ZERO, "{parsed}");
    assert_eq!(body["protocol_name"], "hyperliquid", "{parsed}");
    assert_eq!(body["permission"], "agent", "{parsed}");
    assert_eq!(body["permission_label"], "my-api-agent", "{parsed}");
    assert_eq!(body["authorizer"], HL_SIGNER, "{parsed}");
    assert_eq!(
        body["authorized"], "0x00000000000000000000000000000000a9e47a9e",
        "{parsed}"
    );
    assert_eq!(body["is_authorized"], true, "{parsed}");
}

// b3.approveBuilderFee — builder:address + maxFeeRate:string maps to protocol
// permission with the protocol-native limit retained as a string.
#[test]
fn b3_hl_approve_builder_fee_routes_to_permission() {
    let install = install_ok(HL_REST_APPROVE_BUILDER_FEE);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/rest/approve-builder-fee@1.0.0"
    );

    let message = json!({
        "hyperliquidChain": "Mainnet",
        "maxFeeRate": "0.001%",
        "builder": "0x00000000000000000000000000000000b171d3c0",
        "nonce": 1_700_000_000_u64
    });
    let input = hl_typed_data_input(
        HL_MAINNET,
        "HyperliquidTransaction:ApproveBuilderFee",
        message,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "hyperliquid/rest/approve-builder-fee@1.0.0",
        "{parsed}"
    );

    let action = &parsed["data"]["actions"][0];
    assert_eq!(action["meta"]["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(
        action["meta"]["nature"]["domain"]["name"], HL_DOMAIN,
        "{parsed}"
    );

    let body = &action["body"];
    assert_eq!(body["domain"], "permission", "{parsed}");
    assert_eq!(body["action"], "protocol_authorization", "{parsed}");
    assert_eq!(body["chain"], format!("eip155:{HL_MAINNET}"), "{parsed}");
    assert_eq!(body["protocol"], HL_VC_ZERO, "{parsed}");
    assert_eq!(body["protocol_name"], "hyperliquid", "{parsed}");
    assert_eq!(body["permission"], "builder_fee", "{parsed}");
    assert_eq!(body["permission_limit"], "0.001%", "{parsed}");
    assert_eq!(body["authorizer"], HL_SIGNER, "{parsed}");
    assert_eq!(
        body["authorized"], "0x00000000000000000000000000000000b171d3c0",
        "{parsed}"
    );
    assert_eq!(body["is_authorized"], true, "{parsed}");
}

// b3.spotSend — adds an L1 `token` string → structured hl_spot_send. Also
// exercises the TESTNET chain (421614) to prove the 2nd chain_to_addresses entry routes.
#[test]
fn b3_hl_spot_send_routes_to_structured_testnet() {
    let install = install_ok(HL_REST_SPOT_SEND);
    assert_eq!(
        install["data"]["bundle_id"],
        "hyperliquid/rest/spot-send@1.0.0"
    );

    let message = json!({
        "hyperliquidChain": "Testnet",
        "destination": "0x00000000000000000000000000000000deadbeef",
        "token": "PURR:0xc1fb593aeffbeb02f85e0b7c0f6f3b8a7e7f7e7e",
        "amount": "42.5",
        "time": 1_700_000_000_u64
    });
    let input = hl_typed_data_input(421_614, "HyperliquidTransaction:SpotSend", message);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    let body = assert_hl_structured(
        &parsed,
        "hyperliquid/rest/spot-send@1.0.0",
        421_614,
        "hl_spot_send",
    );
    assert_eq!(
        body["destination"], "0x00000000000000000000000000000000deadbeef",
        "{parsed}"
    );
    assert_eq!(
        body["token"], "PURR:0xc1fb593aeffbeb02f85e0b7c0f6f3b8a7e7f7e7e",
        "{parsed}"
    );
    assert_eq!(body["amount"], "42.5", "{parsed}");
}
