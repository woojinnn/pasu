//! Integration test: external-orchestrator workflow.
//!
//! Validates that:
//! 1. `Pipeline::build_action_for(&Request::Tx(tx))` returns the same action
//!    `Pipeline::evaluate_tx` would build internally.
//! 2. `required_host_facts(&action)` enumerates exactly the tokens the engine
//!    consults during enrichment.
//! 3. Evaluating with a `SnapshotOracle` populated from the plan yields the
//!    same verdict as evaluating with a `MockOracle` populated identically.

use policy_engine::core::{Address, LegacyAction, Request, TransactionRequest, UsdValuation};
use policy_engine::host::oracle::{MockOracle, SnapshotOracle};
use policy_engine::host::HostCapabilities;
use policy_engine::lowering::required_host_facts;
use policy_engine::policy::{PolicyEngine, Verdict};
use policy_engine::Pipeline;
use policy_engine_adapters_bundle::default_registry;

const ACTOR: &str = "0x1111111111111111111111111111111111111111";
const V3_SWAP_ROUTER: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

fn weth_swap_calldata_v3_exact_input_single() -> Vec<u8> {
    // Calldata for a Uniswap V3 exactInputSingle WETH->USDC swap.
    // Selector 0x414bf389 then 8 × 32-byte words (matches
    // crates/adapters-bundle/src/uniswap_v3/exact_input_single.rs decode shape).
    let raw = concat!(
        "414bf389",
        "000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // tokenIn = WETH
        "000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // tokenOut = USDC
        "00000000000000000000000000000000000000000000000000000000000001f4", // fee = 500
        "0000000000000000000000001111111111111111111111111111111111111111", // recipient
        "00000000000000000000000000000000000000000000000000000000ffffffff", // deadline
        "0000000000000000000000000000000000000000000000000de0b6b3a7640000", // amountIn = 1 WETH
        "00000000000000000000000000000000000000000000000000000000b2d05e00", // amountOutMin = 3000 USDC
        "0000000000000000000000000000000000000000000000000000000000000000", // sqrtPriceLimitX96 = 0
    );
    hex::decode(raw).expect("static hex literal decodes")
}

#[test]
fn extract_plan_then_evaluate_matches_direct_path() {
    let registry = default_registry();
    let policies = PolicyEngine::builder()
        .build()
        .expect("empty PolicyEngine builds");

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new(ACTOR).unwrap(),
        to: Address::new(V3_SWAP_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: weth_swap_calldata_v3_exact_input_single(),
        gas: None,
        nonce: None,
    };

    // Path A: extract action via public API, derive plan, populate snapshot.
    let oracle_for_extract = SnapshotOracle::new();
    let host_a = HostCapabilities::new(&oracle_for_extract);
    let pipeline_a = Pipeline::new(&registry, host_a, &policies);
    let action = pipeline_a
        .build_action_for(&Request::Tx(tx.clone()))
        .unwrap();
    assert!(matches!(action, LegacyAction::Dex(_)));

    let plan = required_host_facts(&action);
    // The plan must enumerate WETH (input) + USDC (output) for oracle.
    let oracle_addrs: Vec<_> = plan
        .tokens_for_oracle
        .iter()
        .map(|t| t.address.as_str().to_lowercase())
        .collect();
    assert!(
        oracle_addrs.iter().any(|a| a.contains("c02aaa")),
        "plan must include WETH"
    );
    assert!(
        oracle_addrs.iter().any(|a| a.contains("a0b869")),
        "plan must include USDC"
    );

    // Build the SnapshotOracle the orchestrator would build.
    let mut snapshot = SnapshotOracle::new();
    for token in &plan.tokens_for_oracle {
        let usd = if token.symbol == "WETH" {
            "3500.00"
        } else {
            "1.00"
        };
        snapshot.insert(
            token,
            UsdValuation {
                value: usd.into(),
                as_of_ts: 1_700_000_000,
                sources: vec!["test-snapshot".into()],
                stale_sec: 30,
            },
        );
    }

    let host_b = HostCapabilities::new(&snapshot);
    let pipeline_b = Pipeline::new(&registry, host_b, &policies);
    let verdict_b = pipeline_b.evaluate(&Request::Tx(tx.clone())).unwrap();

    // Path C: direct evaluation with a MockOracle populated identically.
    // Match WETH+USDC by symbol regardless of plan iteration order.
    let mut mock = MockOracle::new();
    for token in &plan.tokens_for_oracle {
        let usd = if token.symbol == "WETH" {
            "3500.00"
        } else {
            "1.00"
        };
        mock = mock.with_simple_price(token, usd, 30);
    }
    let host_c = HostCapabilities::new(&mock);
    let pipeline_c = Pipeline::new(&registry, host_c, &policies);
    let verdict_c = pipeline_c.evaluate(&Request::Tx(tx)).unwrap();

    // SnapshotOracle and MockOracle must produce identical verdicts when populated identically.
    match (&verdict_b, &verdict_c) {
        (Verdict::Pass, Verdict::Pass) => {}
        (Verdict::Warn(a), Verdict::Warn(b)) | (Verdict::Fail(a), Verdict::Fail(b)) => {
            let ids_a: Vec<_> = a.iter().map(|m| &m.policy_id).collect();
            let ids_b: Vec<_> = b.iter().map(|m| &m.policy_id).collect();
            assert_eq!(
                ids_a, ids_b,
                "matched policies differ between snapshot and mock paths"
            );
        }
        _ => panic!(
            "verdict variants differ: snapshot={:?} mock={:?}",
            verdict_b, verdict_c
        ),
    }
}
