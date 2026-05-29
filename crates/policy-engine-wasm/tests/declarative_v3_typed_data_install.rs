//! Phase A.1 — `declarative_route_typed_data_v3_json` end-to-end fixtures.
//!
//! Each test installs a v3 manifest carrying a `match.typed_data` block (which
//! populates the off-chain `(chain_id, verifying_contract, primary_type)`
//! bridge in `declarative_install_v3_json`), then routes a hand-built EIP-712
//! typed-data payload through `declarative_route_typed_data_v3_json` and
//! asserts the resulting `ActionBody` JSON.
//!
//! The load-bearing correctness item is the **WRAP RULE** (`declarative_exports::
//! build_typed_data_args_json`): the EIP-712 `message` is reshaped into the
//! `args_json` the manifest's `$args.<path>` placeholders expect. Two manifest
//! conventions are exercised:
//!   * **Nested** (Permit2 `PermitSingle`): one tuple ABI payload param →
//!     wrap `args_json = { permitSingle: message }`. Asserted by reading the
//!     built body's `spender` field (a real address) — NOT the whole message
//!     object — which only resolves correctly if the wrap happened.
//!   * **Flat** (EIP-2612 `permit`): multiple scalar ABI payload params →
//!     NO wrap, `args_json = message`. Asserted likewise: if the flat path
//!     wrongly wrapped, `$args.spender` would resolve to the whole object and
//!     the address assertion fails.
//!
//! Each fixture's manifest is inlined here verbatim (M2/Phase-A.1 should not
//! depend on the on-disk registry snapshot).

use policy_engine_wasm::{declarative_install_v3_json, declarative_route_typed_data_v3_json};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn install_ok(manifest: &str) -> Value {
    let out = declarative_install_v3_json(manifest.to_owned());
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "install failed: {parsed}");
    parsed
}

fn typed_data_input(
    chain_id: u64,
    verifying_contract: &str,
    primary_type: &str,
    domain_name: &str,
    message: Value,
    submitter: &str,
) -> String {
    json!({
        "chain_id": chain_id,
        "verifying_contract": verifying_contract,
        "primary_type": primary_type,
        "domain_name": domain_name,
        "message": message,
        "submitter": submitter,
        "submitted_at": 1_700_000_000_u64
    })
    .to_string()
}

const PERMIT2: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
const USDC_MAINNET: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const SPENDER: &str = "0x00000000000000000000000000000000deadbeef";
const SIGNER: &str = "0x000000000000000000000000000000000000aaaa";

// ---------------------------------------------------------------------------
// #1 — Permit2 PermitSingle (nested → wrapped)
// ---------------------------------------------------------------------------
//
// ABI payload = single `permitSingle` tuple param (plus `owner` + `signature`
// machinery params which the wrap rule filters out). So the message IS that
// tuple's content → `args_json = { permitSingle: message }`. The body uses
// `$args.permitSingle.spender` etc. The manifest emits
// `token`/`permit2_sign_allowance` (Permit2SignAction). Its `nonce` field is a
// `LiveField<(U256,u8)>` populated from `live_inputs.nonce`.

