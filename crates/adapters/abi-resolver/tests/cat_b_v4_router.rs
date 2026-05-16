//! Nested opcode dispatch — UR `V4_SWAP` opcode re-dispatching its inner V4Router
//! stream.
//!
//! Builds a UR `V4_SWAP` step's `(bytes actions, bytes[] params)` payload
//! with the canonical post-V4 swap sequence (`0x07 SWAP_EXACT_IN` →
//! `0x0b SETTLE` → `0x0e TAKE`) and verifies the V4Router opcode table +
//! engine produce three labelled, named-arg steps.

use abi_resolver::{
    decode::DecodedArg,
    subdecode::{
        opcode_stream::{dispatch, DecodedStep},
        protocols::v4_router::{extract_actions_and_params, V4_ROUTER_TABLE},
    },
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{Address, U256};

fn encode(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
    let func = Function::parse(&format!("step{sig}")).unwrap();
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

#[test]
fn v4_swap_action_stream_decodes_swap_settle_take() {
    // SWAP_EXACT_IN params (multi-hop, native ETH input, USDT intermediate,
    // USDC output — same shape the V4 frontend produced in QA).
    let usdt =
        Address::from_slice(&hex::decode("dac17f958d2ee523a2206206994597c13d831ec7").unwrap());
    let usdc =
        Address::from_slice(&hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap());
    let recipient = Address::from([0xaa; 20]);
    let native = Address::ZERO;

    // PathKey[0]: native -> USDT, PathKey[1]: USDT -> USDC.
    // Use unnamed tuple types for the encoding side — alloy's signature
    // parser doesn't accept named tuple fields inside an array literal.
    // The decoded side (V4_ROUTER_TABLE) keeps names; that's what we assert
    // against further down.
    let path = DynSolValue::Array(vec![
        DynSolValue::Tuple(vec![
            DynSolValue::Address(usdt),
            DynSolValue::Uint(U256::from(100u64), 24),
            DynSolValue::Int(alloy_primitives::I256::ONE, 24),
            DynSolValue::Address(Address::ZERO),
            DynSolValue::Bytes(Vec::new()),
        ]),
        DynSolValue::Tuple(vec![
            DynSolValue::Address(usdc),
            DynSolValue::Uint(U256::from(100u64), 24),
            DynSolValue::Int(alloy_primitives::I256::ONE, 24),
            DynSolValue::Address(Address::ZERO),
            DynSolValue::Bytes(Vec::new()),
        ]),
    ]);
    // Real V4 calldata encodes ExactInputParams via `abi.decode(input, (T))`,
    // i.e. a single dynamic-struct arg — the test must match. Wrap the 5
    // struct fields in a single tuple value and encode against a 1-arg
    // function whose only parameter is that tuple type.
    let swap_in_input = encode(
        "((address,(address,uint24,int24,address,bytes)[],uint256[],uint128,uint128))",
        vec![DynSolValue::Tuple(vec![
            DynSolValue::Address(native),
            path,
            DynSolValue::Array(Vec::new()),
            DynSolValue::Uint(U256::from(30_000_000_000_000u64), 128),
            DynSolValue::Uint(U256::from(1_000_000u64), 128),
        ])],
    );

    let settle_input = encode(
        "(address,uint256,bool)",
        vec![
            DynSolValue::Address(native),
            DynSolValue::Uint(U256::from(30_000_000_000_000u64), 256),
            DynSolValue::Bool(true),
        ],
    );

    let take_input = encode(
        "(address,address,uint256)",
        vec![
            DynSolValue::Address(usdc),
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(1_000_000u64), 256),
        ],
    );

    let actions = vec![0x07, 0x0b, 0x0e];
    let inputs = vec![swap_in_input, settle_input, take_input];

    let steps = dispatch(&actions, &inputs, &V4_ROUTER_TABLE);
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].name, "SWAP_EXACT_IN");
    assert_eq!(steps[1].name, "SETTLE");
    assert_eq!(steps[2].name, "TAKE");

    for step in &steps {
        assert!(
            step.args.is_some(),
            "step {} ({}) failed to ABI-decode: {:?}",
            step.index,
            step.name,
            step.error,
        );
    }

    // SETTLE args: currency, amount, payerIsUser
    let settle_args = steps[1].args.as_ref().unwrap();
    assert_eq!(settle_args[0].name, "currency");
    assert_eq!(settle_args[1].name, "amount");
    assert_eq!(settle_args[2].name, "payerIsUser");

    // TAKE args: currency, recipient, amount
    let take_args = steps[2].args.as_ref().unwrap();
    assert_eq!(take_args[0].name, "currency");
    assert_eq!(take_args[1].name, "recipient");
    assert_eq!(take_args[2].name, "amount");

    // SWAP_EXACT_IN's `params` tuple decodes positionally — JSON ABI was
    // removed so the dispatcher can fall back from the post-#497 (5-field)
    // shape to the mainnet (4-field) shape. The outer `params` name still
    // propagates because alloy's signature parser keeps function-arg names.
    let swap_args = steps[0].args.as_ref().unwrap();
    assert_eq!(swap_args[0].name, "params");
    let DynSolValue::Tuple(params_fields) = &swap_args[0].value else {
        panic!("expected SWAP_EXACT_IN params to decode as a tuple");
    };
    // Calldata above was encoded with the post-#497 5-field shape; verify the
    // dispatcher picked that signature first.
    assert_eq!(params_fields.len(), 5);
    let DynSolValue::Address(currency_in) = &params_fields[0] else {
        panic!("currencyIn should be address");
    };
    assert_eq!(
        *currency_in,
        Address::ZERO,
        "currencyIn should be native ETH"
    );
    let DynSolValue::Uint(amount_in, _) = &params_fields[3] else {
        panic!("amountIn should be uint");
    };
    assert_eq!(*amount_in, U256::from(30_000_000_000_000u64));
}

