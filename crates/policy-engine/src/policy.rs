//! Cedar policy wrapper.
//!
//! Wraps the AWS `cedar-policy` crate to enforce our v0.1 conventions:
//!
//! 1. **Default-allow**: a baseline `permit(principal, action, resource);`
//!    policy is added to the policy set so that, in the absence of any
//!    matching `forbid`, Cedar returns `Allow`.
//! 2. **`@severity` annotation** on each `forbid` clause distinguishes `deny`
//!    from `warn`. Deny-overrides; warn-union otherwise.
//! 3. **Verdict aggregation**: we read Cedar diagnostics to discover which
//!    `forbid` clauses fired and which severity each carried, then collapse
//!    the result into our tri-state `Decision`.

use cedar_policy::{
    Authorizer, Context, Decision as CedarDecision, Entities, EntityUid, Policy, PolicyId,
    PolicySet, Request, Schema, ValidationMode, ValidationResult, Validator,
};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use thiserror::Error;

use crate::schema::PolicySchemaComposer;

/// Final, host-facing verdict. Tri-state: `Pass` carries no data, `Warn` and
/// `Fail` carry the list of matched policies that drove the verdict.
///
/// Variant semantics map directly onto the spec's deny-overrides + warn-union
/// rule:
/// - any matched `forbid` with `@severity("deny")` → `Fail(...)`
/// - otherwise, any matched `forbid` with `@severity("warn")` → `Warn(...)`
/// - otherwise → `Pass`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No matching deny or warning policies fired.
    Pass,
    /// One or more warning policies fired and no deny policy fired.
    Warn(Vec<MatchedPolicy>),
    /// One or more deny policies fired.
    Fail(Vec<MatchedPolicy>),
}

impl Verdict {
    /// True iff the host wallet must refuse to sign.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Fail(_))
    }

    /// True iff there's at least one warning the host should display.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.matched().iter().any(|m| m.severity == Severity::Warn)
    }

    /// Returns the matched policies for the warn / fail variants; empty for
    /// `Pass`. Useful when the caller wants to render diagnostics regardless
    /// of variant.
    #[must_use]
    pub fn matched(&self) -> &[MatchedPolicy] {
        match self {
            Self::Pass => &[],
            Self::Warn(v) | Self::Fail(v) => v,
        }
    }

    /// Combine per-request verdicts with deny-overrides semantics.
    #[must_use]
    pub fn aggregate<I>(verdicts: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut matched = Vec::new();
        let mut any_deny = false;
        let mut any_warn = false;

        for verdict in verdicts {
            match verdict {
                Self::Pass => {}
                Self::Warn(mut v) => {
                    any_warn = true;
                    matched.append(&mut v);
                }
                Self::Fail(mut v) => {
                    any_deny = true;
                    any_warn |= v.iter().any(|m| m.severity == Severity::Warn);
                    matched.append(&mut v);
                }
            }
        }

        if any_deny {
            Self::Fail(matched)
        } else if any_warn {
            Self::Warn(matched)
        } else {
            Self::Pass
        }
    }
}

/// `@severity("deny" | "warn")` annotation parsed from a policy's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Blocking severity.
    Deny,
    /// Non-blocking warning severity.
    Warn,
}

/// One `forbid` clause that fired during evaluation.
///
/// Severity is preserved per-element so that, when a `Verdict::Fail` is
/// produced because a deny fired, any also-fired warns are still reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestKind {
    /// Request originated from an action-level policy request.
    Action,
    /// Request originated from a transaction-level policy request.
    Tx,
}

/// Policy metadata returned with `Warn` and `Fail` verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPolicy {
    /// Cedar policy id.
    pub policy_id: String,
    /// Optional `@reason(...)` annotation.
    pub reason: Option<String>,
    /// Parsed severity annotation.
    pub severity: Severity,
    /// Originating policy request kind.
    pub origin: RequestKind,
}

/// Self-contained Cedar evaluation input.
///
/// Adapter-driven lowering produces this from a transaction; the policy engine
/// consumes it. The request can be serialized, logged, replayed, and built by
/// hand in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRequest {
    /// Cedar `EntityUid` for the principal — e.g., `Wallet::"0xUser"`.
    pub principal: String,
    /// Cedar `EntityUid` for the action — e.g., `Action::"dex"`.
    pub action: String,
    /// Cedar `EntityUid` for the resource — e.g., `Protocol::"dex-v3"`.
    pub resource: String,
    /// Cedar entities array (JSON form Cedar accepts).
    pub entities: JsonValue,
    /// Cedar context record (JSON form).
    pub context: JsonValue,
}

