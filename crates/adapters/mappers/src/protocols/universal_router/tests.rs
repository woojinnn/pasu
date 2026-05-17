//! End-to-end pipeline tests: UR calldata → `UniversalRouterSplitter` →
//! `SubCall.decoded` → UR-specific mapper → `ActionEnvelope`.
//!
//! Phase 4a covers `WRAP_ETH` and `UNWRAP_WETH`. The tests build synthetic
//! UR `execute(commands, inputs)` calldata, run it through the splitter,
//! then dispatch each `SubCall.decoded` to the matching mapper and assert
//! the resulting envelope shape.

use std::str::FromStr as _;

use abi_resolver::splitter::universal_router::UniversalRouterSplitter;
use abi_resolver::{SplitContext, Splitter};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function as AbiFunction;
use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::action::envelope::{Action, Category};
use policy_engine::action::{Address, DecimalString};

use crate::mapper::{MapContext, Mapper};
use crate::EmptyTokenRegistry;

use super::{UrUnwrapWethMapper, UrWrapEthMapper};

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}
fn dec(s: &str) -> DecimalString {
    s.parse().unwrap()
}

/// Encode a synthetic UR `execute(bytes,bytes[])` calldata blob.
fn encode_execute(commands: &[u8], inputs: &[Vec<u8>]) -> Vec<u8> {
    let func = AbiFunction::parse("execute(bytes,bytes[])").unwrap();
    let values = vec![
        DynSolValue::Bytes(commands.to_vec()),
        DynSolValue::Array(
            inputs
                .iter()
                .map(|b| DynSolValue::Bytes(b.clone()))
                .collect(),
        ),
    ];
    func.abi_encode_input(&values).unwrap()
}

/// Encode `(address, uint256)` as the per-opcode raw input bytes (no
/// 4-byte selector — that's how UR's `inputs[i]` blobs are framed).
fn encode_address_uint256(a: [u8; 20], v: u128) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,uint256)").unwrap();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(a)),
        DynSolValue::Uint(U256::from(v), 256),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

fn map_ctx<'a>(
    from: &'a Address,
    to: &'a Address,
    value: &'a DecimalString,
    token_registry: &'a EmptyTokenRegistry,
) -> MapContext<'a> {
    MapContext {
        chain_id: 1,
        from,
        to,
        value_wei: value,
        block_timestamp: None,
        token_registry,
    }
}

#[test]
fn wrap_eth_splits_and_maps_to_wrap_action() {
    let recipient_bytes = [0x77; 20];
    let amount_min: u128 = 1_000_000_000_000_000; // 0.001 ETH
    let wrap_input = encode_address_uint256(recipient_bytes, amount_min);
    let calldata = encode_execute(&[0x0b], &[wrap_input]);

    let user = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let router = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let value = dec("0");
    let split_ctx = SplitContext {
        chain_id: 1,
        from: &user,
        to: &router,
        value_wei: &value,
        block_timestamp: None,
    };

    // Split → exactly one SubCall whose `decoded` carries the WRAP_ETH
    // pre-decoded call with the synthetic UR_WRAP_ETH_DECODER_ID.
    let splitter = UniversalRouterSplitter::uniswap_ur();
    let sub_calls = splitter.split(&split_ctx, &calldata).unwrap();
    assert_eq!(sub_calls.len(), 1);

    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("WRAP_ETH must be pre-decoded by the splitter");
    assert_eq!(
        decoded.decoder_id.as_str(),
        "uniswap-ur/WRAP_ETH",
        "splitter must tag the WRAP_ETH SubCall with the synthetic decoder id"
    );

    // Mapper → exactly one envelope of type Wrap.
    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let mapper = UrWrapEthMapper::new();
    let envelopes = mapper.map(&ctx, decoded).unwrap();
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    let Action::Wrap(wrap) = &envelopes[0].action else {
        panic!("expected Wrap action, got {:?}", envelopes[0].action);
    };
    // Recipient is a literal address (not a UR sentinel) so map_recipient
    // returns it unchanged.
    let expected_recipient = Address::from_str(&format!("0x{}", hex::encode(recipient_bytes)))
        .expect("hex address parses");
    assert_eq!(wrap.recipient, expected_recipient);
    assert_eq!(
        wrap.native_asset.amount.value.as_ref().unwrap().to_string(),
        amount_min.to_string()
    );
    assert_eq!(
        wrap.wrapped_asset
            .amount
            .value
            .as_ref()
            .unwrap()
            .to_string(),
        amount_min.to_string()
    );
}

#[test]
fn unwrap_weth_splits_and_maps_to_unwrap_action() {
    // recipient = 0x...01 sentinel ("the original msg.sender"). The mapper
    // must resolve it to ctx.from.
    let sentinel = {
        let mut bytes = [0u8; 20];
        bytes[19] = 0x01;
        bytes
    };
    let amount_min: u128 = 500_000_000_000_000;
    let unwrap_input = encode_address_uint256(sentinel, amount_min);
    let calldata = encode_execute(&[0x0c], &[unwrap_input]);

    let user = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let router = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let value = dec("0");
    let split_ctx = SplitContext {
        chain_id: 1,
        from: &user,
        to: &router,
        value_wei: &value,
        block_timestamp: None,
    };

    let splitter = UniversalRouterSplitter::uniswap_ur();
    let sub_calls = splitter.split(&split_ctx, &calldata).unwrap();
    assert_eq!(sub_calls.len(), 1);

    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("UNWRAP_WETH must be pre-decoded by the splitter");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/UNWRAP_WETH");

    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let mapper = UrUnwrapWethMapper::new();
    let envelopes = mapper.map(&ctx, decoded).unwrap();
    assert_eq!(envelopes.len(), 1);
    let Action::Unwrap(unwrap) = &envelopes[0].action else {
        panic!("expected Unwrap action, got {:?}", envelopes[0].action);
    };
    // Sentinel 0x...01 must resolve to ctx.from (user).
    assert_eq!(
        unwrap.recipient, user,
        "ACTION_MSG_SENDER (0x...01) must resolve to ctx.from"
    );
    assert_eq!(
        unwrap
            .wrapped_asset
            .amount
            .value
            .as_ref()
            .unwrap()
            .to_string(),
        amount_min.to_string()
    );
}

#[test]
fn unknown_opcode_leaves_subcall_decoded_none() {
    // Use SWEEP (opcode 0x04) which is *not* yet handled by Phase 4a's
    // pre_decode_for_opcode. The splitter still emits a SubCall, but its
    // `decoded` field stays None — the downstream pipeline would need a
    // fallback path (or a Phase 4b mapper) to handle it.
    let sweep_input = {
        let func = AbiFunction::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(AlloyAddress::from([0x11; 20])),
            DynSolValue::Address(AlloyAddress::from([0x22; 20])),
            DynSolValue::Uint(U256::from(10u64), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    };
    let calldata = encode_execute(&[0x04], &[sweep_input]);

    let user = addr("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let router = addr("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let value = dec("0");
    let split_ctx = SplitContext {
        chain_id: 1,
        from: &user,
        to: &router,
        value_wei: &value,
        block_timestamp: None,
    };

    let splitter = UniversalRouterSplitter::uniswap_ur();
    let sub_calls = splitter.split(&split_ctx, &calldata).unwrap();
    assert_eq!(sub_calls.len(), 1);
    assert!(
        sub_calls[0].decoded.is_none(),
        "SWEEP (not yet migrated) should leave SubCall.decoded as None"
    );
}
