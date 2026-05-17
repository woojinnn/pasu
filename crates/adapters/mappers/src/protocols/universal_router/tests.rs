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
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::SwapMode;
use policy_engine::action::envelope::{Action, Category};
use policy_engine::action::{Address, DecimalString};

use crate::mapper::{MapContext, Mapper};
use crate::EmptyTokenRegistry;

use super::{
    UrSweepMapper, UrTransferMapper, UrUnwrapWethMapper, UrV2SwapExactInMapper,
    UrV2SwapExactOutMapper, UrV3SwapExactInMapper, UrV3SwapExactOutMapper, UrV4SwapMapper,
    UrWrapEthMapper,
};

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
    // Use PAY_PORTION (opcode 0x06) which is *not* yet pre-decoded. The
    // splitter still emits a SubCall, but its `decoded` field stays None —
    // the downstream pipeline would need a fallback path (or a future
    // mapper) to handle it.
    let pay_portion_input = {
        let func = AbiFunction::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(AlloyAddress::from([0x11; 20])),
            DynSolValue::Address(AlloyAddress::from([0x22; 20])),
            DynSolValue::Uint(U256::from(50u64), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    };
    let calldata = encode_execute(&[0x06], &[pay_portion_input]);

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
        "PAY_PORTION (not yet migrated) should leave SubCall.decoded as None"
    );
}

// ---------------------------------------------------------------------------
// SWEEP / TRANSFER (router → recipient) — Phase 4b.
// ---------------------------------------------------------------------------

/// Encode `(address, address, uint256)` for SWEEP/TRANSFER opcodes.
fn encode_addr_addr_uint(a: [u8; 20], b: [u8; 20], v: u128) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,address,uint256)").unwrap();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(a)),
        DynSolValue::Address(AlloyAddress::from(b)),
        DynSolValue::Uint(U256::from(v), 256),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

#[test]
fn sweep_splits_and_maps_to_transfer_min() {
    let token = [0x33; 20];
    let recipient_bytes = [0x44; 20];
    let amount_min: u128 = 1_000;
    let input = encode_addr_addr_uint(token, recipient_bytes, amount_min);
    let calldata = encode_execute(&[0x04], &[input]);

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
    let decoded = sub_calls[0].decoded.as_ref().expect("SWEEP pre-decoded");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/SWEEP");

    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let envelopes = UrSweepMapper::new().map(&ctx, decoded).unwrap();
    let Action::Transfer(t) = &envelopes[0].action else {
        panic!("expected Transfer");
    };
    // SWEEP from = router (ctx.to).
    assert_eq!(t.from, router);
    assert_eq!(t.token.amount.kind, AmountKind::Min);
    assert_eq!(t.token.amount.value.as_ref().unwrap().to_string(), "1000");
}

#[test]
fn transfer_splits_and_maps_to_transfer_exact() {
    let token = [0x55; 20];
    let recipient_bytes = [0x66; 20];
    let value_amt: u128 = 7_500;
    let input = encode_addr_addr_uint(token, recipient_bytes, value_amt);
    let calldata = encode_execute(&[0x05], &[input]);

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
    let decoded = sub_calls[0].decoded.as_ref().expect("TRANSFER pre-decoded");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/TRANSFER");

    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let envelopes = UrTransferMapper::new().map(&ctx, decoded).unwrap();
    let Action::Transfer(t) = &envelopes[0].action else {
        panic!("expected Transfer");
    };
    assert_eq!(t.token.amount.kind, AmountKind::Exact);
    assert_eq!(t.token.amount.value.as_ref().unwrap().to_string(), "7500");
}

// ---------------------------------------------------------------------------
// V2 swap — Phase 4b.
// ---------------------------------------------------------------------------

/// Encode `(address, uint256, uint256, address[], bool)` for V2 swap opcodes.
fn encode_v2_swap_input(
    recipient: [u8; 20],
    amt1: u128,
    amt2: u128,
    path: &[[u8; 20]],
    payer_is_user: bool,
) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,uint256,uint256,address[],bool)").unwrap();
    let path_vals: Vec<DynSolValue> = path
        .iter()
        .map(|a| DynSolValue::Address(AlloyAddress::from(*a)))
        .collect();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(recipient)),
        DynSolValue::Uint(U256::from(amt1), 256),
        DynSolValue::Uint(U256::from(amt2), 256),
        DynSolValue::Array(path_vals),
        DynSolValue::Bool(payer_is_user),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

