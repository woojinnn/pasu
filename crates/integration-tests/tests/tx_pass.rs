use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, HostCapabilities, MockOracle, MockTransactionActionAdapterRegistry, Pipeline,
    PolicyEngine, PolicyRequestOrigin, Token, TransactionRequest, Verdict,
};
use policy_engine_adapters_bundle::{default_registry, uniswap_v2, uniswap_v3};
use std::str::FromStr;

const POLICY_DEX_CAP: &str = include_str!("../../../policies/dex/total-input-usd-cap-500.cedar");
const POLICY_DEX_FEE_BPS: &str = include_str!("../../../policies/dex/max-fee-bps-100.cedar");

const POLICY_DEX_SHAPE: &str = r#"
@id("user/dex-shape-check")
@severity("deny")
@reason("dex context mismatch")
forbid (principal, action == Action::"dex", resource)
when {
    !(context.target == "0x7a250d5630b4cf539739df2c5dacb4c659f2488d"
    && context.valueWei == "0"
    && context.protocolIds == ["uniswap-v2"]
    && context.hasZeroMinOutput
    && context has totalInputUsd)
};
"#;

const POLICY_DEX_WARNING: &str = r#"
@id("user/dex-warning-input")
@severity("warn")
@reason("Dex action should warn on any priced input")
forbid (principal, action == Action::"dex", resource)
when {
  context has totalInputUsd && context.totalInputUsd.value.greaterThan(decimal("0.00"))
};
"#;

const POLICY_OTHER_BLOCKLIST: &str = r#"
@id("user/other-target-blocklist")
@severity("deny")
@reason("Other action target is blocklisted")
forbid (principal, action == Action::"other", resource)
when {
  context.target == "0x000000000000000000000000000000000000dead"
};
"#;

const V2_ROUTER: &str = uniswap_v2::UNISWAP_V2_ROUTER_MAINNET;
const V3_ROUTER: &str = uniswap_v3::SWAP_ROUTER_MAINNET;

const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0a0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";
const FROM: &str = "0x0000000000000000000000000000000000000001";

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

fn v2_swap_tx(amount_in: u64) -> TransactionRequest {
    let params = uniswap_v2::SwapExactTokensForTokensParams {
        amount_in: U256::from(amount_in),
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
        from: Address::new(FROM).unwrap(),
        to: Address::new(V2_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: uniswap_v2::encode_swap_exact_tokens_for_tokens(&params),
        gas: None,
        nonce: None,
    }
}

fn v3_exact_input_single(amount_in: u64, fee: u32) -> TransactionRequest {
    let params = uniswap_v3::ExactInputSingleParams {
        token_in: AlloyAddress::from_str(USDT).unwrap(),
        token_out: AlloyAddress::from_str(WETH).unwrap(),
        fee,
        recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
        amount_in: U256::from(amount_in),
        amount_out_minimum: U256::ZERO,
        sqrt_price_limit_x96: U256::ZERO,
    };
    v3_exact_input_single_tx(params)
}

fn v3_exact_input_single_tx(params: uniswap_v3::ExactInputSingleParams) -> TransactionRequest {
    TransactionRequest {
        chain_id: 1,
        from: Address::new(FROM).unwrap(),
        to: Address::new(V3_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: uniswap_v3::encode_exact_input_single(&params),
        gas: None,
        nonce: None,
    }
}

fn v3_multicall_tx() -> TransactionRequest {
    let input = v3_exact_input_single_tx_data(260_000_000, 3000);
    let input2 = v3_exact_input_single_tx_data(260_000_000, 3000);

    TransactionRequest {
        chain_id: 1,
        from: Address::new(FROM).unwrap(),
        to: Address::new(V3_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: uniswap_v3::encode_multicall_deadline(
            U256::from(9_999_999_999u64),
            vec![input, input2],
        ),
        gas: None,
        nonce: None,
    }
}

fn v3_exact_input_single_tx_data(amount_in: u64, fee: u32) -> Vec<u8> {
    let params = uniswap_v3::ExactInputSingleParams {
        token_in: AlloyAddress::from_str(USDT).unwrap(),
        token_out: AlloyAddress::from_str(WETH).unwrap(),
        fee,
        recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
        amount_in: U256::from(amount_in),
        amount_out_minimum: U256::ZERO,
        sqrt_price_limit_x96: U256::ZERO,
    };
    uniswap_v3::encode_exact_input_single(&params)
}

#[test]
fn single_v2_swap_under_cap_passes_and_dex_shape_is_valid() {
    let registry = default_registry();
    let policies = PolicyEngine::from_sources([POLICY_DEX_SHAPE, POLICY_DEX_CAP]).unwrap();
    let oracle = oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = v2_swap_tx(50_000_000);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn multicall_v3_aggregate_action_exceeds_total_input_and_fails_on_action_origin() {
    let registry = default_registry();
    let policies = PolicyEngine::from_sources([POLICY_DEX_CAP]).unwrap();
    let oracle = oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let verdict = pipe.evaluate(&v3_multicall_tx()).unwrap();
    match verdict {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/total-input-usd-cap-500");
            assert!(matches!(matched[0].origin, PolicyRequestOrigin::Action));
        }
        _ => panic!("expected Verdict::Fail, got {verdict:?}"),
    }
}

#[test]
fn dex_warning_and_fee_deny_share_action_origin() {
    let registry = default_registry();
    let policies = PolicyEngine::from_sources([POLICY_DEX_WARNING, POLICY_DEX_FEE_BPS]).unwrap();
    let oracle = oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let verdict = pipe
        .evaluate(&v3_exact_input_single(200_000_000u64, 30_000))
        .unwrap();
    match verdict {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 2);
            assert!(matched
                .iter()
                .all(|m| matches!(m.origin, PolicyRequestOrigin::Action)));
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/max-fee-bps-100"));
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/dex-warning-input"));
        }
        _ => panic!("expected Verdict::Fail, got {verdict:?}"),
    }
}

#[test]
fn no_match_tx_generates_other_action_and_allows_when_no_policies() {
    let registry = MockTransactionActionAdapterRegistry::new();
    let policies = PolicyEngine::from_sources(Vec::<&str>::new()).unwrap();
    let oracle = oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new(FROM).unwrap(),
        to: Address::new("0x000000000000000000000000000000000000beef").unwrap(),
        value_wei: "0".into(),
        data: vec![0xde, 0xad, 0xbe, 0xef],
        gas: None,
        nonce: None,
    };

    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn unknown_target_other_action_can_be_blocklisted_by_other_policy() {
    let registry = MockTransactionActionAdapterRegistry::new();
    let policies = PolicyEngine::from_sources([POLICY_OTHER_BLOCKLIST]).unwrap();
    let oracle = oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new(FROM).unwrap(),
        to: Address::new("0x000000000000000000000000000000000000dead").unwrap(),
        value_wei: "1230000000000000000".into(),
        data: vec![],
        gas: None,
        nonce: None,
    };

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/other-target-blocklist");
            assert!(matches!(matched[0].origin, PolicyRequestOrigin::Action));
        }
        v => panic!("expected Verdict::Fail, got {v:?}"),
    }
}
