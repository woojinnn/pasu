//! End-to-end coverage for composite router aggregation:
//!   outer router calldata → aggregate Dex action → unchanged Dex policies.

use alloy_primitives::{
    aliases::{I24, U24},
    Address as AlloyAddress, U256,
};
use alloy_sol_types::SolValue;
use policy_engine::{
    Address, HostCapabilities, MockOracle, Pipeline, PolicyEngine, Token, TransactionRequest,
    Verdict,
};
use policy_engine_adapters_bundle::uniswap_v3::{
    encode_exact_input_single, encode_multicall_deadline, ExactInputSingleParams,
    SWAP_ROUTER_MAINNET,
};
use policy_engine_adapters_bundle::{default_registry, universal_router};
use std::str::FromStr;

const POLICY_SRC: &str = include_str!("../../../policies/dex/max-input-usd-100.cedar");

const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

fn usdt() -> Token {
    Token {
        chain_id: 1,
        address: Address::new(USDT).unwrap(),
        symbol: "USDT".into(),
        decimals: 6,
        is_native: false,
    }
}

fn weth() -> Token {
    Token {
        chain_id: 1,
        address: Address::new(WETH).unwrap(),
        symbol: "WETH".into(),
        decimals: 18,
        is_native: false,
    }
}

fn oracle() -> MockOracle {
    MockOracle::new()
        .with_simple_price(&usdt(), "1.0000", 5)
        .with_simple_price(&weth(), "3000.0000", 8)
}

fn engine() -> PolicyEngine {
    PolicyEngine::from_sources([POLICY_SRC]).unwrap()
}

fn v3_exact_input_single(amount_in: u64) -> Vec<u8> {
    encode_exact_input_single(&ExactInputSingleParams {
        token_in: AlloyAddress::from_str(USDT).unwrap(),
        token_out: AlloyAddress::from_str(WETH).unwrap(),
        fee: 3000,
        recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
        amount_in: U256::from(amount_in),
        amount_out_minimum: U256::ZERO,
        sqrt_price_limit_x96: U256::ZERO,
    })
}

fn v3_path(token_a: &str, fee: u32, token_b: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(AlloyAddress::from_str(token_a).unwrap().as_slice());
    out.extend_from_slice(&fee.to_be_bytes()[1..4]);
    out.extend_from_slice(AlloyAddress::from_str(token_b).unwrap().as_slice());
    out
}

#[test]
fn v3_multicall_aggregate_dex_action_uses_existing_dex_policy() {
    let registry = default_registry();
    let oracle = oracle();
    let policies = engine();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);
    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_multicall_deadline(
            U256::from(9_999_999_999u64),
            vec![v3_exact_input_single(200_000_000)],
        ),
        gas: None,
        nonce: None,
    };

    let verdict = pipe.evaluate(&tx).unwrap();
    assert!(matches!(verdict, Verdict::Fail(_)));
    assert_eq!(verdict.matched()[0].policy_id, "user/max-input-usd-100");
}

#[test]
fn universal_router_v3_aggregate_dex_action_uses_existing_dex_policy() {
    let input = (
        AlloyAddress::from_str(RECIPIENT).unwrap(),
        U256::from(200_000_000u64),
        U256::ZERO,
        v3_path(USDT, 3000, WETH),
        true,
        Vec::<U256>::new(),
    )
        .abi_encode_sequence();
    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(universal_router::common::UNIVERSAL_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: universal_router::encode_execute(vec![0x00], vec![input]),
        gas: None,
        nonce: None,
    };

    let registry = default_registry();
    let oracle = oracle();
    let policies = engine();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);
    let verdict = pipe.evaluate(&tx).unwrap();
    assert!(matches!(verdict, Verdict::Fail(_)));
    assert_eq!(verdict.matched()[0].policy_id, "user/max-input-usd-100");
}

#[test]
fn universal_router_v4_aggregate_dex_action_uses_existing_dex_policy() {
    let pool_key = universal_router::PoolKey {
        currency0: AlloyAddress::from_str(USDT).unwrap(),
        currency1: AlloyAddress::from_str(WETH).unwrap(),
        fee: U24::from(3000u32),
        tickSpacing: I24::try_from(60i32).unwrap(),
        hooks: AlloyAddress::ZERO,
    };
    let swap_params = universal_router::V4ExactInputSingleParams {
        poolKey: pool_key,
        zeroForOne: true,
        amountIn: 200_000_000u128,
        amountOutMinimum: 0u128,
        minHopPriceX36: U256::ZERO,
        hookData: Vec::<u8>::new().into(),
    }
    .abi_encode_sequence();
    let v4_input = (
        vec![0x06u8, 0x0cu8, 0x0fu8],
        vec![
            swap_params,
            (
                AlloyAddress::from_str(USDT).unwrap(),
                U256::from(200_000_000u64),
            )
                .abi_encode_sequence(),
            (AlloyAddress::from_str(WETH).unwrap(), U256::ZERO).abi_encode_sequence(),
        ],
    )
        .abi_encode_sequence();
    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(universal_router::common::UNIVERSAL_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: universal_router::encode_execute(vec![0x10], vec![v4_input]),
        gas: None,
        nonce: None,
    };

    let registry = default_registry();
    let oracle = oracle();
    let policies = engine();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);
    let verdict = pipe.evaluate(&tx).unwrap();
    assert!(matches!(verdict, Verdict::Fail(_)));
    assert_eq!(verdict.matched()[0].policy_id, "user/max-input-usd-100");
}
