//! Extra Dex policy battery — verifies the 4 additional policies under
//! `policies/dex/` evaluate correctly across V3 and V2 adapters.

use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, HostCapabilities, MockOracle, MockTransactionActionAdapterRegistry, Pipeline,
    PolicyEngine, Token, TransactionRequest, Verdict,
};
use policy_engine_adapters_bundle::uniswap_v2::{
    encode_swap_exact_tokens_for_tokens, SwapExactTokensForTokensParams,
    UniswapV2SwapExactTokensForTokensAdapter,
};
use policy_engine_adapters_bundle::uniswap_v3::{
    encode_exact_input_single, ExactInputSingleParams, UniswapV3ExactInputSingleAdapter,
    SWAP_ROUTER_MAINNET,
};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_FEE_CAP: &str = include_str!("../../../policies/dex/max-fee-bps-100.cedar");
const POLICY_ALLOWLIST: &str = include_str!("../../../policies/dex/uniswap-only-allowlist.cedar");
const POLICY_NO_ZERO_OUT: &str = include_str!("../../../policies/dex/no-zero-min-output.cedar");
const POLICY_USD_FLOOR: &str = include_str!("../../../policies/dex/min-output-usd-floor.cedar");

const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";
const V2_ROUTER: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

fn usdt_token() -> Token {
    Token {
        chain_id: 1,
        address: Address::new(USDT).unwrap(),
        symbol: "USDT".into(),
        decimals: 6,
        is_native: false,
    }
}
fn weth_token() -> Token {
    Token {
        chain_id: 1,
        address: Address::new(WETH).unwrap(),
        symbol: "WETH".into(),
        decimals: 18,
        is_native: false,
    }
}

fn full_oracle() -> MockOracle {
    MockOracle::new()
        .with_simple_price(&usdt_token(), "1.0000", 5)
        .with_simple_price(&weth_token(), "3000.0000", 5)
}

fn v3_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV3ExactInputSingleAdapter::new()))
}

fn v2_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV2SwapExactTokensForTokensAdapter::new()))
}

fn v3_swap_tx(fee: u32, amount_in: U256, amount_out_min: U256) -> TransactionRequest {
    let params = ExactInputSingleParams {
        token_in: AlloyAddress::from_str(USDT).unwrap(),
        token_out: AlloyAddress::from_str(WETH).unwrap(),
        fee,
        recipient: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
        amount_in,
        amount_out_minimum: amount_out_min,
        sqrt_price_limit_x96: U256::ZERO,
    };
    TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_exact_input_single(&params),
        gas: None,
        nonce: None,
    }
}

fn v2_swap_tx(amount_in: U256, amount_out_min: U256) -> TransactionRequest {
    let params = SwapExactTokensForTokensParams {
        amount_in,
        amount_out_min,
        path: vec![
            AlloyAddress::from_str(USDT).unwrap(),
            AlloyAddress::from_str(WETH).unwrap(),
        ],
        to: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
    };
    TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(V2_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: encode_swap_exact_tokens_for_tokens(&params),
        gas: None,
        nonce: None,
    }
}

// ---------------------------------------------------------------------------
// Policy 1: max-fee-bps-100  (deny when maxFeeBps > 100)
// ---------------------------------------------------------------------------

