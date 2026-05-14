//! Compiled Cedar `PolicyEngine` and its evaluation entry points.

use cedar_policy::{
    Authorizer, Context, Decision as CedarDecision, Entities, EntityUid, Policy, PolicyId,
    PolicySet, Request, Schema, ValidationMode, ValidationResult, Validator,
};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use super::error::PolicyError;
use super::request::PolicyRequest;
use super::verdict::{MatchedPolicy, PolicyRequestOrigin, Severity, Verdict};

/// Compiled policy set + the auto-injected baseline permit.
#[derive(Debug)]
pub struct PolicyEngine {
    policy_set: PolicySet,
    schema: Schema,
    /// Per-policy-id severity, lifted from the `@severity(...)` annotation at
    /// parse time so we don't have to re-parse on every evaluation.
    severities: HashMap<String, Severity>,
    /// Per-policy-id @reason annotation.
    reasons: HashMap<String, String>,
}

impl PolicyEngine {
    /// Start a builder. Callers chain `.add_text(...)` and then `.build()`.
    /// The baseline permit and the bundled Cedar schema are added
    /// automatically.
    #[must_use]
    pub fn builder() -> super::builder::PolicyEngineBuilder {
        super::builder::PolicyEngineBuilder::new()
    }

    /// Convenience: build an engine from one or more text Cedar sources only.
    /// Equivalent to `PolicyEngine::builder().add_text(...).build()`.
    ///
    /// # Errors
    ///
    /// Returns an error when Cedar parsing or schema validation fails.
    pub fn from_sources<I, S>(sources: I) -> Result<Self, PolicyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut b = Self::builder();
        for s in sources {
            b = b.add_text(s.as_ref());
        }
        b.build()
    }

    /// Build a `PolicyEngine` from already-collected policies.
    /// `text_combined` is one big Cedar source string (with baseline
    /// prepended). Used by [`super::builder::PolicyEngineBuilder::build`].
    pub(super) fn build_from(text_combined: &str, schema: Schema) -> Result<Self, PolicyError> {
        let mut policy_set = PolicySet::new();
        let mut severities: HashMap<String, Severity> = HashMap::new();
        let mut reasons: HashMap<String, String> = HashMap::new();

        let initial_set =
            PolicySet::from_str(text_combined).map_err(|e| PolicyError::Parse(e.to_string()))?;

        for p in initial_set.policies() {
            ingest_policy(p, &mut policy_set, &mut severities, &mut reasons)?;
        }

        validate_policy_set(&policy_set, &schema)?;

        Ok(Self {
            policy_set,
            schema,
            severities,
            reasons,
        })
    }

    /// Evaluate a `PolicyRequest`. This is the preferred entry point.
    ///
    /// # Errors
    ///
    /// Returns an error when Cedar request construction or evaluation fails.
    pub fn evaluate_request(&self, req: &PolicyRequest) -> Result<Verdict, PolicyError> {
        self.evaluate_request_with_origin(req, PolicyRequestOrigin::Action)
    }

    /// Evaluate a `PolicyRequest` and annotate matches with the originating
    /// request kind.
    ///
    /// # Errors
    ///
    /// Returns an error when Cedar request construction or evaluation fails.
    pub fn evaluate_request_with_origin(
        &self,
        req: &PolicyRequest,
        origin: PolicyRequestOrigin,
    ) -> Result<Verdict, PolicyError> {
        let mut verdict = self.evaluate(
            &req.principal,
            &req.action,
            &req.resource,
            &req.entities,
            &req.context,
        )?;

        match &mut verdict {
            Verdict::Pass => {}
            Verdict::Warn(matches) | Verdict::Fail(matches) => {
                for matched in matches {
                    matched.origin = origin;
                }
            }
        }
        Ok(verdict)
    }

    /// Evaluate requests from one transaction while preserving request kind.
    /// A request list with empty input is treated as pass.
    ///
    /// # Errors
    ///
    /// Returns an error when any request fails to build or evaluate.
    pub fn evaluate_requests<'a, I>(&self, reqs: I) -> Result<Verdict, PolicyError>
    where
        I: IntoIterator<Item = (&'a PolicyRequest, PolicyRequestOrigin)>,
    {
        let verdicts = reqs
            .into_iter()
            .map(|(req, origin)| self.evaluate_request_with_origin(req, origin))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Verdict::aggregate(verdicts))
    }

    /// Evaluate a single Cedar request against the policy set.
    ///
    /// # Errors
    ///
    /// Returns an error when entity ids, context, entities, or request
    /// construction fail.
    pub fn evaluate(
        &self,
        principal: &str,
        action: &str,
        resource: &str,
        entities_json: &JsonValue,
        context_json: &JsonValue,
    ) -> Result<Verdict, PolicyError> {
        let principal: EntityUid = principal
            .parse()
            .map_err(|e: cedar_policy::ParseErrors| PolicyError::EntityUid(e.to_string()))?;
        let action: EntityUid = action
            .parse()
            .map_err(|e: cedar_policy::ParseErrors| PolicyError::EntityUid(e.to_string()))?;
        let resource: EntityUid = resource
            .parse()
            .map_err(|e: cedar_policy::ParseErrors| PolicyError::EntityUid(e.to_string()))?;

        let entities = Entities::from_json_value(entities_json.clone(), Some(&self.schema))
            .map_err(|e| PolicyError::Entities(e.to_string()))?;
        let context = Context::from_json_value(context_json.clone(), Some((&self.schema, &action)))
            .map_err(|e| PolicyError::Context(e.to_string()))?;

        let request = Request::new(principal, action, resource, context, Some(&self.schema))
            .map_err(|e| PolicyError::Request(e.to_string()))?;

        let auth = Authorizer::new();
        let response = auth.is_authorized(&request, &self.policy_set, &entities);

        // Cedar gives us a final `Allow` / `Deny` plus the set of policy ids
        // that contributed (`reason`). Each fired policy becomes a
        // `MatchedPolicy` carrying its own severity; the final `Verdict`
        // variant is then chosen by deny-overrides:
        //  - Cedar Allow: nothing fired → `Pass`.
        //  - Cedar Deny + ≥1 deny-severity match → `Fail(all_matches)` —
        //    `all_matches` includes any *also*-fired warn entries
        //    (warn-union).
        //  - Cedar Deny + only warn-severity matches → `Warn(warn_matches)`.
        //  - Cedar Deny + no severity-tagged matches: should never happen
        //    given our default-allow shape; fail-closed → `Fail([])`.

        let determining: HashSet<PolicyId> = response.diagnostics().reason().cloned().collect();
        let mut matched: Vec<MatchedPolicy> = Vec::new();

        for pid in &determining {
            let pid_str = pid.to_string();
            // Skip the baseline-allow permit; its presence as a "reason" just
            // means it succeeded as a permit, which isn't user-facing.
            if pid_str == "engine/baseline-allow" {
                continue;
            }
            // Skip un-annotated forbids: a `forbid` without `@severity` is
            // effectively a malformed policy in our convention.
            let severity = match self.severities.get(&pid_str) {
                Some(s) => *s,
                None => continue,
            };
            matched.push(MatchedPolicy {
                policy_id: pid_str.clone(),
                reason: self.reasons.get(&pid_str).cloned(),
                severity,
                origin: PolicyRequestOrigin::Action,
            });
        }

        let any_deny = matched.iter().any(|m| m.severity == Severity::Deny);
        let cedar_decision = response.decision();

        let verdict = match cedar_decision {
            CedarDecision::Allow => Verdict::Pass,
            CedarDecision::Deny => {
                if any_deny {
                    // Fail's vec carries BOTH severities — caller can split via .severity.
                    Verdict::Fail(matched)
                } else if !matched.is_empty() {
                    // All matches are warn-severity by elimination.
                    Verdict::Warn(matched)
                } else {
                    Verdict::Fail(Vec::new())
                }
            }
        };

        Ok(verdict)
    }
}

