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
// #3 — no typed_data block → no_typed_data_mapper
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