impl PolicyRequest {
    /// Construct a policy request from Cedar request components.
    #[must_use]
    pub fn new(
        principal: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        entities: JsonValue,
        context: JsonValue,
    ) -> Self {
        Self {
            principal: principal.into(),
            action: action.into(),
            resource: resource.into(),
            entities,
            context,
        }
    }
}

/// Error produced while parsing, validating, or evaluating policies.
#[derive(Debug, Error)]
pub enum PolicyError {
    /// Cedar policy parse failure.
    #[error("failed to parse Cedar policy: {0}")]
    Parse(String),
    /// Cedar schema parse failure.
    #[error("failed to parse Cedar schema: {0}")]
    Schema(String),
    /// Cedar schema validation failure.
    #[error("failed to validate Cedar policy set against schema: {0}")]
    Validation(String),
    /// Cedar request construction failure.
    #[error("failed to build Cedar request: {0}")]
    Request(String),
    /// Cedar context construction failure.
    #[error("failed to build Cedar context: {0}")]
    Context(String),
    /// Cedar entities construction failure.
    #[error("failed to build Cedar entities: {0}")]
    Entities(String),
    /// Cedar entity uid construction failure.
    #[error("invalid entity uid: {0}")]
    EntityUid(String),
    /// Semantic action lowering failure before Cedar request construction.
    #[error("lowering failed: {0}")]
    Lowering(String),
}

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
    /// The baseline permit and the bundled Cedar schema are added automatically.
    #[must_use]
    pub fn builder() -> PolicyEngineBuilder {
        PolicyEngineBuilder::new()
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

    /// Internal: build a `PolicyEngine` from already-collected policies.
    /// `text_combined` is one big Cedar source string (with baseline
    /// prepended).
    fn build_from(text_combined: &str, schema: Schema) -> Result<Self, PolicyError> {
        let mut policy_set = PolicySet::new();
        let mut severities: HashMap<String, Severity> = HashMap::new();
        let mut reasons: HashMap<String, String> = HashMap::new();

        // ---- Text policies ------------------------------------------------
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
        self.evaluate_request_with_origin(req, RequestKind::Action)
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
        origin: RequestKind,
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
        I: IntoIterator<Item = (&'a PolicyRequest, RequestKind)>,
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
        //    `all_matches` includes any *also*-fired warn entries (warn-union).
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
                origin: RequestKind::Action,
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

/// Builder for `PolicyEngine`.
///
/// Accepts text Cedar policy sources via `add_text`. Always injects the
/// baseline permit policy so absence of any matched `forbid` evaluates
/// to allow.
#[derive(Debug)]
pub struct PolicyEngineBuilder {
    text_sources: Vec<String>,
    schema_sources: Vec<String>,
}

impl Default for PolicyEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngineBuilder {
    /// Construct a builder pre-loaded with the bundled Cedar schema
    /// (`core + dex + other`). The bundled schema is mandatory: every
    /// engine produced by this builder is strict-validated.
    ///
    /// To extend the schema with adapter-contributed fragments, chain
    /// `add_schema_text(...)` after `new()`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text_sources: Vec::new(),
            schema_sources: vec![PolicySchemaComposer::new().compose()],
        }
    }

    /// Append one or more text Cedar policies. Multiple `forbid`/`permit`
    /// clauses in the string are fine — they'll all be parsed.
    #[must_use]
    pub fn add_text<S: Into<String>>(mut self, src: S) -> Self {
        self.text_sources.push(src.into());
        self
    }

    /// Extend the schema with an additional Cedar fragment. The bundled
    /// schema (`core + dex + other`) is already pre-loaded by `new()`, so
    /// this method is for adapter- or host-contributed fragments only.
    /// Re-declaring an entity, type, or action that the bundled schema
    /// already provides is a parse error (`Wallet declared twice`); pass
    /// only fragments that introduce new declarations, ideally inside their
    /// own `namespace` block.
    #[must_use]
    pub fn add_schema_text<S: Into<String>>(mut self, src: S) -> Self {
        self.schema_sources.push(src.into());
        self
    }

    /// Finish the builder, producing a ready-to-evaluate `PolicyEngine`.
    ///
    /// # Errors
    ///
    /// Returns an error when schema parsing, policy parsing, or policy
    /// validation fails.
    pub fn build(self) -> Result<PolicyEngine, PolicyError> {
        // schema_sources is non-empty by construction (PolicyEngineBuilder::new()
        // pre-loads the bundled schema and there is no public path that empties it).
        let mut combined_schema = String::new();
        for src in &self.schema_sources {
            combined_schema.push_str(src);
            combined_schema.push('\n');
        }
        let (schema, _warnings) = Schema::from_cedarschema_str(&combined_schema)
            .map_err(|e| PolicyError::Schema(e.to_string()))?;

        let baseline = "@id(\"engine/baseline-allow\")\npermit(principal, action, resource);\n";
        let mut combined = String::new();
        combined.push_str(baseline);
        for src in &self.text_sources {
            combined.push_str(src);
            combined.push('\n');
        }
        PolicyEngine::build_from(&combined, schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entities() -> JsonValue {
        json!([
            {
                "uid": { "type": "Wallet", "id": "0xUser" },
                "attrs": {},
                "parents": []
            },
            {
                "uid": { "type": "Protocol", "id": "dex-v3" },
                "attrs": {},
                "parents": []
            }
        ])
    }

    fn context_dex(usd_value: &str) -> JsonValue {
        json!({
            "target": "0x0000000000000000000000000000000000000002",
            "valueWei": "0",
            "protocolIds": ["uniswap-v3"],
            "inputTokens": [],
            "outputTokens": [],
            "totalInputUsd": {
                "value": { "__extn": { "fn": "decimal", "arg": usd_value } },
                "asOfTs": 1,
                "staleSec": 5,
                "sources": ["mock"],
            },
            "hasZeroMinOutput": false,
            "hasExternalRecipient": false,
        })
    }

    #[test]
    fn empty_policy_set_allows_everything() {
        let engine = PolicyEngine::from_sources(Vec::<&str>::new()).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
                &entities(),
                &context_dex("50.00"),
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
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
                &entities(),
                &context_dex("200.00"),
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
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
                &entities(),
                &context_dex("50.00"),
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
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
                &entities(),
                &context_dex("200.00"),
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
        // Both policies fire on a $200 swap. Deny-overrides → Verdict::Fail,
        // and Fail's vec must contain BOTH the deny entry and the warn entry
        // (warn-union: no info loss). Each entry carries its own severity.
        let policy = r#"
            @id("user/large-swap-warning")
            @severity("warn")
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };

            @id("user/huge-swap-deny")
            @severity("deny")
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("150.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
                &entities(),
                &context_dex("200.00"),
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
            forbid (principal, action == Action::"dex", resource)
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
        // The unbundled `from_sources` constructor must apply the bundled
        // Cedar schema (core + dex + other) so a policy with a typo in a
        // DexContext field is rejected at build time, not silently accepted.
        let policy = r#"
            @id("bad/from-sources-context-typo")
            @severity("deny")
            forbid (principal, action == Action::"dex", resource)
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
            forbid (principal, action == Action::"dex", resource)
            when { context.totalInputUSd.value.greaterThan(decimal("100.00")) };
        "#;

        // builder() pre-loads the bundled schema; no explicit add_schema_text
        // is needed.
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
            forbid (principal, action == Action::"dex", resource)
            when {
              context has totalInputUsd &&
              context.totalInputUsd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::builder().add_text(policy).build().unwrap();

        let err = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"dex""#,
                r#"Protocol::"dex""#,
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
            forbid (principal, action == Action::"dex", resource);
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let request = PolicyRequest::new(
            r#"Wallet::"0xUser""#,
            r#"Action::"dex""#,
            r#"Protocol::"dex""#,
            entities(),
            context_dex("50.00"),
        );

        let verdict = engine.evaluate_request(&request).unwrap();

        match verdict {
            Verdict::Fail(matched) => {
                assert_eq!(matched[0].origin, RequestKind::Action);
            }
            other => panic!("expected Verdict::Fail, got {other:?}"),
        }
    }
}
