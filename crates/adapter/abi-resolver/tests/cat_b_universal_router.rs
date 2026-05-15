//! Opcode dispatch — Universal Router opcode dispatch integration tests.
//!
//! Builds a real `execute(bytes,bytes[],uint256)` calldata containing a
//! WRAP_ETH + V3_SWAP_EXACT_IN + UNWRAP_WETH sequence, then verifies the
//! opcode-dispatch engine + Uniswap UR table together decode the inner opcode steps.

use abi_resolver::{
    decode::DecodedCall,
    openchain::{OpenchainIndex, SignatureCandidate},
    resolver::{ResolveOutcome, Resolver},
    sourcify::SourcifyIndex,
    subdecode::{
        opcode_stream::dispatch,
        protocols::universal_router::{
            extract_commands_and_inputs, is_universal_router_execute, EXECUTE_DEADLINE_SELECTOR,
            UNISWAP_UR_TABLE,
        },
    },
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{Address, U256};

fn encode_input(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
    let func = Function::parse(&format!("step{sig}")).unwrap();
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Build a `(commands, inputs[])` pair for the canonical "swap then unwrap"
/// sequence used by the modern UR frontend when sending tokens for ETH:
///   0x0b WRAP_ETH        (recipient, amount)
///   0x00 V3_SWAP_EXACT_IN (recipient, amountIn, amountOutMin, path, payerIsUser)
///   0x0c UNWRAP_WETH     (recipient, amountMin)
fn sample_commands_and_inputs() -> (Vec<u8>, Vec<Vec<u8>>) {
    let recipient = DynSolValue::Address(Address::from([0xaa; 20]));
    let amount_in = DynSolValue::Uint(U256::from(1_000_000_000_u64), 256);
    let amount_min = DynSolValue::Uint(U256::from(99_u64), 256);
    let payer_is_user = DynSolValue::Bool(true);
    // V3 packed path: tokenA(20) | fee(3) | tokenB(20)
    let mut path = Vec::with_capacity(43);
    path.extend_from_slice(&[0x11; 20]);
    path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
    path.extend_from_slice(&[0x22; 20]);
    let path_value = DynSolValue::Bytes(path);

    let wrap_input = encode_input(
        "(address,uint256)",
        vec![recipient.clone(), amount_in.clone()],
    );
    let swap_input = encode_input(
        "(address,uint256,uint256,bytes,bool)",
        vec![
            recipient.clone(),
            amount_in,
            amount_min.clone(),
            path_value,
            payer_is_user,
        ],
    );
    let unwrap_input = encode_input("(address,uint256)", vec![recipient, amount_min]);

    let commands = vec![0x0b, 0x00, 0x0c];
    let inputs = vec![wrap_input, swap_input, unwrap_input];
    (commands, inputs)
}

fn execute_calldata(commands: Vec<u8>, inputs: Vec<Vec<u8>>, deadline: u64) -> Vec<u8> {
    let func = Function::parse("execute(bytes,bytes[],uint256)").unwrap();
    let inputs_value = DynSolValue::Array(
        inputs
            .into_iter()
            .map(DynSolValue::Bytes)
            .collect::<Vec<_>>(),
    );
    func.abi_encode_input(&[
        DynSolValue::Bytes(commands),
        inputs_value,
        DynSolValue::Uint(U256::from(deadline), 256),
    ])
    .unwrap()
}

fn seeded_resolver() -> Resolver {
    let mut openchain = OpenchainIndex::empty();
    openchain.insert(
        EXECUTE_DEADLINE_SELECTOR,
        SignatureCandidate {
            signature: "execute(bytes,bytes[],uint256)".into(),
            verified: true,
        },
    );
    Resolver::new(SourcifyIndex::empty(), openchain)
}

fn outer_decoded(resolver: &Resolver, calldata: &[u8]) -> DecodedCall {
    match resolver.resolve(1, &Address::from([0xee; 20]), calldata) {
        ResolveOutcome::Resolved(r) => r.decoded,
        other => panic!("expected execute() to resolve, got {other:?}"),
    }
}

#[test]
fn execute_outer_resolves_and_inner_steps_dispatch() {
    let (commands, inputs) = sample_commands_and_inputs();
    let calldata = execute_calldata(commands.clone(), inputs.clone(), 9_999_999_999);

    let resolver = seeded_resolver();
    let decoded = outer_decoded(&resolver, &calldata);
    assert_eq!(decoded.function_name, "execute");

    // Selector identification.
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&calldata[..4]);
    assert!(is_universal_router_execute(&selector));

    // Commands/inputs extraction.
    let (extracted_cmds, extracted_inputs) =
        extract_commands_and_inputs(&decoded).expect("execute should expose commands/inputs");
    assert_eq!(extracted_cmds, commands);
    assert_eq!(extracted_inputs, inputs);

    // opcode dispatch.
    let steps = dispatch(&extracted_cmds, &extracted_inputs, &UNISWAP_UR_TABLE);
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].name, "WRAP_ETH");
    assert_eq!(steps[1].name, "V3_SWAP_EXACT_IN");
    assert_eq!(steps[2].name, "UNWRAP_WETH");
    for step in &steps {
        assert!(
            step.args.is_some(),
            "step {} ({}) should ABI-decode against table schema, but errored: {:?}",
            step.index,
            step.name,
            step.error,
        );
        assert!(!step.allow_revert);
    }

    // Named parameters from the opcode table propagate into the decoded args
    // — UI no longer needs to fall back to `arg0/arg1/...`.
    let wrap_args = steps[0].args.as_ref().unwrap();
    assert_eq!(wrap_args[0].name, "recipient");
    assert_eq!(wrap_args[1].name, "amountMin");

    let v3_args = steps[1].args.as_ref().unwrap();
    let v3_arg_names: Vec<&str> = v3_args.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        v3_arg_names,
        [
            "recipient",
            "amountIn",
            "amountOutMin",
            "path",
            "payerIsUser"
        ]
    );
}

#[test]
fn allow_revert_high_bit_propagates_through_real_calldata() {
    let (mut commands, inputs) = sample_commands_and_inputs();
    commands[1] |= 0x80; // Mark V3_SWAP_EXACT_IN as allowRevert.
    let calldata = execute_calldata(commands.clone(), inputs.clone(), 9_999_999_999);

    let resolver = seeded_resolver();
    let decoded = outer_decoded(&resolver, &calldata);
    let (cmds, ins) = extract_commands_and_inputs(&decoded).unwrap();
    let steps = dispatch(&cmds, &ins, &UNISWAP_UR_TABLE);
    assert_eq!(steps[1].name, "V3_SWAP_EXACT_IN");
    assert!(steps[1].allow_revert);
    assert_eq!(steps[1].opcode, 0x00);
}

#[test]
fn extract_returns_none_when_args_dont_match_execute_shape() {
    // Build a fake decoded call with mismatched arg shapes.
    let decoded = DecodedCall {
        function_name: "approve".into(),
        signature: "approve(address,uint256)".into(),
        args: vec![],
    };
    assert!(extract_commands_and_inputs(&decoded).is_none());
}
