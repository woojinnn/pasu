use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, HostCapabilities, MockAdapterRegistry, MockApprovals, MockOracle, MockPortfolio,
    Pipeline, PolicyEngine, RequestKind, Token, TransactionRequest, Verdict,
    Adapter,
};
use policy_engine_adapter_uniswap_v2::{
    encode_swap_exact_eth_for_tokens, encode_swap_exact_tokens_for_tokens,
    native_eth, SwapExactETHForTokensParams, SwapExactTokensForTokensParams,
    UniswapV2SwapExactETHForTokensAdapter, UniswapV2SwapExactTokensForTokensAdapter,
    UNISWAP_V2_ROUTER_MAINNET,
};
use policy_engine_adapter_uniswap_v3::{
    encode_exact_input_single, ExactInputSingleParams, UniswapV3MulticallAdapter,
    SWAP_ROUTER_MAINNET, encode_multicall_deadline,
};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_MAX_FRACTION: &str = include_str!("../../../policies/swap/max-fraction-of-balance-2000-bps.cedar");
const POLICY_ALLOWANCE: &str = include_str!("../../../policies/swap/allowance-must-cover-input.cedar");

const FROM: &str = "0x0000000000000000000000000000000000000001";
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

fn from_address() -> Address {
    Address::new(FROM).unwrap()
}

fn full_oracle() -> MockOracle {
    MockOracle::new().with_simple_price(&usdt(), "1.0000", 5)
}

fn v2_registry() -> MockAdapterRegistry {
    MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV2SwapExactTokensForTokensAdapter::new()))
}

fn v2_eth_registry() -> MockAdapterRegistry {
    MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV2SwapExactETHForTokensAdapter::new()))
}

fn v3_multicall_registry() -> MockAdapterRegistry {
    MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV3MulticallAdapter::new()))
}

fn v2_swap_tx(amount_in: U256) -> TransactionRequest {
    let params = SwapExactTokensForTokensParams {
        amount_in,
        amount_out_min: U256::ZERO,
        path: vec![
            AlloyAddress::from_str(USDT).unwrap(),
            AlloyAddress::from_str(WETH).unwrap(),
        ],
        to: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
    };
    TransactionRequest {
        chain_id: 1,
        from: from_address(),
        to: Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_swap_exact_tokens_for_tokens(&params),
        gas: None,
        nonce: None,
    }
}

fn v2_eth_swap_tx(amount_in_wei: U256) -> TransactionRequest {
    let params = SwapExactETHForTokensParams {
        amount_out_min: U256::ZERO,
        path: vec![
            AlloyAddress::from_str(WETH).unwrap(),
            AlloyAddress::from_str(USDT).unwrap(),
        ],
        to: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
    };
    TransactionRequest {
        chain_id: 1,
        from: from_address(),
        to: Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        value_wei: amount_in_wei.to_string(),
        data: encode_swap_exact_eth_for_tokens(&params),
        gas: None,
        nonce: None,
    }
}

