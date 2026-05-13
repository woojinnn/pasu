use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    enrich_dex_action, Address, HostCapabilities, LegacyAction, MockApprovals, MockOracle,
    MockPortfolio, MockTransactionActionAdapterRegistry, Pipeline, PolicyEngine, PolicyRequest,
    PolicyRequestOrigin, Token, TransactionActionAdapter, TransactionRequest, Verdict,
};
use policy_engine_adapters_bundle::uniswap_v2::{
    encode_swap_exact_eth_for_tokens, encode_swap_exact_tokens_for_tokens, native_eth,
    SwapExactETHForTokensParams, SwapExactTokensForTokensParams,
    UniswapV2SwapExactETHForTokensAdapter, UniswapV2SwapExactTokensForTokensAdapter,
    UNISWAP_V2_ROUTER_MAINNET,
};
use policy_engine_adapters_bundle::uniswap_v3::{
    encode_exact_input_single, encode_multicall_deadline, ExactInputSingleParams,
    UniswapV3MulticallAdapter, SWAP_ROUTER_MAINNET,
};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_MAX_FRACTION: &str =
    include_str!("../../../policies/dex/max-input-fraction-of-portfolio-2000-bps.cedar");
const POLICY_ALLOWANCE: &str =
    include_str!("../../../policies/dex/allowance-must-cover-input.cedar");

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

fn v2_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV2SwapExactTokensForTokensAdapter::new()))
}

fn v2_eth_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV2SwapExactETHForTokensAdapter::new()))
}

fn v3_multicall_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV3MulticallAdapter::new()))
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
        data: encode_multicall_deadline(
            U256::from(9_999_999_999u64),
            vec![v3_swap(first_input), v3_swap(second_input)],
        ),
        gas: None,
        nonce: None,
    }
}

fn requests_from_adapter(
    adapter: &dyn TransactionActionAdapter,
    tx: &TransactionRequest,
    host: &HostCapabilities,
) -> Vec<PolicyRequest> {
    let mut action = adapter
        .build_action(tx)
        .expect("adapter should build an aggregate action");
    match &mut action {
        LegacyAction::Dex(dex) => enrich_dex_action(dex, host),
        other => panic!("expected adapter to emit Dex action, got {other:?}"),
    }
    policy_engine::lowering::requests_from_action(&action)
        .expect("Dex action should lower without host")
}

#[test]
fn balance_fraction_deny_when_fraction_exceeds_20_percent() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION]).unwrap();
    let oracle = full_oracle();
    let pf =
        MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let host = HostCapabilities::new(&oracle).with_portfolio(&pf);
    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(300_000_000u64));

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(
                matched[0].policy_id,
                "user/max-input-fraction-of-portfolio-2000-bps"
            );
            assert!(matches!(matched[0].origin, PolicyRequestOrigin::Action));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn balance_fraction_allows_when_fraction_is_under_20_percent() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION]).unwrap();
    let oracle = full_oracle();
    let pf =
        MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let host = HostCapabilities::new(&oracle).with_portfolio(&pf);
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
    let host = HostCapabilities::new(&oracle).with_approvals(&approvals);
    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(U256::from(200_000_000u64));

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Warn(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/allowance-must-cover-input");
            assert!(matches!(matched[0].origin, PolicyRequestOrigin::Action));
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
    let host = HostCapabilities::new(&oracle).with_approvals(&approvals);
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
    let host = HostCapabilities::new(&oracle).with_approvals(&approvals);
    let adapter = UniswapV2SwapExactETHForTokensAdapter::new();
    let tx = v2_eth_swap_tx(U256::from(1_000_000_000_000_000_000u64));
    let request = requests_from_adapter(&adapter, &tx, &host)
        .into_iter()
        .next()
        .expect("expected one aggregate request");
    assert_eq!(
        request
            .context
            .get("allowancesCoverInputs")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let registry = v2_eth_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn multicall_aggregate_action_receives_capability_enrichment() {
    let policies = PolicyEngine::from_sources([POLICY_MAX_FRACTION, POLICY_ALLOWANCE]).unwrap();
    let oracle = full_oracle();
    let pf =
        MockPortfolio::new().with_balance(&from_address(), &usdt(), U256::from(1_000_000_000u64));
    let approvals = MockApprovals::new().with_allowance(
        &from_address(),
        &usdt(),
        &Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        U256::from(220_000_000u64),
    );
    let host = HostCapabilities::new(&oracle)
        .with_portfolio(&pf)
        .with_approvals(&approvals);
    let adapter = UniswapV3MulticallAdapter::new();
    let tx = v3_multicall_tx(U256::from(250_000_000u64), U256::from(100_000_000u64));

    let requests = requests_from_adapter(&adapter, &tx, &host);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request
            .context
            .get("totalInputFractionOfPortfolioBps")
            .and_then(serde_json::Value::as_i64),
        Some(3500)
    );
    assert_eq!(
        request
            .context
            .get("allowancesCoverInputs")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );

    let registry = v3_multicall_registry();
    let pipe = Pipeline::new(&registry, host, &policies);
    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert!(matched.iter().any(|m| m.policy_id
                == "user/max-input-fraction-of-portfolio-2000-bps"
                && matches!(m.origin, PolicyRequestOrigin::Action)));
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/allowance-must-cover-input"
                    && matches!(m.origin, PolicyRequestOrigin::Action)));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}
