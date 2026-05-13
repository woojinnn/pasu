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

    // SWAP_EXACT_IN's `params` tuple should carry inner-field names through
    // the JSON-ABI path (`currencyIn`, `path`, `minHopPriceX36`, `amountIn`,
    // `amountOutMinimum`). Those names live in the arg's `components` so the
    // renderer can surface them as `(field: value, …)`.
    let swap_args = steps[0].args.as_ref().unwrap();
    assert_eq!(swap_args[0].name, "params");
    let params_components = &swap_args[0].components;
    let component_names: Vec<&str> = params_components.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        component_names,
        [
            "currencyIn",
            "path",
            "minHopPriceX36",
            "amountIn",
            "amountOutMinimum",
        ]
    );
    // PathKey field names propagate one level deeper.
    let path_param = params_components
        .iter()
        .find(|p| p.name == "path")
        .expect("path component present");
    let path_field_names: Vec<&str> = path_param
        .components
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        path_field_names,
        [
            "intermediateCurrency",
            "fee",
            "tickSpacing",
            "hooks",
            "hookData",
        ]
    );
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