fn v3_multicall_tx(first_input: U256, second_input: U256) -> TransactionRequest {
    fn v3_swap(amount_in: U256) -> Vec<u8> {
        let params = ExactInputSingleParams {
            token_in: AlloyAddress::from_str(USDT).unwrap(),
            token_out: AlloyAddress::from_str(WETH).unwrap(),
            fee: 3000,
            recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
            deadline: U256::from(9_999_999_999u64),
            amount_in,
            amount_out_minimum: U256::ZERO,
            sqrt_price_limit_x96: U256::ZERO,
        };
        encode_exact_input_single(&params)
    }

    TransactionRequest {
        chain_id: 1,
        from: from_address(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_multicall_deadline(U256::from(9_999_999_999u64), vec![v3_swap(first_input), v3_swap(second_input)]),
        gas: None,
        nonce: None,
    }
}

#[test]
fn balance_fraction_deny_when_fraction_exceeds_20_percent() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION]).unwrap();
    let oracle = full_oracle();
    let pf = MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let host = HostCapabilities::builder(&oracle).with_portfolio(&pf).build();

    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(300_000_000u64));

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/max-fraction-of-balance-2000-bps");
            assert!(matches!(matched[0].origin, RequestKind::Leaf { index: 0 }));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn balance_fraction_allows_when_fraction_is_under_20_percent() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION]).unwrap();
    let oracle = full_oracle();
    let pf = MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let host = HostCapabilities::builder(&oracle).with_portfolio(&pf).build();

    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(100_000_000u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn balance_fraction_policy_is_fail_open_without_portfolio() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION]).unwrap();
    let registry = v2_registry();
    let oracle = full_oracle();
    let host = HostCapabilities::new(&oracle);
    let pipe = Pipeline::new(&registry, host, &policies);

    let tx = v2_swap_tx(U256::from(300_000_000u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn allowance_warn_fires_when_allowance_does_not_cover_input() {
    let policies = PolicyEngine::from_sources([POLICY_ALLOWANCE]).unwrap();
    let oracle = full_oracle();
    let approvals = MockApprovals::new().with_allowance(
        &from_address(),
        &usdt(),
        &Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        U256::ZERO,
    );
    let host = HostCapabilities::builder(&oracle).with_approvals(&approvals).build();

    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(200_000_000u64));

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Warn(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/allowance-must-cover-input");
            assert!(matches!(matched[0].origin, RequestKind::Leaf { index: 0 }));
        }
        other => panic!("expected Verdict::Warn, got {other:?}"),
    }
}

#[test]
fn allowance_policy_passes_when_allowance_covers_input() {
    let policies = PolicyEngine::from_sources([POLICY_ALLOWANCE]).unwrap();
    let oracle = full_oracle();
    let approvals = MockApprovals::new().with_allowance(
        &from_address(),
        &usdt(),
        &Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        U256::from(250_000_000u64),
    );
    let host = HostCapabilities::builder(&oracle).with_approvals(&approvals).build();

    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(200_000_000u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn allowance_policy_skips_native_input_token_for_v2_eth_swap() {
    let policies = PolicyEngine::from_sources([POLICY_ALLOWANCE]).unwrap();
    let oracle = full_oracle();
    let approvals = MockApprovals::new().with_allowance(
        &from_address(),
        &native_eth(1),
        &Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        U256::from(1_000_000_000_000_000_000u128),
    );
    let host = HostCapabilities::builder(&oracle).with_approvals(&approvals).build();

    let adapter = UniswapV2SwapExactETHForTokensAdapter::new();
    let tx = v2_eth_swap_tx(U256::from(1_000_000_000_000_000_000u64));
    let request = adapter.into_request(&tx, &host).unwrap();
    assert!(request.context.get("currentAllowance").is_none());
    assert!(request.context.get("allowanceCoversInput").is_none());

    let registry = v2_eth_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn multicall_leaves_receive_capability_enrichment() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION, POLICY_ALLOWANCE]).unwrap();
    let oracle = full_oracle();
    let pf = MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let approvals = MockApprovals::new().with_allowance(
        &from_address(),
        &usdt(),
        &Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        U256::from(220_000_000u64),
    );
    let host = HostCapabilities::builder(&oracle)
        .with_portfolio(&pf)
        .with_approvals(&approvals)
        .build();

    let adapter = UniswapV3MulticallAdapter::new();
    let tx = v3_multicall_tx(U256::from(250_000_000u64), U256::from(100_000_000u64));

    let requests = adapter.into_requests(&tx, &host).unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|req| {
        req.context.get("actorBalanceInputToken").is_some()
            && req.context.get("currentAllowance").is_some()
            && req.context.get("inputFractionOfBalanceBps").is_some()
    }));

    let registry = v3_multicall_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/max-fraction-of-balance-2000-bps" && matches!(m.origin, RequestKind::Leaf { index: 0 })));
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/allowance-must-cover-input" && matches!(m.origin, RequestKind::Leaf { index: 0 })));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}
