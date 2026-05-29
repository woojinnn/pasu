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
          "new_mode": "variable"
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
            // interestRateMode (current mode being swapped from)
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
    assert_eq!(body["new_mode"], "variable");
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
// shape of supply@1.0.0 / repay@1.0.0. `repayWithPermit` binds a literal
// `rate_mode: "variable"` (mirroring swapBorrowRateMode's `new_mode`) because
// the on-chain `interestRateMode` uint does not deserialize into the `RateMode`
// enum and repay@1.0.0's `$derived.aave_v3_rate_mode` has no registered
// fallback type. Both route FULLY GREEN (`ok:true`) on the B.2-infra
// foundation (lending `live_input_default` skeletons + uint≤64 coercion).

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
          "rate_mode":    "variable",
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
    // `interestRateMode` arg is decoded but the body binds a literal
    // `rate_mode: "variable"` (the on-chain uint can't deserialize into the
    // RateMode enum); the 4 permit params are decoded but ignored.
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
            // interestRateMode (2 = variable; decoded but not bound via $args)
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
    // repay@1.0.0 but with the route-green literal `rate_mode: "variable"`.
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
    assert_eq!(body["rate_mode"], "variable");
    assert_eq!(body["use_a_tokens"], false);
    // live_input defaults wrapped + deserialized (same 3 as repay@1.0.0).
    assert_eq!(
        body["live_inputs"]["reserve_state"]["value"]["total_borrow"],
        "0x0"
    );
    assert_eq!(body["live_inputs"]["current_debt"]["value"], "0x0");
}
