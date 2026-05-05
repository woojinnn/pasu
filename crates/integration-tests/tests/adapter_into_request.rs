//! Demonstrate `Adapter::into_request` (the calldata → `PolicyRequest`
//! one-shot) in three flavors:
//!
//! 1. Default `into_request` on the existing UniswapV3 adapter — no override
//!    needed; the blanket-style default does Action build → USD enrich →
//!    Cedar request build.
//! 2. Custom adapter that *overrides* `into_request` and bypasses the `Action`
//!    intermediate, emitting a Cedar context straight from raw calldata.
//! 3. Hand-built `PolicyRequest` for unit-testing the policy layer alone.

use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Action, Adapter, AdapterError, AdapterId, Address, MatchKey, MockOracle, Oracle, PolicyEngine,
    PolicyRequest, SwapAction, Token, TransactionRequest, Verdict,
};
use policy_engine_adapter_uniswap_v3::{
    decode_exact_input_single, encode_exact_input_single, ExactInputSingleParams,
    UniswapV3ExactInputSingleAdapter, SELECTOR_EXACT_INPUT_SINGLE, SWAP_ROUTER_MAINNET,
};
use serde_json::{json, Value as JsonValue};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_TEXT: &str = include_str!("../../../policies/swap/max-swap-usd-100.cedar");

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

// ---------------------------------------------------------------------------
// (1) Default `into_request` on existing adapters — no override needed.
// ---------------------------------------------------------------------------

#[test]
fn default_into_request_produces_evaluable_policy_request() {
    let adapter = UniswapV3ExactInputSingleAdapter::new();
    let oracle = full_oracle();
    let tx = build_swap_tx(U256::from(200_000_000u64));

    let request: PolicyRequest = adapter.into_request(&tx, &oracle).unwrap();

    // Principal is derived from `tx.from` (action.actor()), not hardcoded.
    assert_eq!(
        request.principal,
        r#"Wallet::"0x0000000000000000000000000000000000000001""#
    );
    assert_eq!(request.action, r#"Action::"swap""#);
    assert_eq!(request.resource, r#"Protocol::"uniswap-v3""#);

    let usd_value = request
        .context
        .get("inputAmount")
        .and_then(|v| v.get("usd"))
        .and_then(|v| v.get("value"));
    assert!(usd_value.is_some(), "context.inputAmount.usd.value missing");
}

#[test]
fn default_into_request_path_evaluates_to_deny_at_200_usdt() {
    let adapter = UniswapV3ExactInputSingleAdapter::new();
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();

    let tx = build_swap_tx(U256::from(200_000_000u64));
    let request = adapter.into_request(&tx, &oracle).unwrap();
    let verdict = policies.evaluate_request(&request).unwrap();

    match verdict {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/max-swap-usd-100");
        }
        _ => panic!("expected Verdict::Fail, got {verdict:?}"),
    }
}

#[test]
fn default_into_request_path_evaluates_to_allow_under_cap() {
    let adapter = UniswapV3ExactInputSingleAdapter::new();
    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();

    let tx = build_swap_tx(U256::from(50_000_000u64));
    let request = adapter.into_request(&tx, &oracle).unwrap();
    let verdict = policies.evaluate_request(&request).unwrap();
    assert_eq!(verdict, Verdict::Pass);
}

// ---------------------------------------------------------------------------
// (2) Custom adapter that overrides `into_request` to bypass Action.
//
// `build` is required by the trait, so we emit a placeholder Swap action that
// would let the default path also work. The override produces a custom Cedar
// context directly from calldata without consulting the oracle.
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
        // Stub: produce a minimally valid Swap action. The override below
        // means this is rarely reached in practice.
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

    fn into_request(
        &self,
        tx: &TransactionRequest,
        _oracle: &dyn Oracle,
    ) -> Result<PolicyRequest, AdapterError> {
        // Skip the Action intermediate entirely: produce a Cedar context
        // directly from the decoded calldata, with USDT pegged at $1 by fiat.
        let p = decode_exact_input_single(&tx.data)
            .map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
        let usd_int = p.amount_in / U256::from(1_000_000u64);
        let usd_str = format!("{usd_int}.0000");

        let entities: JsonValue = json!([
            { "uid": { "type": "Wallet",   "id": "0xUser" },     "attrs": {}, "parents": [] },
            { "uid": { "type": "Protocol", "id": "uniswap-v3" }, "attrs": {}, "parents": [] },
        ]);
        let context: JsonValue = json!({
            "inputAmount": {
                "tokenSymbol": "USDT",
                "raw": p.amount_in.to_string(),
                "usd": {
                    "value": { "__extn": { "fn": "decimal", "arg": usd_str } },
                    "staleSec": 0,
                }
            }
        });

        Ok(PolicyRequest::new(
            r#"Wallet::"0xUser""#,
            r#"Action::"swap""#,
            r#"Protocol::"uniswap-v3""#,
            entities,
            context,
        ))
    }
}

#[test]
fn custom_adapter_can_override_into_request_to_skip_action() {
    let adapter = DirectAdapter;
    let oracle = MockOracle::new(); // ignored by DirectAdapter
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();

    // 200 USDT → $200 → fail
    let tx = build_swap_tx(U256::from(200_000_000u64));
    let request = adapter.into_request(&tx, &oracle).unwrap();
    assert!(policies.evaluate_request(&request).unwrap().is_failure());

    // 50 USDT → $50 → pass
    let tx = build_swap_tx(U256::from(50_000_000u64));
    let request = adapter.into_request(&tx, &oracle).unwrap();
    assert_eq!(policies.evaluate_request(&request).unwrap(), Verdict::Pass);
}

#[test]
fn custom_adapter_propagates_decode_failure() {
    let adapter = DirectAdapter;
    let oracle = MockOracle::new();
    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
        to: Address::new(SWAP_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x00, 0x00],
        gas: None,
        nonce: None,
    };
    let err = adapter.into_request(&tx, &oracle).unwrap_err();
    assert!(matches!(err, AdapterError::BadCalldata(_)));
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
    use policy_engine::{AdapterRegistry, MockAdapterRegistry, Pipeline, Verdict};

    let concrete =
        MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV3ExactInputSingleAdapter::new()));
    let dyn_registry: &dyn AdapterRegistry = &concrete;

    let oracle = full_oracle();
    let policies = PolicyEngine::from_sources([POLICY_TEXT]).unwrap();
    let pipeline = Pipeline::new(dyn_registry, &oracle, &policies);

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
    let pipeline = Pipeline::new(&custom, &oracle, &policies);

    // No adapter resolved → Action::Other → swap-targeted forbid does not match → Pass.
    let tx = build_swap_tx(U256::from(200_000_000u64));
    assert_eq!(pipeline.evaluate(&tx).unwrap(), Verdict::Pass);
}