const PERMIT2_PERMIT_SINGLE_V3: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/permit2/permitSingle@2.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x000000000022D473030F116dDEE9F6B43aC78BA3"]
    },
    "selector": "0x2b67b570",
    "typed_data": {
      "domain_name": "Permit2",
      "verifying_contract": "0x000000000022D473030F116dDEE9F6B43aC78BA3",
      "primary_type": "PermitSingle",
      "types": {
        "PermitSingle": [
          { "name": "details", "type": "PermitDetails" },
          { "name": "spender", "type": "address" },
          { "name": "sigDeadline", "type": "uint256" }
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
          "name": "permitSingle",
          "type": "tuple",
          "components": [
            {
              "name": "details",
              "type": "tuple",
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
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "permit2_sign_allowance",
        "permit2_sign_allowance": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.permitSingle.details.token" } },
          "spender": "$args.permitSingle.spender",
          "amount": "$args.permitSingle.details.amount",
          "expires_at": "$args.permitSingle.details.expiration",
          "sig_deadline": "$args.permitSingle.sigDeadline"
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

#[test]
fn typed_data_permit2_permit_single_nested_wrapped() {
    let install = install_ok(PERMIT2_PERMIT_SINGLE_V3);
    assert_eq!(install["data"]["bundle_id"], "uniswap/permit2/permitSingle@2.0.0");

    // EIP-712 message — the PermitSingle tuple's content directly (NO outer
    // `permitSingle` wrapper; the wrap rule supplies that).
    let message = json!({
        "details": {
            "token": USDC_MAINNET,
            "amount": "1000",
            "expiration": 1_738_001_800_u64,
            "nonce": "0"
        },
        "spender": SPENDER,
        "sigDeadline": 1_738_002_000_u64
    });
    let input = typed_data_input(1, PERMIT2, "PermitSingle", "Permit2", message, SIGNER);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswap/permit2/permitSingle@2.0.0",
        "{parsed}"
    );

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "expected exactly 1 action: {parsed}");

    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(meta["nature"]["domain"]["name"], "Permit2", "{parsed}");
    // deadline came from message.sigDeadline.
    assert_eq!(meta["nature"]["deadline"], 1_738_002_000_u64, "{parsed}");

    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "permit2_sign_allowance", "{parsed}");
    // THE WRAP PROOF: `$args.permitSingle.spender` resolved to the actual
    // spender ADDRESS, not the whole message object. If the wrap had not
    // happened, `$args.permitSingle.*` would have failed to resolve.
    assert_eq!(body["spender"], SPENDER, "{parsed}");
    assert_eq!(body["token"]["key"]["address"], USDC_MAINNET, "{parsed}");
    assert_eq!(body["token"]["key"]["standard"], "erc20", "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:1", "{parsed}");
    // amount round-trips as a hex string through alloy serde (1000 == 0x3e8).
    assert_eq!(body["amount"], "0x3e8", "{parsed}");
}

// ---------------------------------------------------------------------------
// #2 — EIP-2612-style permit (flat → unwrapped)
// ---------------------------------------------------------------------------
//
// ABI payload = MULTIPLE scalar params (`spender`, `value`, `deadline`) after
// filtering out `owner` + `v`/`r`/`s`. So NO wrap: `args_json = message` and
// `$args.spender` / `$args.value` resolve against the message directly. The
// manifest emits `token`/`erc20_permit` (Erc20PermitAction). Its `nonce` field
// is a `LiveField<U256>` populated from `live_inputs.nonce`.

const ERC2612_PERMIT_V3: &str = r#"{
  "type": "adapter_function",
  "id": "standard/erc20/permit@2.0.0",
  "publisher": "ethereum.org",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x6B175474E89094C44Da98b954EedeAC495271d0F"]
    },
    "selector": "0xd505accf",
    "typed_data": {
      "domain_name": "Dai Stablecoin",
      "verifying_contract": "0x6B175474E89094C44Da98b954EedeAC495271d0F",
      "primary_type": "Permit",
      "types": {
        "Permit": [
          { "name": "owner", "type": "address" },
          { "name": "spender", "type": "address" },
          { "name": "value", "type": "uint256" },
          { "name": "nonce", "type": "uint256" },
          { "name": "deadline", "type": "uint256" }
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
        { "name": "spender", "type": "address" },
        { "name": "value", "type": "uint256" },
        { "name": "deadline", "type": "uint256" },
        { "name": "v", "type": "uint8" },
        { "name": "r", "type": "bytes32" },
        { "name": "s", "type": "bytes32" }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_permit",
        "erc20_permit": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$tx.to" } },
          "spender": "$args.spender",
          "amount": "$args.value",
          "deadline": "$args.deadline"
        }
      }
    },
    "live_inputs": {
      "nonce": {
        "source": {
          "kind": "onchain_view",
          "chain": "$chain",
          "contract": "0x6b175474e89094c44da98b954eedeac495271d0f",
          "function": "nonces(address)",
          "decoder_id": "erc20_permit_nonce"
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

#[test]
fn typed_data_erc2612_permit_flat_unwrapped() {
    let dai = "0x6b175474e89094c44da98b954eedeac495271d0f";
    install_ok(ERC2612_PERMIT_V3);

    // Flat EIP-2612 message — keys map directly to `$args.<key>`.
    let message = json!({
        "owner": SIGNER,
        "spender": SPENDER,
        "value": "5000",
        "nonce": "0",
        "deadline": 1_738_002_000_u64
    });
    // verifying_contract checksum-mixed on input → must still resolve
    // (lowercased at bridge lookup).
    let input = typed_data_input(
        1,
        "0x6B175474E89094C44Da98b954EedeAC495271d0F",
        "Permit",
        "Dai Stablecoin",
        message,
        SIGNER,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(parsed["data"]["decoder_id"], "standard/erc20/permit@2.0.0", "{parsed}");

    let body = &parsed["data"]["actions"][0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_permit", "{parsed}");
    // THE FLAT PROOF: `$args.spender` resolved to the message's spender
    // ADDRESS directly (no wrap). If the flat path had wrongly wrapped,
    // `$args.spender` would resolve to the whole message object and this
    // address assertion would fail.
    assert_eq!(body["spender"], SPENDER, "{parsed}");
    // amount = message.value (5000 == 0x1388), proving $args.value resolved flat.
    assert_eq!(body["amount"], "0x1388", "{parsed}");
    // token.key.address = $tx.to = verifying_contract (lowercased).
    assert_eq!(body["token"]["key"]["address"], dai, "{parsed}");
    // deadline came from message.deadline (EIP-2612 has no sigDeadline).
    assert_eq!(
        parsed["data"]["actions"][0]["meta"]["nature"]["deadline"],
        1_738_002_000_u64,
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// #3 — UniswapX ExclusiveDutchOrder (nested → wrapped, amm/sign_intent_order)
// ---------------------------------------------------------------------------
//
// Route-proof for the `registryV2/manifests/uniswapx/exclusive-dutch-order/
// sign@1.0.0.json` fixture. The ABI payload is a SINGLE `order` tuple param
// (plus the `signature` machinery param the wrap rule filters out), so the
// EIP-712 message IS the order content → `args_json = { order: message }` and
// the body's `$args.order.*` placeholders resolve. The manifest emits
// `amm`/`sign_intent_order` (SignIntentOrderAction); its `venue` is the
// `IntentVenue::UniswapX { name, chain, reactor }` tagged enum (snake_case
// `uniswap_x`) and its `live_inputs` (expected_fill_price: LiveField<Price>,
// competing_orders: LiveField<u32>) take the NESTED layout (defaults `"0"` /
// `0` from the action_builder catalog).
//
// `emit.body` + `abi_fragment` are kept byte-identical to the committed
// fixture (only the reactor placeholder address is shared) so this test
// pins the on-disk manifest's routing behaviour.

const UNISWAPX_REACTOR: &str = "0x6000da47483062a0d734ba3dc7576ce6a0b645c4";
const TOKEN_IN: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"; // WETH
const TOKEN_OUT: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
const RECIPIENT: &str = "0x000000000000000000000000000000000000c0fe";

const UNISWAPX_EXCLUSIVE_DUTCH_V3: &str = r#"{
  "type": "adapter_action",
  "id": "uniswapx/exclusive-dutch-order/sign@1.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x00000001",
    "chain_to_addresses": {
      "1": ["0x6000da47483062a0d734ba3dc7576ce6a0b645c4"]
    },
    "typed_data": {
      "domain_name": "UniswapX",
      "verifying_contract": "0x6000da47483062a0d734ba3dc7576ce6a0b645c4",
      "primary_type": "ExclusiveDutchOrder",
      "types": {
        "ExclusiveDutchOrder": [
          { "name": "info", "type": "OrderInfo" },
          { "name": "decayStartTime", "type": "uint256" },
          { "name": "decayEndTime", "type": "uint256" },
          { "name": "exclusiveFiller", "type": "address" },
          { "name": "exclusivityOverrideBps", "type": "uint256" },
          { "name": "input", "type": "DutchInput" },
          { "name": "outputs", "type": "DutchOutput[]" }
        ],
        "OrderInfo": [
          { "name": "reactor", "type": "address" },
          { "name": "swapper", "type": "address" },
          { "name": "nonce", "type": "uint256" },
          { "name": "deadline", "type": "uint256" }
        ],
        "DutchInput": [
          { "name": "token", "type": "address" },
          { "name": "startAmount", "type": "uint256" },
          { "name": "endAmount", "type": "uint256" }
        ],
        "DutchOutput": [
          { "name": "token", "type": "address" },
          { "name": "startAmount", "type": "uint256" },
          { "name": "endAmount", "type": "uint256" },
          { "name": "recipient", "type": "address" }
        ]
      }
    }
  },
  "abi_fragment": {
    "function_name": "execute",
    "abi": {
      "name": "execute",
      "type": "function",
      "inputs": [
        {
          "name": "order",
          "type": "tuple",
          "components": [
            {
              "name": "info",
              "type": "tuple",
              "components": [
                { "name": "reactor", "type": "address" },
                { "name": "swapper", "type": "address" },
                { "name": "nonce", "type": "uint256" },
                { "name": "deadline", "type": "uint256" }
              ]
            },
            { "name": "decayStartTime", "type": "uint256" },
            { "name": "decayEndTime", "type": "uint256" },
            { "name": "exclusiveFiller", "type": "address" },
            { "name": "exclusivityOverrideBps", "type": "uint256" },
            {
              "name": "input",
              "type": "tuple",
              "components": [
                { "name": "token", "type": "address" },
                { "name": "startAmount", "type": "uint256" },
                { "name": "endAmount", "type": "uint256" }
              ]
            },
            {
              "name": "outputs",
              "type": "tuple[]",
              "components": [
                { "name": "token", "type": "address" },
                { "name": "startAmount", "type": "uint256" },
                { "name": "endAmount", "type": "uint256" },
                { "name": "recipient", "type": "address" }
              ]
            }
          ]
        },
        { "name": "signature", "type": "bytes" }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "amm",
      "amm": {
        "action": "sign_intent_order",
        "sign_intent_order": {
          "venue": {
            "name": "uniswap_x",
            "chain": "$chain",
            "reactor": "$to"
          },
          "sell": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.order.input.token" } },
          "buy": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.order.outputs[0].token" } },
          "sell_amount": "$args.order.input.startAmount",
          "buy_min": "$args.order.outputs[0].endAmount",
          "order_kind": "dutch",
          "recipient": "$args.order.outputs[0].recipient",
          "valid_until": "$args.order.info.deadline"
        }
      }
    },
    "live_inputs": {
      "expected_fill_price": {
        "source": {
          "kind": "venue_api",
          "endpoint": "https://api.uniswap.org/v2/orders",
          "parser_id": "uniswapx_quote"
        },
        "ttl_s": 6
      },
      "competing_orders": {
        "source": {
          "kind": "venue_api",
          "endpoint": "https://api.uniswap.org/v2/orders",
          "parser_id": "uniswapx_open_orders"
        },
        "ttl_s": 6
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
fn typed_data_uniswapx_exclusive_dutch_sign_intent_order() {
    install_ok(UNISWAPX_EXCLUSIVE_DUTCH_V3);

    // EIP-712 message — the ExclusiveDutchOrder content directly (the wrap
    // rule supplies the outer `order` key because the ABI payload is a single
    // `order` tuple param).
    let message = json!({
        "info": {
            "reactor": UNISWAPX_REACTOR,
            "swapper": SIGNER,
            "nonce": "0",
            "deadline": 1_738_002_000_u64
        },
        "decayStartTime": 1_738_001_800_u64,
        "decayEndTime": 1_738_002_000_u64,
        "exclusiveFiller": "0x0000000000000000000000000000000000000000",
        "exclusivityOverrideBps": "0",
        "input": {
            "token": TOKEN_IN,
            "startAmount": "1000000000000000000",
            "endAmount": "1000000000000000000"
        },
        "outputs": [
            {
                "token": TOKEN_OUT,
                "startAmount": "3500000000",
                "endAmount": "3400000000",
                "recipient": RECIPIENT
            }
        ]
    });
    let input = typed_data_input(
        1,
        UNISWAPX_REACTOR,
        "ExclusiveDutchOrder",
        "UniswapX",
        message,
        SIGNER,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswapx/exclusive-dutch-order/sign@1.0.0",
        "{parsed}"
    );

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "expected exactly 1 action: {parsed}");

    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(meta["nature"]["domain"]["name"], "UniswapX", "{parsed}");
    // deadline came from message.info.deadline (no top-level sigDeadline /
    // deadline on the ExclusiveDutchOrder message, so the best-effort
    // OffchainSig deadline collapses to 0 — that's fine, it's not the routing
    // proof. The load-bearing assertion is the body below.)

    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "amm", "{parsed}");
    assert_eq!(body["action"], "sign_intent_order", "{parsed}");
    // THE WRAP PROOF: `$args.order.input.token` resolved to the actual sell
    // token ADDRESS, not the whole message object — only possible if the wrap
    // happened under `order`.
    assert_eq!(body["venue"]["name"], "uniswap_x", "{parsed}");
    assert_eq!(body["venue"]["reactor"], UNISWAPX_REACTOR, "{parsed}");
    assert_eq!(body["venue"]["chain"], "eip155:1", "{parsed}");
    assert_eq!(body["sell"]["key"]["address"], TOKEN_IN, "{parsed}");
    assert_eq!(body["buy"]["key"]["address"], TOKEN_OUT, "{parsed}");
    assert_eq!(body["order_kind"], "dutch", "{parsed}");
    assert_eq!(body["recipient"], RECIPIENT, "{parsed}");
    // valid_until = message.info.deadline (Time transparent over u64).
    assert_eq!(body["valid_until"], 1_738_002_000_u64, "{parsed}");
    // sell_amount round-trips as hex through alloy serde
    // (1000000000000000000 == 0xde0b6b3a7640000).
    assert_eq!(body["sell_amount"], "0xde0b6b3a7640000", "{parsed}");
    // live_inputs NESTED layout with catalog defaults applied
    // (expected_fill_price: Price="0", competing_orders: u32=0).
    assert_eq!(body["live_inputs"]["expected_fill_price"]["value"], "0", "{parsed}");
    assert_eq!(body["live_inputs"]["competing_orders"]["value"], 0, "{parsed}");
}

// ---------------------------------------------------------------------------
// #4 — HyperLiquid UsdSend (nested → wrapped, token/erc20_transfer)
// ---------------------------------------------------------------------------
//
// Route-proof for the `registryV2/manifests/hyperliquid/usd-send/sign@1.0.0.
// json` fixture. The ABI payload is a SINGLE `usdSend` tuple param → wrap rule
// wraps `args_json = { usdSend: message }`. The manifest emits
// `token`/`erc20_transfer` (Erc20TransferAction, no live_inputs). The
// `recipient` is sourced from `$args.usdSend.destination` (a hex address
// string), while `amount` + the token address are LITERAL placeholders
// (HyperLiquid `amount` is a decimal string that cannot parse as U256, and the
// USDC value moves on the HyperLiquid L1, not as an EVM token transfer) — a
// B.3 placeholder so the payload ROUTES to a valid ActionBody.
//
// The colon-bearing primaryType `HyperliquidTransaction:UsdSend` is the exact
// EIP-712 discriminator (kept verbatim by the bridge — never lowered).

const HYPERLIQUID_USD_SEND_V3: &str = r#"{
  "type": "adapter_action",
  "id": "hyperliquid/usd-send/sign@1.0.0",
  "publisher": "hyperliquid",
  "schema_version": "3",
  "match": {
    "selector": "0x00000002",
    "chain_to_addresses": {
      "42161": ["0x0000000000000000000000000000000000000000"],
      "421614": ["0x0000000000000000000000000000000000000000"]
    },
    "typed_data": {
      "domain_name": "HyperliquidSignTransaction",
      "verifying_contract": "0x0000000000000000000000000000000000000000",
      "primary_type": "HyperliquidTransaction:UsdSend",
      "types": {
        "HyperliquidTransaction:UsdSend": [
          { "name": "hyperliquidChain", "type": "string" },
          { "name": "destination", "type": "string" },
          { "name": "amount", "type": "string" },
          { "name": "time", "type": "uint64" }
        ]
      }
    }
  },
  "abi_fragment": {
    "function_name": "usdSend",
    "abi": {
      "name": "usdSend",
      "type": "function",
      "inputs": [
        {
          "name": "usdSend",
          "type": "tuple",
          "components": [
            { "name": "hyperliquidChain", "type": "string" },
            { "name": "destination", "type": "address" },
            { "name": "amount", "type": "string" },
            { "name": "time", "type": "uint64" }
          ]
        }
      ]
    }
  },
  "emit": {
    "strategy": "single_emit",
    "body": {
      "domain": "token",
      "token": {
        "action": "erc20_transfer",
        "erc20_transfer": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "0x0000000000000000000000000000000000000000" } },
          "recipient": "$args.usdSend.destination",
          "amount": "0"
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
fn typed_data_hyperliquid_usd_send_erc20_transfer() {
    install_ok(HYPERLIQUID_USD_SEND_V3);

    // EIP-712 message — the UsdSend content directly. `destination` is a hex
    // address string (decodes as Address); `amount` is a HyperLiquid decimal
    // string (deliberately NOT used for the body's U256 amount).
    let message = json!({
        "hyperliquidChain": "Mainnet",
        "destination": "0x00000000000000000000000000000000deadbeef",
        "amount": "100.0",
        "time": 1_700_000_000_u64
    });
    let input = typed_data_input(
        42161,
        "0x0000000000000000000000000000000000000000",
        "HyperliquidTransaction:UsdSend",
        "HyperliquidSignTransaction",
        message,
        SIGNER,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "hyperliquid/usd-send/sign@1.0.0",
        "{parsed}"
    );

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "expected exactly 1 action: {parsed}");

    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(
        meta["nature"]["domain"]["name"], "HyperliquidSignTransaction",
        "{parsed}"
    );

    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "token", "{parsed}");
    assert_eq!(body["action"], "erc20_transfer", "{parsed}");
    // THE WRAP PROOF: `$args.usdSend.destination` resolved to the actual
    // destination ADDRESS (lowercased by alloy), proving the wrap under
    // `usdSend` happened.
    assert_eq!(
        body["recipient"], "0x00000000000000000000000000000000deadbeef",
        "{parsed}"
    );
    // amount is the literal 0 placeholder (round-trips as 0x0 through alloy).
    assert_eq!(body["amount"], "0x0", "{parsed}");
    assert_eq!(body["token"]["key"]["standard"], "erc20", "{parsed}");
    assert_eq!(body["token"]["key"]["chain"], "eip155:42161", "{parsed}");
}

// ---------------------------------------------------------------------------
// #5 — no typed_data block → no_typed_data_mapper
// ---------------------------------------------------------------------------
//
// A manifest WITHOUT a `match.typed_data` block never populates the typed-data
// bridge, so a typed-data route against its verifying_contract misses.

const NO_TYPED_DATA_V3: &str = r#"{
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
fn typed_data_no_typed_data_block_misses() {
    install_ok(NO_TYPED_DATA_V3);

    // Route a typed-data payload against the SAME verifying_contract — but the
    // manifest carried no `match.typed_data`, so the typed-data bridge is
    // empty for it.
    let message = json!({ "spender": SPENDER, "amount": "1000" });
    let input = typed_data_input(
        1,
        USDC_MAINNET,
        "Approve",
        "USD Coin",
        message,
        SIGNER,
    );

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(parsed["error"]["kind"], "no_typed_data_mapper", "{parsed}");
}
