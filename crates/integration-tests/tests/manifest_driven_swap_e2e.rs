//! Phase 8 / Task 8.1 — manifest-driven swap policy end-to-end.
//!
//! Walks the full Phase-5+ pipeline: build a synthetic swap envelope,
//! attach a manifest that produces `context.custom.totalInputUsd` via a
//! mock RPC, materialize the result, and evaluate a policy that fails
//! when `totalInputUsd.value > 100`. Asserts Pass at $50 and Fail at $150.
//!
//! Mirrors the design-spec acceptance criterion:
//!   "install/evaluate round-trip with custom context, and both runtime
//!    failure branches in D9".
//!
//! Unlike `e2e_new_pipeline.rs::test_max_input_usd_100_*` (which uses
//! the bundled example manifest verbatim), this test programmatically
//! constructs the manifest object so future re-uses of the helper don't
//! depend on the on-disk JSON fixture.

use policy_engine::action::common::AssetRefWithAmountConstraint;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::policy_rpc::{
    apply_rpc_results, plan_calls, PolicyManifest, PolicyRpcResponse, PolicyRpcResult, RootInput,
};
use policy_engine::{
    policy_request_from_envelope, Action, ActionAddress, ActionEnvelope, AmountConstraint,
    AmountKind, AssetKind, AssetRef, Category, DecimalString, PolicyEngineBuilder, Severity,
    Validity, ValiditySource, Verdict,
};
use serde_json::{json, Value};
use std::str::FromStr as _;

const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

// ── Manifest builder ────────────────────────────────────────────────

/// Build a swap manifest with a single `oracle.usd_value`-style
/// requirement producing one context-bound output `totalInputUsd:
/// UsdValuation`. Mirrors the shape `apply_rpc_results` expects.
fn build_swap_manifest_with_total_input_usd() -> PolicyManifest {
    let json = serde_json::json!({
        "id": "test::swap/max-100",
        "schema_version": 1,
        "requires": [
            {
                "id": "swap-total-input-usd",
                "when": { "action": "swap" },
                "method": "oracle.usd_value",
                "params": {
                    "chain_id": "$.root.chain_id",
                    "asset": "$.action.inputToken.asset",
                    "amount": "$.action.inputToken.amount.value"
                },
                "outputs": [
                    {
                        "kind": "context",
                        "field": "totalInputUsd",
                        "type": "UsdValuation",
                        "from": "$.result",
                        "required": true
                    }
                ],
                "optional": false
            }
        ],
        "context_extensions": {
            "swap": { "totalInputUsd": "UsdValuation" }
        }
    });
    serde_json::from_value(json).expect("test manifest should parse")
}

// ── Synthetic swap envelope ─────────────────────────────────────────

fn synthetic_swap_envelope() -> (ActionEnvelope, ActionAddress, ActionAddress) {
    let from = ActionAddress::from_str("0x1111111111111111111111111111111111111111")
        .expect("valid from address");
    let to = ActionAddress::from_str("0x2222222222222222222222222222222222222222")
        .expect("valid to address");
    let token_in = AssetRef {
        kind: AssetKind::Erc20,
        address: Some(
            ActionAddress::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                .expect("valid USDC address"),
        ),
        token_id: None,
        symbol: Some("USDC".to_owned()),
        decimals: Some(6),
    };
    let token_out = AssetRef {
        kind: AssetKind::Erc20,
        address: Some(
            ActionAddress::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
                .expect("valid WETH address"),
        ),
        token_id: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    };
    let amount_in = AmountConstraint {
        kind: AmountKind::Exact,
        value: Some(DecimalString::from_str("10000000000").expect("valid amount-in")),
    };
    let amount_out = AmountConstraint {
        kind: AmountKind::Min,
        value: Some(DecimalString::from_str("200000000").expect("valid amount-out")),
    };
    let validity = Some(Validity {
        expires_at: DecimalString::from_str(&(BLOCK_TIMESTAMP as i64 + 300).to_string())
            .expect("valid expiry"),
        source: ValiditySource::TxDeadline,
    });
    let envelope = ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            swap_mode: SwapMode::ExactIn,
            input_token: AssetRefWithAmountConstraint {
                asset: token_in,
                amount: amount_in,
            },
            output_token: AssetRefWithAmountConstraint {
                asset: token_out,
                amount: amount_out,
            },
            recipient: from.clone(),
            validity,
            fee_bps: Some(30),
        }),
    };
    (envelope, from, to)
}

// ── Engine wrapper ──────────────────────────────────────────────────

struct ManifestEngine {
    manifest: PolicyManifest,
    policy_text: String,
    envelope: ActionEnvelope,
    from: ActionAddress,
    to: ActionAddress,
}

impl ManifestEngine {
    fn new(manifest: PolicyManifest, policy_text: &str) -> Self {
        let (envelope, from, to) = synthetic_swap_envelope();
        Self {
            manifest,
            policy_text: policy_text.to_owned(),
            envelope,
            from,
            to,
        }
    }

