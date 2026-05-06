//! Demonstrate adapter lowering through `Pipeline` in three flavors:
//!
//! 1. Default path on an existing UniswapV3 adapter — no custom metadata.
//! 2. Custom adapter that overrides only `leaf_metadata`.
//! 3. Hand-built `PolicyRequest` for unit-testing the policy layer alone.

use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Action, Adapter, AdapterError, AdapterId, Address, HostCapabilities, MatchKey, MockAdapterRegistry,
    MockOracle, Pipeline, PipelineError, PolicyEngine, PolicyRequest, SwapAction, Token,
    TransactionRequest, Verdict,
};
use policy_engine_adapter_uniswap_v3::{
    decode_exact_input_single, encode_exact_input_single, ExactInputSingleParams,
    UniswapV3ExactInputSingleAdapter, SELECTOR_EXACT_INPUT_SINGLE, SWAP_ROUTER_MAINNET,
};
use serde_json::{json, Map, Value as JsonValue};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_TEXT: &str = include_str!("../../../policies/swap/max-swap-usd-100.cedar");
const POLICY_MATCHING_REQUEST: &str = r#"
@id("user/e2e-principal-action-resource")
@severity("deny")
@reason("swap request metadata should be stable in default path")
forbid (
    principal == Wallet::"0x0000000000000000000000000000000000000001",
    action == Action::"swap",
    resource == Protocol::"uniswap-v3",
) when { context has "inputAmount" };
"#;

const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

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

fn build_swap_tx(amount_in: U256) -> TransactionRequest {
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
    TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_exact_input_single(&params),
        gas: Some(200_000),
        nonce: Some(42),
    }
}

fn full_oracle() -> MockOracle {
    MockOracle::new()
        .with_simple_price(&usdt_token(), "1.0000", 5)
        .with_simple_price(&weth_token(), "3000.0000", 8)
}

fn uniswap_v3_registry() -> MockAdapterRegistry {
    MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV3ExactInputSingleAdapter::new()))
}

// ---------------------------------------------------------------------------
// (1) Default pipeline lowering path.
// ---------------------------------------------------------------------------

#[test]
fn default_into_request_produces_evaluable_policy_request() {
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_MATCHING_REQUEST]).unwrap();
    let registry = uniswap_v3_registry();
    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = build_swap_tx(U256::from(200_000_000u64));
    match pipeline.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/e2e-principal-action-resource");
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn default_into_request_path_evaluates_to_deny_at_200_usdt() {
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let registry = uniswap_v3_registry();
    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = build_swap_tx(U256::from(200_000_000u64));
    match pipeline.evaluate(&tx).unwrap() {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/max-swap-usd-100");
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn default_into_request_path_evaluates_to_allow_under_cap() {
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let registry = uniswap_v3_registry();
    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    let tx = build_swap_tx(U256::from(50_000_000u64));
    assert_eq!(pipeline.evaluate(&tx).unwrap(), Verdict::Pass);
}

// ---------------------------------------------------------------------------
// (2) Custom adapter that overrides `leaf_metadata`.
// ---------------------------------------------------------------------------

struct DirectAdapter;

impl Adapter for DirectAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::new("test/direct-adapter@0.0.1")
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        vec![MatchKey::exact(
            1,
            Address::new(SWAP_ROUTER_MAINNET).unwrap(),
            SELECTOR_EXACT_INPUT_SINGLE,
        )]
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        let p = decode_exact_input_single(&tx.data)
            .map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        let token = usdt_token();
        Ok(Action::Swap(SwapAction {
            protocol_id: "uniswap-v3".into(),
            actor: tx.from.clone(),
            target: tx.to.clone(),
            value_wei: tx.value_wei.clone(),
            input_token: token.clone(),
            output_token: weth_token(),
            input_amount: policy_engine::AmountSpec {
                token: token.clone(),
                raw: p.amount_in.to_string(),
                human: None,
                usd: None,
            },
            min_output_amount: None,
            recipient: Address::from_alloy(p.recipient),
            deadline: None,
            fee_bips: Some((p.fee / 100) as u32),
        }))
    }

    fn leaf_metadata(
        &self,
        tx: &TransactionRequest,
        _leaves: &[Action],
    ) -> Vec<Map<String, JsonValue>> {
        let p = decode_exact_input_single(&tx.data)
            .map_err(|e| AdapterError::BadCalldata(e.to_string()));
        let Ok(p) = p else {
            return vec![Default::default()];
        };
        let usd_int = p.amount_in / U256::from(1_000_000u64);
        let context = json!({
            "inputAmount": {
                "tokenSymbol": "USDT",
                "raw": p.amount_in.to_string(),
                "usd": {
                    "value": { "__extn": { "fn": "decimal", "arg": format!("{usd_int}.0000") } },
                    "staleSec": 0,
                }
            }
        });
        match context {
            JsonValue::Object(map) => vec![map],
            _ => vec![Default::default()],
        }
    }
}

