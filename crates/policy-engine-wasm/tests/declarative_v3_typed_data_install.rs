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
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/permit2/permitSingle@2.0.0"
    );

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
    assert_eq!(
        parsed["data"]["decoder_id"], "standard/erc20/permit@2.0.0",
        "{parsed}"
    );

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
        parsed["data"]["actions"][0]["meta"]["nature"]["deadline"], 1_738_002_000_u64,
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
    assert_eq!(
        body["live_inputs"]["expected_fill_price"]["value"], "0",
        "{parsed}"
    );
    assert_eq!(
        body["live_inputs"]["competing_orders"]["value"], 0,
        "{parsed}"
    );
}

// ---------------------------------------------------------------------------
// #4 — HyperLiquid UsdSend (Mode B "UserSigned") → best-effort Unknown
// ---------------------------------------------------------------------------
//
// Route-proof for the on-disk `registryV2/manifests/hyperliquid/rest/
// usd-send@1.0.0.json` fixture (pinned via `include_str!` so this test tracks
// the committed manifest). HyperLiquid's REST `HyperliquidTransaction:UsdSend`
// is an OFF-CHAIN L1 action authorized by an `eth_signTypedData_v4` signature:
// `amount` is a DECIMAL STRING ("100.0", not U256) and `destination` is an L1
// identifier string. The 8-domain ActionBody cannot faithfully hold those, so a
// `token`/`erc20_transfer` mapping (amount→0, token→0x0) would be a MISLABEL
// with DATA LOSS. The frozen decision routes it to best-effort `ActionBody::
// Unknown` instead: `target=0x0` sentinel (off-chain sign has NO contract
// target), `chain=$chain`, `calldata="0x"` (sigs have no calldata), `value="0"`.
// The WIN is ROUTING (recognized: a HyperLiquid UsdSend signature, NOT
// no_adapter); the STRUCTURED representation (destination/amount) requires a NEW
// off-chain-exchange ActionBody variant = DEFERRED schema enhancement (the key
// b3 limitation, OUT OF SCOPE). This SUPERSEDES the prior erc20_transfer
// placeholder mapping (and the deleted `hyperliquid/usd-send/sign@1.0.0.json`).
//
// The colon-bearing primaryType `HyperliquidTransaction:UsdSend` is the exact
// EIP-712 discriminator (kept verbatim by the bridge — never lowered). vc=0x0
// membership: the typed-data bridge keys on the `chain_to_addresses` chain ids
// (42161 / 421614), so those entries (value 0x0) are what make the per-chain
// bridge keys install — mirrored from the canonical fixture.

const HYPERLIQUID_USD_SEND_V3: &str =
    include_str!("../../../registryV2/manifests/hyperliquid/rest/usd-send@1.0.0.json");

#[test]
fn typed_data_hyperliquid_usd_send_best_effort_unknown() {
    let install = install_ok(HYPERLIQUID_USD_SEND_V3);
    assert_eq!(
        install["data"]["bundle_id"], "hyperliquid/rest/usd-send@1.0.0",
        "{install}"
    );

    // EIP-712 message — the UsdSend content directly. `destination` is an L1
    // identifier string and `amount` a HyperLiquid decimal string; NEITHER is
    // surfaced in a structured body (the deferred off-chain-exchange variant
    // would carry them — that is the data the Unknown bucket cannot represent).
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
        parsed["data"]["decoder_id"], "hyperliquid/rest/usd-send@1.0.0",
        "{parsed}"
    );

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(actions.len(), 1, "expected exactly 1 action: {parsed}");

    // OffchainSig nature + HyperLiquid domain bound to chain 42161.
    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(
        meta["nature"]["domain"]["name"], "HyperliquidSignTransaction",
        "{parsed}"
    );
    assert_eq!(meta["nature"]["domain"]["chain_id"], 42161, "{parsed}");

    // Best-effort Unknown body — the frozen sentinel shape (recognized, NOT a
    // token transfer): target 0x0 sentinel, chain $chain, value "0", calldata
    // "0x". No `recipient`/`amount`/`token` fields exist on an Unknown body.
    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "unknown", "{parsed}");
    assert_eq!(
        body["target"], "0x0000000000000000000000000000000000000000",
        "{parsed}"
    );
    assert_eq!(body["chain"], "eip155:42161", "{parsed}");
    assert_eq!(body["value"], "0x0", "{parsed}");
    assert_eq!(body["calldata"], "0x", "{parsed}");
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
    let input = typed_data_input(1, USDC_MAINNET, "Approve", "USD Coin", message, SIGNER);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], false, "{parsed}");
    assert_eq!(parsed["error"]["kind"], "no_typed_data_mapper", "{parsed}");
}