#[test]
fn v2_swap_exact_in_emits_swap_action() {
    let weth = [0xc0; 20];
    let usdc = [0xa0; 20];
    let amount_in: u128 = 10_000;
    let amount_out_min: u128 = 9_500;
    let input = encode_v2_swap_input([0x77; 20], amount_in, amount_out_min, &[weth, usdc], true);
    let calldata = encode_execute(&[0x08], &[input]);

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
    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("V2_SWAP_EXACT_IN pre-decoded");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/V2_SWAP_EXACT_IN");

    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let envelopes = UrV2SwapExactInMapper::new().map(&ctx, decoded).unwrap();
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap");
    };
    assert_eq!(s.swap_mode, SwapMode::ExactIn);
    assert_eq!(s.fee_bps, Some(30));
    assert_eq!(s.input_token.amount.kind, AmountKind::Exact);
    assert_eq!(s.output_token.amount.kind, AmountKind::Min);
    assert_eq!(
        s.input_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(weth))
    );
    assert_eq!(
        s.output_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdc))
    );
}

#[test]
fn v2_swap_exact_out_emits_swap_action() {
    let weth = [0xc0; 20];
    let usdc = [0xa0; 20];
    let amount_out: u128 = 1_000;
    let amount_in_max: u128 = 2_000;
    let input = encode_v2_swap_input([0x77; 20], amount_out, amount_in_max, &[weth, usdc], true);
    let calldata = encode_execute(&[0x09], &[input]);

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
    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("V2_SWAP_EXACT_OUT pre-decoded");
    let envelopes = UrV2SwapExactOutMapper::new()
        .map(
            &map_ctx(&user, &router, &value, &EmptyTokenRegistry),
            decoded,
        )
        .unwrap();
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap");
    };
    assert_eq!(s.swap_mode, SwapMode::ExactOut);
    assert_eq!(s.input_token.amount.kind, AmountKind::Max);
    assert_eq!(s.output_token.amount.kind, AmountKind::Exact);
}

// ---------------------------------------------------------------------------
// V3 swap — Phase 4b. Packed path = addr(20) | fee(3) | addr(20) | …
// ---------------------------------------------------------------------------

fn encode_v3_swap_input(
    recipient: [u8; 20],
    amt1: u128,
    amt2: u128,
    path_bytes: &[u8],
    payer_is_user: bool,
) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,uint256,uint256,bytes,bool)").unwrap();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(recipient)),
        DynSolValue::Uint(U256::from(amt1), 256),
        DynSolValue::Uint(U256::from(amt2), 256),
        DynSolValue::Bytes(path_bytes.to_vec()),
        DynSolValue::Bool(payer_is_user),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Build a packed V3 single-hop path: `addr_in(20) | fee(3) | addr_out(20)`.
fn pack_v3_path(token_in: [u8; 20], fee_pips: u32, token_out: [u8; 20]) -> Vec<u8> {
    let mut path = Vec::with_capacity(20 + 3 + 20);
    path.extend_from_slice(&token_in);
    path.push(((fee_pips >> 16) & 0xff) as u8);
    path.push(((fee_pips >> 8) & 0xff) as u8);
    path.push((fee_pips & 0xff) as u8);
    path.extend_from_slice(&token_out);
    path
}

#[test]
fn v3_swap_exact_in_emits_swap_action_with_fee_bps() {
    let usdt = [0xda; 20];
    let usdc = [0xa0; 20];
    // 500 pips = 0.05%. UR encodes fee in hundredths of bps; mapper divides
    // by 100 → expects 5 bps in the surfaced fee_bps.
    let path = pack_v3_path(usdt, 500, usdc);
    let amount_in: u128 = 6_531_525;
    let amount_out_min: u128 = 6_497_371;
    let input = encode_v3_swap_input([0x77; 20], amount_in, amount_out_min, &path, true);
    let calldata = encode_execute(&[0x00], &[input]);

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
    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("V3_SWAP_EXACT_IN pre-decoded");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/V3_SWAP_EXACT_IN");

    let envelopes = UrV3SwapExactInMapper::new()
        .map(
            &map_ctx(&user, &router, &value, &EmptyTokenRegistry),
            decoded,
        )
        .unwrap();
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap");
    };
    assert_eq!(s.fee_bps, Some(5), "500 pips → 5 bps");
    assert_eq!(
        s.input_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdt))
    );
    assert_eq!(
        s.output_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdc))
    );
}

