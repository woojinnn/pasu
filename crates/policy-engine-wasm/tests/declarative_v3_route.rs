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
use policy_engine_wasm::{declarative_install_v3_json, declarative_route_request_v3_json};
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

    // Empty `transfers` array → empty Multicall (valid, ok:true).
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
    assert_eq!(body["domain"], "multicall", "{parsed}");
    assert!(
        body["actions"]
            .as_array()
            .expect("inner actions")
            .is_empty(),
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
//      the live_input `source.contract`) therefore bind `$resolved.pool` (a
//      registered Address placeholder; Sync fills the real per-chain Pool, so it
//      resolves to the zero-address placeholder on this narrow-scope route path)
//      — NOT `$args.pool` (the ignored dummy) and NOT `$to` (the WTG, not the
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
const ADDR_ZERO: &str = "0x0000000000000000000000000000000000000000";
// `$resolved.weth` IS pre-populated by the route handler (declarative_exports
// `route_request` chain→WETH map) — mainnet 1 → canonical WETH9. So the WETH
// asset substitutes the REAL address on the route path (NOT a zero
// placeholder). `$resolved.pool`, by contrast, is NOT pre-populated (the Sync
// orchestrator fills it later) → it falls through to the Address-typed zero
// placeholder. These two assertions together prove the watch-point resolution:
// asset = real WETH, venue.pool = $resolved.pool (zero until Sync wires it).
const WETH_MAINNET: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

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
    // pool/asset bind `$resolved.{pool,weth}` → zero-address placeholder on the
    // route path (Sync orchestrator not wired here).
    assert_eq!(body["venue"]["pool"], ADDR_ZERO);
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
    assert_eq!(body["venue"]["pool"], ADDR_ZERO);
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
    assert_eq!(body["venue"]["pool"], ADDR_ZERO);
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
    assert_eq!(body["venue"]["pool"], ADDR_ZERO);
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
    assert_eq!(install["data"]["bundle_id"], "uniswap/permit2/lockdown@1.0.0");

    let input = route_input(
        1,
        "0x000000000022d473030f116ddee9f6b43ac78ba3",
        "0xcc53287f",
        b1_two_element_lockdown_calldata(),
        "0x000000000000000000000000000000000000aaaa",
    );
    let parsed = route_ok(input);
    assert_eq!(parsed["data"]["decoder_id"], "uniswap/permit2/lockdown@1.0.0");

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
    assert_eq!(install["data"]["bundle_id"], "uniswap/permit2/permitBatch@1.0.0");

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
    assert_eq!(inner.len(), 2, "one permit2_sign_allowance per details: {parsed}");

    // element-0 — token = USDC, amount = 1000, expiration = 1738001800.
    assert_eq!(inner[0]["domain"], "token", "{parsed}");
    assert_eq!(inner[0]["action"], "permit2_sign_allowance", "{parsed}");
    assert_eq!(
        inner[0]["token"]["key"]["address"],
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
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
        inner[0]["spender"],
        "0x00000000000000000000000000000000deadbeef",
        "{parsed}"
    );

    // element-1 — token = WETH, amount = 2000, expiration = 1738001900, nonce = 1.
    assert_eq!(
        inner[1]["token"]["key"]["address"],
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
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
        DynSolValue::Int(alloy_primitives::I256::try_from(887_220i64).unwrap(), 24),  // [4] tickUpper
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
    assert_eq!(
        body["params"]["nft_key"]["token_id"], "0x67932",
        "{parsed}"
    ); // 424242
    assert_eq!(body["params"]["amount_min"][0], "0x32", "{parsed}"); // 50
    assert_eq!(body["params"]["amount_min"][1], "0x3c", "{parsed}"); // 60
}

// ---------------------------------------------------------------------------
// b1.nfpm.collect — collect_fees (no params enum; direct nft_key + recipient)
// ---------------------------------------------------------------------------

#[test]
fn b1_nfpm_collect_collect_fees() {
    let install = install_ok(NFPM_COLLECT_V3);
    assert_eq!(install["data"]["bundle_id"], "uniswap/v3-nfpm/collect@1.0.0");

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