#[test]
fn v4_swap_exact_in_falls_back_to_pre_497_shape() {
    // Same SWAP_EXACT_IN action encoded with the **mainnet** shape (no
    // `minHopPriceX36` per-hop slippage array). The dispatcher tries the
    // post-#497 signature first, hits a buffer overrun, then falls back to
    // the pre-#497 signature and decodes cleanly.
    let usdc =
        Address::from_slice(&hex::decode("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap());
    let native = Address::ZERO;

    let path = DynSolValue::Array(vec![DynSolValue::Tuple(vec![
        DynSolValue::Address(usdc),
        DynSolValue::Uint(U256::from(7u64), 24),
        DynSolValue::Int(alloy_primitives::I256::ONE, 24),
        DynSolValue::Address(Address::ZERO),
        DynSolValue::Bytes(Vec::new()),
    ])]);
    // 4-field shape (no minHopPriceX36[]).
    let swap_in_input = encode(
        "((address,(address,uint24,int24,address,bytes)[],uint128,uint128))",
        vec![DynSolValue::Tuple(vec![
            DynSolValue::Address(native),
            path,
            DynSolValue::Uint(U256::from(6_531_525u64), 128),
            DynSolValue::Uint(U256::from(6_497_371u64), 128),
        ])],
    );

    let steps = dispatch(&[0x07], &[swap_in_input], &V4_ROUTER_TABLE);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].name, "SWAP_EXACT_IN");
    let args = steps[0]
        .args
        .as_ref()
        .expect("pre-#497 SWAP_EXACT_IN should decode via the fallback signature");
    let DynSolValue::Tuple(fields) = &args[0].value else {
        panic!("expected SWAP_EXACT_IN params to decode as a tuple");
    };
    assert_eq!(fields.len(), 4, "fallback shape has 4 fields, not 5");
    let DynSolValue::Uint(amount_in, _) = &fields[2] else {
        panic!("amountIn should be uint at index 2 in 4-field shape");
    };
    assert_eq!(*amount_in, U256::from(6_531_525u64));
    let DynSolValue::Uint(amount_out_min, _) = &fields[3] else {
        panic!("amountOutMinimum should be uint at index 3 in 4-field shape");
    };
    assert_eq!(*amount_out_min, U256::from(6_497_371u64));
}

#[test]
fn extract_actions_and_params_pulls_pair_from_v4_swap_step() {
    // Build a fake V4_SWAP DecodedStep whose args mimic what UR's table
    // produces: arg[0] = bytes named "actions", arg[1] = bytes[] named
    // "params". The extractor should hand both back as raw bytes for
    // re-dispatch.
    let actions_bytes = vec![0x07u8, 0x0b, 0x0e];
    let params_bytes = vec![vec![0x01u8], vec![0x02], vec![0x03]];

    let step = DecodedStep {
        index: 0,
        raw_byte: 0x10,
        opcode: 0x10,
        allow_revert: false,
        name: "V4_SWAP",
        args: Some(vec![
            DecodedArg {
                name: "actions".into(),
                sol_type: "bytes".into(),
                value: DynSolValue::Bytes(actions_bytes.clone()),
                components: Vec::new(),
            },
            DecodedArg {
                name: "params".into(),
                sol_type: "bytes[]".into(),
                value: DynSolValue::Array(
                    params_bytes
                        .iter()
                        .map(|b| DynSolValue::Bytes(b.clone()))
                        .collect(),
                ),
                components: Vec::new(),
            },
        ]),
        error: None,
        raw_input: Vec::new(),
    };

    let (actions_out, params_out) =
        extract_actions_and_params(&step).expect("V4_SWAP step should expose actions+params");
    assert_eq!(actions_out, actions_bytes);
    assert_eq!(params_out, params_bytes);
}