fn validate_policy_set(policy_set: &PolicySet, schema: &Schema) -> Result<(), PolicyError> {
    let result = Validator::new(schema.clone()).validate(policy_set, ValidationMode::Strict);
    if result.validation_passed() {
        Ok(())
    } else {
        Err(PolicyError::Validation(validation_errors_message(&result)))
    }
}

fn validation_errors_message(result: &ValidationResult) -> String {
    let messages: Vec<String> = result
        .validation_errors()
        .map(ToString::to_string)
        .collect();
    if messages.is_empty() {
        "unknown validation error".into()
    } else {
        messages.join("; ")
    }
}

fn annotation(p: &Policy, key: &str) -> Option<String> {
    p.annotation(key).map(std::string::ToString::to_string)
}

/// Pull `@id`, `@severity`, `@reason` out of a policy and add it to the set
/// under the user-facing id from `@id` (falling back to Cedar's auto-id).
fn ingest_policy(
    p: &Policy,
    policy_set: &mut PolicySet,
    severities: &mut HashMap<String, Severity>,
    reasons: &mut HashMap<String, String>,
) -> Result<(), PolicyError> {
    let user_id_raw = annotation(p, "id").unwrap_or_else(|| p.id().to_string());
    let user_id: PolicyId = match user_id_raw.parse() {
        Ok(user_id) => user_id,
        Err(never) => match never {},
    };
    let user_id_str = user_id.to_string();
    let new_policy = p.clone().new_id(user_id);

    if let Some(sev) = annotation(p, "severity") {
        let parsed = match sev.as_str() {
            "deny" => Severity::Deny,
            "warn" => Severity::Warn,
            other => {
                return Err(PolicyError::Parse(format!(
                    "policy {user_id_str}: unknown @severity({other:?}); expected \"deny\" or \"warn\""
                )));
            }
        };
        severities.insert(user_id_str.clone(), parsed);
    }
    if let Some(r) = annotation(p, "reason") {
        reasons.insert(user_id_str, r);
    }

    policy_set
        .add(new_policy)
        .map_err(|e| PolicyError::Parse(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{PolicyEngineBuilder, PolicyError, PolicyRequest};
    use super::*;
    use serde_json::json;

    fn entities() -> JsonValue {
        json!([
            {
                "uid": { "type": "Wallet", "id": "0xUser" },
                "attrs": {
                    "address": "0x0000000000000000000000000000000000000001"
                },
                "parents": []
            },
            {
                "uid": { "type": "Protocol", "id": "swap" },
                "attrs": {},
                "parents": []
            }
        ])
    }

    fn token(symbol: &str) -> JsonValue {
        json!({
            "kind": "erc20",
            "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "symbol": symbol,
            "decimals": 18,
        })
    }

    fn context_swap(usd_value: &str) -> JsonValue {
        json!({
            "swapMode": "exact_in",
            "inputToken": {
                "asset": token("WETH"),
                "amount": { "kind": "exact", "value": "1000" },
            },
            "outputToken": {
                "asset": token("USDC"),
                "amount": { "kind": "min", "value": "900" },
            },
            "recipient": "0x0000000000000000000000000000000000000001",
            "totalInputUsd": {
                "value": { "__extn": { "fn": "decimal", "arg": usd_value } },
                "asOfTs": 1,
                "staleSec": 5,
                "sources": ["mock"],
            },
        })
    }

    #[test]
    fn empty_policy_set_allows_everything() {
        let engine = PolicyEngine::from_sources(Vec::<&str>::new()).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &context_swap("50.00"),
            )
            .unwrap();
        assert_eq!(v, Verdict::Pass);
    }

    #[test]
    fn deny_when_usd_exceeds_cap() {
        let policy = r#"
            @id("user/max-swap-usd-100")
            @severity("deny")
            @reason("USD value of swap exceeds 100")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &context_swap("200.00"),
            )
            .unwrap();
        match v {
            Verdict::Fail(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/max-swap-usd-100");
                assert_eq!(matched[0].severity, Severity::Deny);
                assert_eq!(
                    matched[0].reason.as_deref(),
                    Some("USD value of swap exceeds 100")
                );
            }
            _ => panic!("expected Verdict::Fail, got {v:?}"),
        }
    }

    #[test]
    fn allow_when_usd_under_cap() {
        let policy = r#"
            @id("user/max-swap-usd-100")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &context_swap("50.00"),
            )
            .unwrap();
        assert_eq!(v, Verdict::Pass);
    }

    #[test]
    fn warn_variant_when_only_warn_severity_fires() {
        let policy = r#"
            @id("user/large-swap-warning")
            @severity("warn")
            @reason("Large swap — please review")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &context_swap("200.00"),
            )
            .unwrap();
        match v {
            Verdict::Warn(matched) => {
                assert_eq!(matched.len(), 1);
                assert_eq!(matched[0].policy_id, "user/large-swap-warning");
                assert_eq!(matched[0].severity, Severity::Warn);
            }
            _ => panic!("expected Verdict::Warn, got {v:?}"),
        }
    }

    #[test]
    fn fail_preserves_warn_entries_alongside_deny() {
        let policy = r#"
            @id("user/large-swap-warning")
            @severity("warn")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };

            @id("user/huge-swap-deny")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("150.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &context_swap("200.00"),
            )
            .unwrap();
        match v {
            Verdict::Fail(matched) => {
                assert_eq!(
                    matched.len(),
                    2,
                    "Fail vec must include both warn + deny entries"
                );
                assert!(matched.iter().any(|m| {
                    m.policy_id == "user/huge-swap-deny" && m.severity == Severity::Deny
                }));
                assert!(matched.iter().any(|m| {
                    m.policy_id == "user/large-swap-warning" && m.severity == Severity::Warn
                }));
            }
            _ => panic!("expected Verdict::Fail, got {v:?}"),
        }
    }

    #[test]
    fn rejects_unknown_severity() {
        let policy = r#"
            @id("bad")
            @severity("idk")
            forbid (principal, action, resource);
        "#;
        let err = PolicyEngine::from_sources([policy]).unwrap_err();
        assert!(matches!(err, PolicyError::Parse(_)));
    }

    #[test]
    fn default_builder_matches_new_builder_schema() {
        // Regression guard against accidentally re-adding `#[derive(Default)]`
        // to PolicyEngineBuilder. Both the explicit `new()` constructor and
        // the `Default::default()` path must produce engines that strict-
        // validate against the bundled schema.
        let typo_policy = r#"
            @id("smoke/typo")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUSd.value.greaterThan(decimal("0"))
            };
        "#;

        let via_default = PolicyEngineBuilder::default().add_text(typo_policy).build();
        let via_new = PolicyEngineBuilder::new().add_text(typo_policy).build();

        assert!(matches!(via_default, Err(PolicyError::Validation(_))));
        assert!(matches!(via_new, Err(PolicyError::Validation(_))));
    }

    #[test]
    fn from_sources_validates_policy_against_bundled_schema() {
        let policy = r#"
            @id("bad/from-sources-context-typo")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when { context.totalInputUSd.value.greaterThan(decimal("100.00")) };
        "#;

        let err = PolicyEngine::from_sources([policy]).unwrap_err();
        assert!(
            matches!(err, PolicyError::Validation(_)),
            "expected PolicyError::Validation, got {err:?}"
        );
    }

    #[test]
    fn schema_validation_rejects_policy_with_unknown_context_field() {
        let policy = r#"
            @id("bad/context-typo")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when { context.totalInputUSd.value.greaterThan(decimal("100.00")) };
        "#;

        let err = PolicyEngine::builder()
            .add_text(policy)
            .build()
            .unwrap_err();

        assert!(matches!(err, PolicyError::Validation(_)));
    }

    #[test]
    fn schema_validation_rejects_request_with_invalid_context_shape() {
        let policy = r#"
            @id("user/max-swap-usd-100")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::builder().add_text(policy).build().unwrap();

        let err = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"swap""#,
                &entities(),
                &json!({}),
            )
            .unwrap_err();

        // Passing the schema to Context::from_json_value moves missing-required-attr
        // detection into context construction (the layer below Request::new), so the
        // error attribution is PolicyError::Context, not PolicyError::Request.
        assert!(matches!(err, PolicyError::Context(_)));
    }

    #[test]
    fn evaluate_request_marks_matches_as_single_action_origin() {
        let policy = r#"
            @id("user/action-deny")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource);
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let request = PolicyRequest::new(
            r#"Wallet::"0xUser""#,
            r#"Action::"swap""#,
            r#"Protocol::"swap""#,
            entities(),
            context_swap("50.00"),
        );

        let verdict = engine.evaluate_request(&request).unwrap();

        match verdict {
            Verdict::Fail(matched) => {
                assert_eq!(matched[0].origin, PolicyRequestOrigin::Action);
            }
            other => panic!("expected Verdict::Fail, got {other:?}"),
        }
    }
}