// ---------------------------------------------------------------------------
// #6 — Permit2 PermitBatch (Phase A.2 array_emit, off-chain array sig)
// ---------------------------------------------------------------------------
//
// PermitBatch's ABI payload is a SINGLE `permitBatch` tuple param
// `(PermitDetails[] details, address spender, uint256 sigDeadline)`, so the
// wrap rule wraps `args_json = { permitBatch: message }`. The `emit.strategy`
// is `array_emit` with `array_source: "$args.permitBatch.details"` — the
// homogeneous `details` array fans out to one `permit2_sign_allowance` per
// element. Each element binds as `$inputs.*` (token / amount / expiration),
// while the batch-level `spender` / `sigDeadline` resolve via `$args.*` in the
// per-item body (the child ctx keeps the full wrapped args_json). Per-element
// `live_inputs.nonce` wraps into the `LiveField<(U256,u8)>` default.
//
// The two `details` elements differ (token / amount) — proving per-element
// binding through the typed-data route.

const PERMIT2_PERMIT_BATCH_V3: &str = r#"{
  "type": "adapter_function",
  "id": "uniswap/permit2/permitBatch@2.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "2",
  "match": {
    "chain_to_addresses": {
      "1": ["0x000000000022D473030F116dDEE9F6B43aC78BA3"]
    },
    "selector": "0x2a2d80d1",
    "typed_data": {
      "domain_name": "Permit2",
      "verifying_contract": "0x000000000022D473030F116dDEE9F6B43aC78BA3",
      "primary_type": "PermitBatch",
      "types": {
        "PermitBatch": [
          { "name": "details", "type": "PermitDetails[]" },
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
    "array_source": "$args.permitBatch.details",
    "body": {
      "domain": "token",
      "token": {
        "action": "permit2_sign_allowance",
        "permit2_sign_allowance": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.token" } },
          "spender": "$args.permitBatch.spender",
          "amount": "$inputs.amount",
          "expires_at": "$inputs.expiration",
          "sig_deadline": "$args.permitBatch.sigDeadline"
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

const USDT_MAINNET: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";

#[test]
fn typed_data_permit2_permit_batch_array_emit() {
    let install = install_ok(PERMIT2_PERMIT_BATCH_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/permit2/permitBatch@2.0.0"
    );

    // EIP-712 message — the PermitBatch content directly. `details` is a
    // 2-element array; the wrap rule supplies the outer `permitBatch` key.
    // expiration / sigDeadline kept as JSON numbers (Time over u64).
    let message = json!({
        "details": [
            {
                "token": USDC_MAINNET,
                "amount": "1000",
                "expiration": 1_738_001_800_u64,
                "nonce": "0"
            },
            {
                "token": USDT_MAINNET,
                "amount": "2000",
                "expiration": 1_738_001_900_u64,
                "nonce": "1"
            }
        ],
        "spender": SPENDER,
        "sigDeadline": 1_738_002_000_u64
    });
    let input = typed_data_input(1, PERMIT2, "PermitBatch", "Permit2", message, SIGNER);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswap/permit2/permitBatch@2.0.0",
        "{parsed}"
    );

    let actions = parsed["data"]["actions"].as_array().expect("actions array");
    assert_eq!(
        actions.len(),
        1,
        "expected exactly 1 outer action: {parsed}"
    );

    // OffchainSig nature on the single outer Action.
    let meta = &actions[0]["meta"];
    assert_eq!(meta["nature"]["kind"], "offchain_sig", "{parsed}");
    assert_eq!(meta["nature"]["domain"]["name"], "Permit2", "{parsed}");
    // deadline from message.sigDeadline.
    assert_eq!(meta["nature"]["deadline"], 1_738_002_000_u64, "{parsed}");

    // Body is a Multicall with 2 permit2_sign_allowance actions.
    let body = &actions[0]["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let inner = body["actions"].as_array().expect("inner actions array");
    assert_eq!(inner.len(), 2, "{parsed}");

    // element-0 — token = USDC, amount = 1000.
    assert_eq!(inner[0]["domain"], "token", "{parsed}");
    assert_eq!(inner[0]["action"], "permit2_sign_allowance", "{parsed}");
    assert_eq!(
        inner[0]["token"]["key"]["address"], USDC_MAINNET,
        "{parsed}"
    );
    assert_eq!(inner[0]["amount"], "0x3e8", "{parsed}"); // 1000
                                                         // batch-level spender / sig_deadline resolved via $args in per-item body.
    assert_eq!(inner[0]["spender"], SPENDER, "{parsed}");
    assert_eq!(inner[0]["sig_deadline"], 1_738_002_000_u64, "{parsed}");
    assert_eq!(inner[0]["expires_at"], 1_738_001_800_u64, "{parsed}");
    // nonce LiveField default applied (word 0, bit 0). The U256 word
    // round-trips as hex "0x0" through alloy serde; the u8 bit as 0.
    assert_eq!(inner[0]["nonce"]["value"], json!(["0x0", 0]), "{parsed}");

    // element-1 — DIFFERENT token + amount + expiration prove per-element bind.
    assert_eq!(inner[1]["action"], "permit2_sign_allowance", "{parsed}");
    assert_eq!(
        inner[1]["token"]["key"]["address"], USDT_MAINNET,
        "{parsed}"
    );
    assert_eq!(inner[1]["amount"], "0x7d0", "{parsed}"); // 2000
    assert_eq!(inner[1]["expires_at"], 1_738_001_900_u64, "{parsed}");
    // batch-level fields shared across all elements.
    assert_eq!(inner[1]["spender"], SPENDER, "{parsed}");
    assert_eq!(inner[1]["sig_deadline"], 1_738_002_000_u64, "{parsed}");
}

// ---------------------------------------------------------------------------
// #6b — Permit2 PermitBatch ON-DISK manifest (schema_version "3")
// ---------------------------------------------------------------------------
//
// Pins the GREEN wallet-facing route for the on-disk
// `registryV2/manifests/uniswap/permit2/permitBatch@1.0.0.json` (inlined here
// VERBATIM — `type:"adapter_action"`, `schema_version:"3"`, the canonical
// Permit2 CREATE2 address on 4 chains, named per-item `$inputs.*` access). This
// is the off-chain EIP-712 `permitBatch` SIGNATURE — the analysis surface a
// wallet actually sees. (The on-chain `permit(...)` calldata is a relayer
// submission; its nested uint48→Time ABI-width decode is now handled by the
// b1-infra bridge fix — see the positional-access calldata test
// `declarative_v3_route.rs::b2_permit2_permit_batch_calldata_decodes`. This
// on-disk manifest's NAMED `$inputs.token` access remains the typed-data
// convention, so the calldata route resolves it positionally.)
//
// Distinct from the `@2.0.0` fixture above (which exercised the same array_emit
// engine with the older `schema_version "2"` shape): this one is byte-for-byte
// the committed `@1.0.0` manifest, so the test fails loudly if the on-disk
// emit shape drifts.

const PERMIT2_PERMIT_BATCH_ON_DISK_V3: &str = r#"{
  "type": "adapter_action",
  "id": "uniswap/permit2/permitBatch@1.0.0",
  "publisher": "uniswap.eth",
  "schema_version": "3",
  "match": {
    "selector": "0x2a2d80d1",
    "chain_to_addresses": {
      "1":     ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "10":    ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "8453":  ["0x000000000022d473030f116ddee9f6b43ac78ba3"],
      "42161": ["0x000000000022d473030f116ddee9f6b43ac78ba3"]
    },
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
    "array_source": "$args.permitBatch.details",
    "body": {
      "domain": "token",
      "token": {
        "action": "permit2_sign_allowance",
        "permit2_sign_allowance": {
          "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.token" } },
          "spender": "$args.permitBatch.spender",
          "amount": "$inputs.amount",
          "expires_at": "$inputs.expiration",
          "sig_deadline": "$args.permitBatch.sigDeadline"
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
fn typed_data_permit2_permit_batch_on_disk_manifest() {
    let install = install_ok(PERMIT2_PERMIT_BATCH_ON_DISK_V3);
    assert_eq!(
        install["data"]["bundle_id"],
        "uniswap/permit2/permitBatch@1.0.0"
    );

    // EIP-712 message — `details` is a 2-element array; the ABI-derived wrap
    // rule supplies the outer `permitBatch` key. expiration / sigDeadline are
    // JSON numbers (Time over u64) — the typed-data path carries them as
    // numbers, so `expires_at` decodes cleanly (unlike the calldata path).
    let message = json!({
        "details": [
            {
                "token": USDC_MAINNET,
                "amount": "1000",
                "expiration": 1_738_001_800_u64,
                "nonce": "0"
            },
            {
                "token": USDT_MAINNET,
                "amount": "2000",
                "expiration": 1_738_001_900_u64,
                "nonce": "1"
            }
        ],
        "spender": SPENDER,
        "sigDeadline": 1_738_002_000_u64
    });
    let input = typed_data_input(1, PERMIT2, "PermitBatch", "Permit2", message, SIGNER);

    let out = declarative_route_typed_data_v3_json(input);
    let parsed: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["ok"], true, "route failed: {parsed}");
    assert_eq!(
        parsed["data"]["decoder_id"], "uniswap/permit2/permitBatch@1.0.0",
        "{parsed}"
    );

    // OffchainSig nature carrying the Permit2 domain + sigDeadline.
    let action0 = &parsed["data"]["actions"][0];
    assert_eq!(
        action0["meta"]["nature"]["kind"], "offchain_sig",
        "{parsed}"
    );
    assert_eq!(
        action0["meta"]["nature"]["domain"]["name"], "Permit2",
        "{parsed}"
    );

    // array_emit → Multicall with 2 permit2_sign_allowance actions.
    let body = &action0["body"];
    assert_eq!(body["domain"], "multicall", "{parsed}");
    let inner = body["actions"].as_array().expect("inner actions array");
    assert_eq!(inner.len(), 2, "{parsed}");

    // element-0 — token = USDC, amount = 1000, per-element expiration.
    assert_eq!(inner[0]["action"], "permit2_sign_allowance", "{parsed}");
    assert_eq!(
        inner[0]["token"]["key"]["address"], USDC_MAINNET,
        "{parsed}"
    );
    assert_eq!(inner[0]["amount"], "0x3e8", "{parsed}"); // 1000
    assert_eq!(inner[0]["expires_at"], 1_738_001_800_u64, "{parsed}");
    // batch-level spender / sig_deadline shared via $args in per-item body.
    assert_eq!(inner[0]["spender"], SPENDER, "{parsed}");
    assert_eq!(inner[0]["sig_deadline"], 1_738_002_000_u64, "{parsed}");
    // nonce LiveField default applied (word 0, bit 0).
    assert_eq!(inner[0]["nonce"]["value"], json!(["0x0", 0]), "{parsed}");

    // element-1 — DIFFERENT token + amount + expiration prove per-element bind.
    assert_eq!(
        inner[1]["token"]["key"]["address"], USDT_MAINNET,
        "{parsed}"
    );
    assert_eq!(inner[1]["amount"], "0x7d0", "{parsed}"); // 2000
    assert_eq!(inner[1]["expires_at"], 1_738_001_900_u64, "{parsed}");
    assert_eq!(inner[1]["spender"], SPENDER, "{parsed}");
}