#[test]
fn fee_cap_denies_v3_pool_with_300_bps() {
    let engine = PolicyEngine::from_sources([POLICY_FEE_CAP]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // V3 fee 30000 (raw) → 300 bps in our adapter (fee / 100). Above 100 cap.
    let tx = v3_swap_tx(30_000, U256::from(1_000_000u64), U256::from(0u64));
    assert!(matches!(pipe.evaluate(&tx).unwrap(), Verdict::Fail(_)));
}

#[test]
fn fee_cap_allows_v3_pool_with_30_bps() {
    let engine = PolicyEngine::from_sources([POLICY_FEE_CAP]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // V3 fee 3000 raw → 30 bps. Under cap.
    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::from(0u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn fee_cap_allows_v2_at_30_bps() {
    let engine = PolicyEngine::from_sources([POLICY_FEE_CAP]).unwrap();
    let registry = v2_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // V2 always emits 30 bps fee — well under cap.
    let tx = v2_swap_tx(U256::from(1_000_000u64), U256::from(0u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

// ---------------------------------------------------------------------------
// Policy 2: uniswap-only-allowlist  (deny when protocolId not in {v2, v3})
// ---------------------------------------------------------------------------

#[test]
fn allowlist_passes_uniswap_v3() {
    let engine = PolicyEngine::from_sources([POLICY_ALLOWLIST]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::from(0u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn allowlist_passes_uniswap_v2() {
    let engine = PolicyEngine::from_sources([POLICY_ALLOWLIST]).unwrap();
    let registry = v2_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v2_swap_tx(U256::from(1_000_000u64), U256::from(0u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

/// A test-only adapter that emits a Dex action with a non-allowlisted protocol id.
/// Validates that the allowlist policy actually fires for unknown protocols.
mod fake_protocol_adapter {
    use super::*;
    use policy_engine::{
        ActionAdapterError, ActionAdapterId, DexAction, DexFacts, DexTrace, LegacyAction,
        OracleRequirement, OracleRequirementKind, TransactionActionAdapter, TransactionMatchKey,
    };

    pub struct FakeProtocolAdapter;

    impl TransactionActionAdapter for FakeProtocolAdapter {
        fn id(&self) -> ActionAdapterId {
            ActionAdapterId::new("test/fake-protocol@0.0.1")
                .expect("static ActionAdapterId is well-formed")
        }
        fn match_keys(&self) -> Vec<TransactionMatchKey> {
            vec![TransactionMatchKey::exact(
                1,
                Address::new("0x000000000000000000000000000000000000beef").unwrap(),
                [0xde, 0xad, 0xbe, 0xef],
            )]
        }
        fn build_action(
            &self,
            tx: &TransactionRequest,
        ) -> Result<LegacyAction, ActionAdapterError> {
            let usdt = usdt_token();
            let weth = weth_token();
            Ok(LegacyAction::Dex(DexAction {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                value_wei: tx.value_wei.clone(),
                facts: DexFacts {
                    protocol_ids: vec!["fake-dex".into()],
                    input_tokens: vec![usdt.clone()],
                    output_tokens: vec![weth.clone()],
                    max_fee_bps: Some(0),
                    has_zero_min_output: false,
                    ..DexFacts::default()
                },
                oracle_requirements: vec![
                    OracleRequirement {
                        kind: OracleRequirementKind::Input,
                        token: usdt,
                        raw_amount: "1000000".into(),
                    },
                    OracleRequirement {
                        kind: OracleRequirementKind::MinOutput,
                        token: weth,
                        raw_amount: "330000000000000".into(),
                    },
                ],
                trace: DexTrace {
                    steps: vec!["fake-protocol aggregate dex action".into()],
                },
            }))
        }
    }
}

#[test]
fn allowlist_denies_unknown_protocol() {
    use fake_protocol_adapter::FakeProtocolAdapter;

    let engine = PolicyEngine::from_sources([POLICY_ALLOWLIST]).unwrap();
    let registry =
        MockTransactionActionAdapterRegistry::new().with_adapter(Arc::new(FakeProtocolAdapter));
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new("0x000000000000000000000000000000000000beef").unwrap(),
        value_wei: "0".into(),
        data: vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x00, 0x00],
        gas: None,
        nonce: None,
    };
    assert!(matches!(pipe.evaluate(&tx).unwrap(), Verdict::Fail(_)));
}

// ---------------------------------------------------------------------------
// Policy 3: no-zero-min-output  (warn when hasZeroMinOutput is true)
// ---------------------------------------------------------------------------

#[test]
fn zero_min_output_warns_v3() {
    let engine = PolicyEngine::from_sources([POLICY_NO_ZERO_OUT]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::ZERO);
    match pipe.evaluate(&tx).unwrap() {
        Verdict::Warn(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/no-zero-min-output");
        }
        v => panic!("expected Verdict::Warn, got {v:?}"),
    }
}

#[test]
fn zero_min_output_warns_v2() {
    let engine = PolicyEngine::from_sources([POLICY_NO_ZERO_OUT]).unwrap();
    let registry = v2_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v2_swap_tx(U256::from(1_000_000u64), U256::ZERO);
    assert!(matches!(pipe.evaluate(&tx).unwrap(), Verdict::Warn(_)));
}

#[test]
fn nonzero_min_output_passes() {
    let engine = PolicyEngine::from_sources([POLICY_NO_ZERO_OUT]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::from(1u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

// ---------------------------------------------------------------------------
// Policy 4: min-output-usd-floor  (deny when totalMinOutputUsd < $10)
// ---------------------------------------------------------------------------

#[test]
fn usd_floor_denies_dust_output() {
    let engine = PolicyEngine::from_sources([POLICY_USD_FLOOR]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // 1000 wei WETH → ~3 * 10^-15 USD, well below the $10 floor.
    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::from(1000u64));
    assert!(matches!(pipe.evaluate(&tx).unwrap(), Verdict::Fail(_)));
}

#[test]
fn usd_floor_passes_normal_swap() {
    let engine = PolicyEngine::from_sources([POLICY_USD_FLOOR]).unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // 0.01 WETH ≈ 30 USD min output, above floor.
    let amount_out_min = U256::from(10_000_000_000_000_000u64); // 0.01 WETH
    let tx = v3_swap_tx(3000, U256::from(50_000_000u64), amount_out_min);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn usd_floor_skips_when_oracle_missing() {
    // No prices in the oracle → totalMinOutputUsd is omitted by the lowering
    // step, so the policy's `context has totalMinOutputUsd` guard fails.
    let engine = PolicyEngine::from_sources([POLICY_USD_FLOOR]).unwrap();
    let registry = v3_registry();
    let oracle = MockOracle::new();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let tx = v3_swap_tx(3000, U256::from(1_000_000u64), U256::from(1000u64));
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

// ---------------------------------------------------------------------------
// Composition: stack all four policies on the same engine. Verify that:
//  - normal swap (V3, 30bps fee, $30 minOut) passes
//  - low-minOut warning surfaces alongside other passes
//  - high-fee deny-overrides the warn
// ---------------------------------------------------------------------------

#[test]
fn all_four_policies_compose_for_normal_swap() {
    let engine = PolicyEngine::from_sources([
        POLICY_FEE_CAP,
        POLICY_ALLOWLIST,
        POLICY_NO_ZERO_OUT,
        POLICY_USD_FLOOR,
    ])
    .unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    let amount_out_min = U256::from(10_000_000_000_000_000u64); // 0.01 WETH ≈ $30
    let tx = v3_swap_tx(3000, U256::from(50_000_000u64), amount_out_min);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn all_four_policies_high_fee_deny_overrides_zero_min_warn() {
    let engine = PolicyEngine::from_sources([
        POLICY_FEE_CAP,
        POLICY_ALLOWLIST,
        POLICY_NO_ZERO_OUT,
        POLICY_USD_FLOOR,
    ])
    .unwrap();
    let registry = v3_registry();
    let oracle = full_oracle();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&oracle), &engine);

    // High fee (300 bps) AND zero minOut → fail variant carries BOTH the
    // deny entry (fee-cap) and the warn entry (no-zero-min-output).
    let tx = v3_swap_tx(30_000, U256::from(1_000_000u64), U256::ZERO);
    match pipe.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/max-fee-bps-100"));
            assert!(matched
                .iter()
                .any(|m| m.policy_id == "user/no-zero-min-output"));
        }
        v => panic!("expected Verdict::Fail, got {v:?}"),
    }
}
