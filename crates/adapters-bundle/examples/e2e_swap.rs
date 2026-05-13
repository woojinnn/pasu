//! Run with: `cargo run --example e2e_swap`
//!
//! Demonstrates the v0.1 pipeline end-to-end:
//!  - encode an `exactInputSingle` calldata for 200 USDT → WETH on mainnet
//!  - resolve through the mock adapter registry
//!  - decode + emit an aggregate Dex action
//!  - inject aggregate USD valuations from the mock oracle
//!  - evaluate the "max 100 USD per Dex input" Cedar policy
//!  - print the verdict

use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, HostCapabilities, MockOracle, Pipeline, PolicyEngine, Token, TransactionRequest,
};
use policy_engine_adapters_bundle::uniswap_v3::{
    encode_exact_input_single, ExactInputSingleParams, SWAP_ROUTER_MAINNET,
};
use policy_engine_adapters_bundle::default_registry;
use std::str::FromStr;

const USDT_ADDR: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH_ADDR: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

const POLICY_SRC: &str = include_str!("../../../policies/dex/max-input-usd-100.cedar");

fn main() {
    let usdt = Token {
        chain_id: 1,
        address: Address::new(USDT_ADDR).unwrap(),
        symbol: "USDT".into(),
        decimals: 6,
        is_native: false,
    };
    let weth = Token {
        chain_id: 1,
        address: Address::new(WETH_ADDR).unwrap(),
        symbol: "WETH".into(),
        decimals: 18,
        is_native: false,
    };

    let oracle = MockOracle::new()
        .with_simple_price(&usdt, "1.0000", 5)
        .with_simple_price(&weth, "3000.0000", 8);

    let registry = default_registry();

    let policies = PolicyEngine::from_sources([POLICY_SRC]).expect("policy file should parse");

    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    for (label, amount_in) in [
        ("50 USDT (under cap)", U256::from(50_000_000u64)),
        ("100 USDT (at cap)", U256::from(100_000_000u64)),
        ("200 USDT (over cap)", U256::from(200_000_000u64)),
    ] {
        let params = ExactInputSingleParams {
            token_in: AlloyAddress::from_str(USDT_ADDR).unwrap(),
            token_out: AlloyAddress::from_str(WETH_ADDR).unwrap(),
            fee: 3000,
            recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_in,
            amount_out_minimum: U256::ZERO,
            sqrt_price_limit_x96: U256::ZERO,
        };
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            value_wei: "0".into(),
            data: encode_exact_input_single(&params),
            gas: None,
            nonce: None,
        };

        let verdict = pipeline.evaluate(&tx).expect("pipeline should not error");
        println!("─── {label} ───");
        let label = match &verdict {
            policy_engine::Verdict::Pass => "Pass",
            policy_engine::Verdict::Warn(_) => "Warn",
            policy_engine::Verdict::Fail(_) => "Fail",
        };
        println!("  verdict  : {label}");
        for m in verdict.matched() {
            println!(
                "  matched  : {} {}",
                m.policy_id,
                m.reason.as_deref().unwrap_or("")
            );
        }
        println!();
    }
}