#[test]
fn v3_swap_exact_out_reverses_path_endpoints() {
    let usdt = [0xda; 20];
    let usdc = [0xa0; 20];
    // For exact-out, the packed path is reversed (output first, input last).
    // pack_v3_path(usdc, 500, usdt) → parser sees token_in=usdc, token_out=usdt,
    // but the mapper relabels: input=usdt, output=usdc.
    let path = pack_v3_path(usdc, 3000, usdt);
    let amount_out: u128 = 1_000;
    let amount_in_max: u128 = 2_000;
    let input = encode_v3_swap_input([0x77; 20], amount_out, amount_in_max, &path, true);
    let calldata = encode_execute(&[0x01], &[input]);

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
    let decoded = sub_calls[0]
        .decoded
        .as_ref()
        .expect("V3_SWAP_EXACT_OUT pre-decoded");

    let envelopes = UrV3SwapExactOutMapper::new()
        .map(
            &map_ctx(&user, &router, &value, &EmptyTokenRegistry),
            decoded,
        )
        .unwrap();
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap");
    };
    assert_eq!(s.swap_mode, SwapMode::ExactOut);
    // Confirm the input is the originally-second token in the packed path
    // (usdt), and output is the originally-first (usdc) — the relabel.
    assert_eq!(
        s.input_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdt))
    );
    assert_eq!(
        s.output_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdc))
    );
    assert_eq!(s.fee_bps, Some(30), "3000 pips → 30 bps");
}

// ---------------------------------------------------------------------------
// V4_SWAP — Phase 4c. The opcode wraps a nested `(bytes actions, bytes[]
// params)` stream that gets dispatched against V4_ROUTER_TABLE. The mapper
// builds a SwapAction per inner swap action and patches the recipient from
// any trailing TAKE step.
// ---------------------------------------------------------------------------

/// Encode the inner V4_SWAP_EXACT_IN_SINGLE params (mainnet 4-field shape):
/// `(poolKey, zeroForOne, amountIn, amountOutMinimum, hookData)` wrapped in
/// an outer tuple so the dispatcher sees a single named-param arg.
fn encode_v4_swap_exact_in_single_params(
    currency0: [u8; 20],
    currency1: [u8; 20],
    fee_pips: u32,
    zero_for_one: bool,
    amount_in: u128,
    amount_out_min: u128,
) -> Vec<u8> {
    let func = AbiFunction::parse(
        "step(((address,address,uint24,int24,address),bool,uint128,uint128,bytes))",
    )
    .unwrap();
    let pool_key = DynSolValue::Tuple(vec![
        DynSolValue::Address(AlloyAddress::from(currency0)),
        DynSolValue::Address(AlloyAddress::from(currency1)),
        DynSolValue::Uint(U256::from(fee_pips), 24),
        DynSolValue::Int(alloy_primitives::I256::ONE, 24),
        DynSolValue::Address(AlloyAddress::ZERO),
    ]);
    let params = DynSolValue::Tuple(vec![
        pool_key,
        DynSolValue::Bool(zero_for_one),
        DynSolValue::Uint(U256::from(amount_in), 128),
        DynSolValue::Uint(U256::from(amount_out_min), 128),
        DynSolValue::Bytes(Vec::new()),
    ]);
    let raw = func.abi_encode_input(&[params]).unwrap();
    raw[4..].to_vec()
}

fn encode_v4_settle_params(currency: [u8; 20], amount: u128, payer_is_user: bool) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,uint256,bool)").unwrap();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(currency)),
        DynSolValue::Uint(U256::from(amount), 256),
        DynSolValue::Bool(payer_is_user),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

