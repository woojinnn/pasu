//! Verify that the JSON form of `max-swap-usd-100` produces verdicts identical
//! to the text form, both individually and through the full pipeline.

use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, MockAdapterRegistry, MockOracle, Pipeline, PolicyEngine, Token, TransactionRequest,
    Verdict,
};
use policy_engine_adapter_uniswap_v3::{
    encode_exact_input_single, ExactInputSingleParams, UniswapV3ExactInputSingleAdapter,
    SWAP_ROUTER_MAINNET,
};
use serde_json::Value as JsonValue;
use std::str::FromStr;
use std::sync::Arc;

const POLICY_TEXT: &str = include_str!("../../../policies/swap/max-swap-usd-100.cedar");
const POLICY_JSON: &str = include_str!("../../../policies/swap/max-swap-usd-100.json");

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
        gas: None,
        nonce: None,
    }
}

fn pipeline_with(engine: PolicyEngine) -> impl Fn(U256) -> policy_engine::policy::Verdict {
    let registry =
        MockAdapterRegistry::new().with_adapter(Arc::new(UniswapV3ExactInputSingleAdapter::new()));
    let oracle = MockOracle::new()
        .with_simple_price(&usdt_token(), "1.0000", 5)
        .with_simple_price(&weth_token(), "3000.0000", 8);
    move |amount_in| {
        let pipe = Pipeline::new(&registry, &oracle, &engine);
        pipe.evaluate(&build_swap_tx(amount_in))
            .expect("pipeline ok")
    }
}

// --- JSON policy parses correctly -------------------------------------------

#[test]
fn json_policy_file_loads_via_builder() {
    let engine = PolicyEngine::builder()
        .add_json_str(POLICY_JSON)
        .expect("valid JSON")
        .build()
        .expect("build ok");
    // The fact that we got here means parse + ingest succeeded.
    drop(engine);
}

#[test]
fn json_policy_value_loads_via_builder() {
    let json: JsonValue = serde_json::from_str(POLICY_JSON).unwrap();
    let _engine = PolicyEngine::builder().add_json(json).build().unwrap();
}

#[test]
fn empty_builder_allows_everything_with_baseline() {
    let engine = PolicyEngine::builder().build().unwrap();
    let pipe = pipeline_with(engine);
    let v = pipe(U256::from(200_000_000u64));
    assert_eq!(v, Verdict::Pass);
}

// --- Text vs JSON path produce the same verdicts ----------------------------

#[test]
fn text_and_json_agree_on_deny_at_200_usdt() {
    let text_engine = PolicyEngine::builder()
        .add_text(POLICY_TEXT)
        .build()
        .unwrap();
    let json_engine = PolicyEngine::builder()
        .add_json_str(POLICY_JSON)
        .unwrap()
        .build()
        .unwrap();

    let v_text = pipeline_with(text_engine)(U256::from(200_000_000u64));
    let v_json = pipeline_with(json_engine)(U256::from(200_000_000u64));

    let (text_matched, json_matched) = match (&v_text, &v_json) {
        (Verdict::Fail(t), Verdict::Fail(j)) => (t, j),
        _ => panic!("expected both Fail, got text={v_text:?} json={v_json:?}"),
    };
    assert_eq!(text_matched.len(), 1);
    assert_eq!(json_matched.len(), 1);
    assert_eq!(text_matched[0].policy_id, json_matched[0].policy_id);
    assert_eq!(text_matched[0].reason, json_matched[0].reason);
}

#[test]
fn text_and_json_agree_on_allow_at_50_usdt() {
    let text_engine = PolicyEngine::builder()
        .add_text(POLICY_TEXT)
        .build()
        .unwrap();
    let json_engine = PolicyEngine::builder()
        .add_json_str(POLICY_JSON)
        .unwrap()
        .build()
        .unwrap();

    let v_text = pipeline_with(text_engine)(U256::from(50_000_000u64));
    let v_json = pipeline_with(json_engine)(U256::from(50_000_000u64));

    assert_eq!(v_text, Verdict::Pass);
    assert_eq!(v_json, Verdict::Pass);
}

#[test]
fn text_and_json_agree_at_boundary_100_usdt() {
    let text_engine = PolicyEngine::builder()
        .add_text(POLICY_TEXT)
        .build()
        .unwrap();
    let json_engine = PolicyEngine::builder()
        .add_json_str(POLICY_JSON)
        .unwrap()
        .build()
        .unwrap();

    let v_text = pipeline_with(text_engine)(U256::from(100_000_000u64));
    let v_json = pipeline_with(json_engine)(U256::from(100_000_000u64));

    // Both must agree: ">100" means exactly-100 allows.
    assert_eq!(v_text, Verdict::Pass);
    assert_eq!(v_json, Verdict::Pass);
}

// --- Mixing text + JSON in the same builder ---------------------------------

#[test]
fn mixing_text_and_json_in_same_builder_works() {
    // Identical policy added via both inputs. Cedar will see two policies, each
    // with the same `@id("user/max-swap-usd-100")`. Cedar disallows duplicate
    // policy ids in a single set, so this must error — proving the builder
    // honors id uniqueness across both source kinds.
    let result = PolicyEngine::builder()
        .add_text(POLICY_TEXT)
        .add_json_str(POLICY_JSON)
        .unwrap()
        .build();
    assert!(
        result.is_err(),
        "expected error when same @id appears in both text and JSON sources"
    );
}

#[test]
fn json_alongside_extra_text_policy() {
    // JSON max-100 + an additional text-only policy that warns on swaps below
    // 10 USD. Both should fire on a 5-USDT swap (warn, but not deny).
    let extra_text = r#"
        @id("user/min-warn-on-tiny-swaps")
        @severity("warn")
        @reason("Tiny swap — fee may exceed value")
        forbid (principal, action == Action::"swap", resource)
        when {
          context has "inputAmount" &&
          context.inputAmount has "usd" &&
          context.inputAmount.usd.value.lessThan(decimal("10.00"))
        };
    "#;

    let engine = PolicyEngine::builder()
        .add_json_str(POLICY_JSON)
        .unwrap()
        .add_text(extra_text)
        .build()
        .unwrap();

    let pipe = pipeline_with(engine);
    let v = pipe(U256::from(5_000_000u64)); // 5 USDT
    match v {
        Verdict::Warn(matched) => {
            assert_eq!(matched.len(), 1);
            assert_eq!(matched[0].policy_id, "user/min-warn-on-tiny-swaps");
        }
        _ => panic!("expected Verdict::Warn, got {v:?}"),
    }
}

// --- Bad JSON ---------------------------------------------------------------

#[test]
fn invalid_json_string_is_rejected() {
    let result = PolicyEngine::builder().add_json_str("{ not real json");
    assert!(result.is_err());
}

#[test]
fn structurally_valid_but_semantically_wrong_json_is_rejected() {
    // Valid JSON, but missing the `effect` field Cedar needs.
    let result = PolicyEngine::builder()
        .add_json_str(r#"{ "totally": "not-a-policy" }"#)
        .unwrap()
        .build();
    assert!(result.is_err());
}