    /// Plan → mock RPC response with `usd_value` → materialize → evaluate.
    /// `usd_value` is the dollar amount the policy compares against; the
    /// caller writes the policy threshold in cedar (e.g. > 100).
    fn evaluate_swap_at(&self, usd_value: &str) -> Verdict {
        // 1. Build the enriched cedarschema from the manifest map. This
        //    is what the runtime materializer writes against.
        let mut manifests_map = std::collections::BTreeMap::new();
        manifests_map.insert("swap".to_owned(), self.manifest.clone());
        let enriched =
            policy_engine::schema::compose_enriched(&manifests_map).expect("compose enriched");

        // 2. Build the policy engine with the enriched schema text and
        //    the deny-style policy.
        let mut builder = PolicyEngineBuilder::with_schema_text(enriched.schema_text.clone());
        builder = builder.add_text(self.policy_text.clone());
        let engine = builder.build().expect("policy engine should build");

        // 3. Lower the envelope to a PolicyRequest with empty `context.custom`.
        let mut requests = vec![policy_request_from_envelope(
            &self.envelope,
            &self.from,
            &self.to,
            &DecimalString::from_str("0").expect("zero decimal"),
            1,
            BLOCK_TIMESTAMP,
        )
        .expect("envelope should lower to swap request")];

        // 4. Plan calls.
        let root = RootInput {
            chain_id: 1,
            from: self.from.to_string(),
            to: self.to.to_string(),
            value_wei: "0".to_owned(),
            block_timestamp: Some(BLOCK_TIMESTAMP),
        };
        let manifests = [self.manifest.clone()];
        let call = plan_calls(
            &root,
            std::slice::from_ref(&self.envelope),
            &manifests,
            &json!({}),
        )
        .expect("manifest should plan")
        .pop()
        .expect("manifest should produce one call");

        // 5. Materialize the mock RPC response → context.custom.totalInputUsd.
        apply_rpc_results(
            &mut requests,
            std::slice::from_ref(&self.envelope),
            &manifests,
            &PolicyRpcResponse {
                request_id: "manifest-driven-e2e".to_owned(),
                results: vec![PolicyRpcResult {
                    id: call.id,
                    ok: true,
                    result: Some(json!({
                        "value": usd_value,
                        "asOfTs": BLOCK_TIMESTAMP,
                        "sources": ["test-oracle"],
                        "staleSec": 0
                    })),
                    error: None,
                }],
            },
        )
        .expect("RPC response should materialize");

        // Sanity: context.custom.totalInputUsd landed where the policy
        // expects. The materializer encodes `value` as a Cedar decimal
        // extension: `{"__extn": {"fn": "decimal", "arg": "<n>"}}`.
        let request = &requests[0];
        let custom = request
            .context
            .get("custom")
            .expect("context should have custom block");
        let total_input_usd = custom
            .get("totalInputUsd")
            .expect("context.custom.totalInputUsd should be materialized");
        let extn = total_input_usd
            .get("value")
            .and_then(|v| v.get("__extn"))
            .expect("totalInputUsd.value should be a Cedar decimal extension");
        let arg = extn
            .get("arg")
            .and_then(Value::as_str)
            .expect("decimal __extn arg should be a string");
        assert_eq!(arg, usd_value);

        // 6. Evaluate.
        engine
            .evaluate(
                &request.principal,
                &request.action,
                &request.resource,
                &request.entities,
                &request.context,
            )
            .expect("policy request should evaluate")
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn manifest_driven_swap_passes_with_total_input_under_threshold() {
    let manifest = build_swap_manifest_with_total_input_usd();
    let policy = r#"@id("test::swap/max-100")
@severity("deny")
@reason("over 100")
forbid (principal, action == Action::"swap", resource)
when {
  context has custom &&
  context.custom has totalInputUsd &&
  context.custom.totalInputUsd.value.greaterThan(decimal("100.0000"))
};"#;
    let engine = ManifestEngine::new(manifest, policy);

    let verdict = engine.evaluate_swap_at("50.0000");
    assert_eq!(verdict, Verdict::Pass);
}

#[test]
fn manifest_driven_swap_fails_with_total_input_over_threshold() {
    let manifest = build_swap_manifest_with_total_input_usd();
    let policy = r#"@id("test::swap/max-100")
@severity("deny")
@reason("over 100")
forbid (principal, action == Action::"swap", resource)
when {
  context has custom &&
  context.custom has totalInputUsd &&
  context.custom.totalInputUsd.value.greaterThan(decimal("100.0000"))
};"#;
    let engine = ManifestEngine::new(manifest, policy);

    let verdict = engine.evaluate_swap_at("150.0000");
    match verdict {
        Verdict::Fail(matched) => {
            assert!(
                matched
                    .iter()
                    .any(|m| m.policy_id == "test::swap/max-100" && m.severity == Severity::Deny),
                "expected deny policy test::swap/max-100 to fire, got {matched:?}"
            );
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn manifest_driven_swap_fails_at_threshold_boundary() {
    // Boundary check: $100.0001 should fail (> 100), $100.0000 should pass.
    let manifest = build_swap_manifest_with_total_input_usd();
    let policy = r#"@id("test::swap/max-100")
@severity("deny")
@reason("over 100")
forbid (principal, action == Action::"swap", resource)
when {
  context has custom &&
  context.custom has totalInputUsd &&
  context.custom.totalInputUsd.value.greaterThan(decimal("100.0000"))
};"#;
    let engine = ManifestEngine::new(manifest.clone(), policy);

    let edge_pass = engine.evaluate_swap_at("100.0000");
    assert_eq!(edge_pass, Verdict::Pass);

    let edge_fail = ManifestEngine::new(manifest, policy).evaluate_swap_at("100.0001");
    assert!(matches!(edge_fail, Verdict::Fail(_)));
}