fn encode_v4_take_params(currency: [u8; 20], recipient: [u8; 20], amount: u128) -> Vec<u8> {
    let func = AbiFunction::parse("step(address,address,uint256)").unwrap();
    let values = vec![
        DynSolValue::Address(AlloyAddress::from(currency)),
        DynSolValue::Address(AlloyAddress::from(recipient)),
        DynSolValue::Uint(U256::from(amount), 256),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Encode the V4_SWAP outer wrapper: `(bytes actions, bytes[] params)`.
fn encode_v4_swap_outer(actions: &[u8], params: &[Vec<u8>]) -> Vec<u8> {
    let func = AbiFunction::parse("step(bytes,bytes[])").unwrap();
    let values = vec![
        DynSolValue::Bytes(actions.to_vec()),
        DynSolValue::Array(
            params
                .iter()
                .map(|b| DynSolValue::Bytes(b.clone()))
                .collect(),
        ),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

#[test]
fn v4_swap_emits_swap_action_with_take_recipient_patch() {
    // Inner V4 action sequence: [SWAP_EXACT_IN_SINGLE, SETTLE, TAKE].
    // V4 swap params don't carry a recipient → mapper defaults to ctx.from,
    // then patches it with TAKE's recipient.
    let usdt = [0xda; 20];
    let usdc = [0xa0; 20];
    let real_recipient = [0x55; 20];
    let amount_in: u128 = 6_531_525;
    let amount_out_min: u128 = 6_497_371;

    let swap_params =
        encode_v4_swap_exact_in_single_params(usdt, usdc, 500, true, amount_in, amount_out_min);
    let settle_params = encode_v4_settle_params(usdt, amount_in, true);
    let take_params = encode_v4_take_params(usdc, real_recipient, amount_out_min);

    let v4_swap_input = encode_v4_swap_outer(
        &[0x06, 0x0b, 0x0e],
        &[swap_params, settle_params, take_params],
    );
    let calldata = encode_execute(&[0x10], &[v4_swap_input]);

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
        .expect("V4_SWAP must be pre-decoded by the splitter");
    assert_eq!(decoded.decoder_id.as_str(), "uniswap-ur/V4_SWAP");

    let tr = EmptyTokenRegistry;
    let ctx = map_ctx(&user, &router, &value, &tr);
    let envelopes = UrV4SwapMapper::new().map(&ctx, decoded).unwrap();
    assert_eq!(envelopes.len(), 1);
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap action");
    };
    assert_eq!(s.swap_mode, SwapMode::ExactIn);

    // zeroForOne=true with currency0=USDT, currency1=USDC means swap goes
    // USDT → USDC.
    assert_eq!(
        s.input_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdt))
    );
    assert_eq!(
        s.output_token.asset.address.as_ref().unwrap().to_string(),
        format!("0x{}", hex::encode(usdc))
    );

    // Fee 500 pips / 100 = 5 bps.
    assert_eq!(s.fee_bps, Some(5));

    // TAKE recipient patching: the swap defaulted to ctx.from (user), then
    // got patched to TAKE's recipient.
    let expected_recipient = Address::from_str(&format!("0x{}", hex::encode(real_recipient)))
        .expect("hex address parses");
    assert_eq!(
        s.recipient, expected_recipient,
        "TAKE recipient should overwrite the default ctx.from"
    );
}

#[test]
fn v4_swap_without_take_keeps_default_recipient() {
    // SWAP without a TAKE step (degenerate but legal — settlement could be
    // handled out-of-band). The mapper should leave the default ctx.from.
    let usdt = [0xda; 20];
    let usdc = [0xa0; 20];
    let swap_params = encode_v4_swap_exact_in_single_params(usdt, usdc, 500, true, 1_000, 950);
    let v4_swap_input = encode_v4_swap_outer(&[0x06], &[swap_params]);
    let calldata = encode_execute(&[0x10], &[v4_swap_input]);

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
    let decoded = sub_calls[0].decoded.as_ref().unwrap();
    let envelopes = UrV4SwapMapper::new()
        .map(
            &map_ctx(&user, &router, &value, &EmptyTokenRegistry),
            decoded,
        )
        .unwrap();
    let Action::Swap(s) = &envelopes[0].action else {
        panic!("expected Swap");
    };
    assert_eq!(s.recipient, user, "no TAKE → recipient stays as ctx.from");
}