#[test]
fn custom_adapter_can_override_into_request_to_skip_action() {
    let registry = MockAdapterRegistry::new().with_adapter(Arc::new(DirectAdapter));
    let oracle = MockOracle::new();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);

    // 200 USDT → $200 → fail
    let tx = build_swap_tx(U256::from(200_000_000u64));
    assert!(pipeline.evaluate(&tx).unwrap().is_failure());

    // 50 USDT → $50 → pass
    let tx = build_swap_tx(U256::from(50_000_000u64));
    assert_eq!(pipeline.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn custom_adapter_propagates_decode_failure() {
    let registry = MockAdapterRegistry::new().with_adapter(Arc::new(DirectAdapter));
    let oracle = MockOracle::new();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let pipeline = Pipeline::new(&registry, HostCapabilities::new(&oracle), &policies);
    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: SELECTOR_EXACT_INPUT_SINGLE.to_vec(),
        gas: None,
        nonce: None,
    };
    let err = pipeline.evaluate(&tx).unwrap_err();
    assert!(matches!(err, PipelineError::AdapterBuild(_)));
}

// ---------------------------------------------------------------------------
// (3) PolicyRequest is hand-buildable for unit-testing policies in isolation.
// ---------------------------------------------------------------------------

#[test]
fn hand_built_policy_request_evaluates_without_any_adapter() {
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();

    let request = PolicyRequest::new(
        r#"Wallet::"0xUser""#,
        r#"Action::"swap""#,
        r#"Protocol::"uniswap-v3""#,
        json!([
            { "uid": { "type": "Wallet",   "id": "0xUser" },     "attrs": {}, "parents": [] },
            { "uid": { "type": "Protocol", "id": "uniswap-v3" }, "attrs": {}, "parents": [] },
        ]),
        json!({
            "inputAmount": {
                "tokenSymbol": "USDT",
                "raw": "999999999",
                "usd": {
                    "value": { "__extn": { "fn": "decimal", "arg": "999.9999" } },
                    "staleSec": 1,
                }
            }
        }),
    );

    let verdict = policies.evaluate_request(&request).unwrap();
    assert!(verdict.is_failure());
}

// ---------------------------------------------------------------------------
// Object safety check.
// ---------------------------------------------------------------------------

#[test]
fn adapter_is_object_safe() {
    let adapters: Vec<Arc<dyn Adapter>> = vec![
        Arc::new(UniswapV3ExactInputSingleAdapter::new()),
        Arc::new(DirectAdapter),
    ];
    assert_eq!(adapters.len(), 2);
    let _ids: Vec<_> = adapters.iter().map(|a| a.id()).collect();
}

// ---------------------------------------------------------------------------
// Pipeline genericity: registry plugs in via `&dyn AdapterRegistry`.
// ---------------------------------------------------------------------------

#[test]
fn pipeline_accepts_dyn_adapter_registry() {
    use policy_engine::{AdapterRegistry, Verdict};

    let concrete = uniswap_v3_registry();
    let dyn_registry: &dyn AdapterRegistry = &concrete;

    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let pipeline = Pipeline::new(dyn_registry, HostCapabilities::new(&oracle), &policies);

    let tx = build_swap_tx(U256::from(50_000_000u64));
    assert_eq!(pipeline.evaluate(&tx).unwrap(), Verdict::Pass);

    let tx = build_swap_tx(U256::from(200_000_000u64));
    assert!(matches!(pipeline.evaluate(&tx).unwrap(), Verdict::Fail(_)));
}

/// Custom `AdapterRegistry` that always returns `NoMatch`. Verifies hosts
/// can substitute their own registry impl behind `&dyn AdapterRegistry`.
struct AlwaysNoMatchRegistry;
impl policy_engine::AdapterRegistry for AlwaysNoMatchRegistry {
    fn resolve_with_adapter(
        &self,
        _tx: &TransactionRequest,
    ) -> (policy_engine::ResolverOutcome, Option<Arc<dyn Adapter>>) {
        (policy_engine::ResolverOutcome::NoMatch, None)
    }
}

#[test]
fn pipeline_routes_through_custom_registry_impl() {
    use policy_engine::{Pipeline, Verdict};

    let custom = AlwaysNoMatchRegistry;
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let pipeline = Pipeline::new(&custom, HostCapabilities::new(&oracle), &policies);

    // No adapter resolved → Action::Other → swap-targeted forbid does not match → Pass.
    let tx = build_swap_tx(U256::from(200_000_000u64));
    assert_eq!(pipeline.evaluate(&tx).unwrap(), Verdict::Pass);
}
