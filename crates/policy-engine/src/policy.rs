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
    PolicySet, Request,
};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use thiserror::Error;

/// Final, host-facing verdict. Tri-state: `Pass` carries no data, `Warn` and
/// `Fail` carry the list of matched policies that drove the verdict.
///
/// Variant semantics map directly onto the spec's deny-overrides + warn-union
/// rule:
/// - any matched `forbid` with `@severity("deny")` → `Fail(...)`
/// - otherwise, any matched `forbid` with `@severity("warn")` → `Warn(...)`
/// - otherwise → `Pass`
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Pass,
    Warn(Vec<MatchedPolicy>),
    Fail(Vec<MatchedPolicy>),
}

impl Verdict {
    /// True iff the host wallet must refuse to sign.
    pub fn is_failure(&self) -> bool {
        matches!(self, Verdict::Fail(_))
    }

    /// True iff there's at least one warning the host should display.
    pub fn has_warnings(&self) -> bool {
        self.matched().iter().any(|m| m.severity == Severity::Warn)
    }

    /// Returns the matched policies for the warn / fail variants; empty for
    /// `Pass`. Useful when the caller wants to render diagnostics regardless
    /// of variant.
    pub fn matched(&self) -> &[MatchedPolicy] {
        match self {
            Verdict::Pass => &[],
            Verdict::Warn(v) | Verdict::Fail(v) => v,
        }
    }

    pub fn aggregate<I>(verdicts: I) -> Self
    where
        I: IntoIterator<Item = Verdict>,
    {
        let mut matched = Vec::new();
        let mut any_deny = false;
        let mut any_warn = false;

        for verdict in verdicts {
            match verdict {
                Verdict::Pass => {}
                Verdict::Warn(mut v) => {
                    any_warn = true;
                    matched.append(&mut v);
                }
                Verdict::Fail(mut v) => {
                    any_deny = true;
                    any_warn |= v.iter().any(|m| m.severity == Severity::Warn);
                    matched.append(&mut v);
                }
            }
        }

        if any_deny {
            Verdict::Fail(matched)
        } else if any_warn {
            Verdict::Warn(matched)
        } else {
            Verdict::Pass
        }
    }
}

/// `@severity("deny" | "warn")` annotation parsed from a policy's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Deny,
    Warn,
}

/// One `forbid` clause that fired during evaluation. Severity is preserved
/// per-element so that, when a `Verdict::Fail` is produced because a deny
/// fired, any *also*-fired warns are still reported (no info loss).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestKind {
    Leaf { index: usize },
    Tx,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchedPolicy {
    pub policy_id: String,
    pub reason: Option<String>,
    pub severity: Severity,
    pub origin: RequestKind,
}

/// Self-contained Cedar evaluation input. `Adapter`-driven lowering produces
/// this from a transaction; `PolicyEngine::evaluate_request` consumes it.
/// Designed so it can be serialized, logged, replayed, and built by hand in
/// tests.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyRequest {
    /// Cedar `EntityUid` for the principal — e.g., `Wallet::"0xUser"`.
    pub principal: String,
    /// Cedar `EntityUid` for the action — e.g., `Action::"swap"`.
    pub action: String,
    /// Cedar `EntityUid` for the resource — e.g., `Protocol::"uniswap-v3"`.
    pub resource: String,
    /// Cedar entities array (JSON form Cedar accepts).
    pub entities: JsonValue,
    /// Cedar context record (JSON form).
    pub context: JsonValue,
}

impl PolicyRequest {
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

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("failed to parse Cedar policy: {0}")]
    Parse(String),
    #[error("failed to build Cedar request: {0}")]
    Request(String),
    #[error("failed to build Cedar context: {0}")]
    Context(String),
    #[error("failed to build Cedar entities: {0}")]
    Entities(String),
    #[error("invalid entity uid: {0}")]
    EntityUid(String),
}

/// Compiled policy set + the auto-injected baseline permit.
#[derive(Debug)]
pub struct PolicyEngine {
    policy_set: PolicySet,
    /// Per-policy-id severity, lifted from the `@severity(...)` annotation at
    /// parse time so we don't have to re-parse on every evaluation.
    severities: HashMap<String, Severity>,
    /// Per-policy-id @reason annotation.
    reasons: HashMap<String, String>,
}

impl PolicyEngine {
    /// Start a builder. Callers chain `.add_text(...)` and/or `.add_json(...)`
    /// and finish with `.build()`. The baseline permit is added automatically.
    pub fn builder() -> PolicyEngineBuilder {
        PolicyEngineBuilder::default()
    }

    /// Convenience: build an engine from one or more text Cedar sources only.
    /// Equivalent to `PolicyEngine::builder().add_text(...).build()`.
    pub fn from_sources<I, S>(sources: I) -> Result<Self, PolicyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut b = PolicyEngine::builder();
        for s in sources {
            b = b.add_text(s.as_ref());
        }
        b.build()
    }

    /// Internal: build a `PolicyEngine` from already-collected policies.
    /// `text_combined` is one big Cedar source string (with baseline prepended);
    /// `json_policies` are each one full CedarJSON policy object.
    fn build_from(
        text_combined: String,
        json_policies: Vec<JsonValue>,
    ) -> Result<Self, PolicyError> {
        let mut policy_set = PolicySet::new();
        let mut severities: HashMap<String, Severity> = HashMap::new();
        let mut reasons: HashMap<String, String> = HashMap::new();

        // ---- Text policies ------------------------------------------------
        let initial_set =
            PolicySet::from_str(&text_combined).map_err(|e| PolicyError::Parse(e.to_string()))?;

        for p in initial_set.policies() {
            ingest_policy(p, &mut policy_set, &mut severities, &mut reasons)?;
        }

        // ---- JSON policies ------------------------------------------------
        for (i, json) in json_policies.into_iter().enumerate() {
            let placeholder_id: PolicyId = format!("__json_policy_{i}")
                .parse()
                .expect("PolicyId parse is infallible");
            let p = Policy::from_json(Some(placeholder_id), json)
                .map_err(|e| PolicyError::Parse(e.to_string()))?;
            ingest_policy(&p, &mut policy_set, &mut severities, &mut reasons)?;
        }

        Ok(PolicyEngine {
            policy_set,
            severities,
            reasons,
        })
    }

    /// Evaluate a `PolicyRequest`. This is the preferred entry point.
    pub fn evaluate_request(&self, req: &PolicyRequest) -> Result<Verdict, PolicyError> {
        self.evaluate_request_with_origin(req, RequestKind::Leaf { index: 0 })
    }

    /// Evaluate a `PolicyRequest` and annotate matches with the originating
    /// request kind.
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
                    matched.origin = origin.clone();
                }
            }
        }
        Ok(verdict)
    }

    /// Evaluate requests from one transaction while preserving request kind.
    /// A request list with empty input is treated as pass.
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

        let entities = Entities::from_json_value(entities_json.clone(), None)
            .map_err(|e| PolicyError::Entities(e.to_string()))?;
        let context = Context::from_json_value(context_json.clone(), None)
            .map_err(|e| PolicyError::Context(e.to_string()))?;

        let request = Request::new(principal, action, resource, context, None)
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
                origin: RequestKind::Leaf { index: 0 },
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

fn annotation(p: &Policy, key: &str) -> Option<String> {
    p.annotation(key).map(|s| s.to_string())
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
    let user_id: PolicyId = user_id_raw.parse().expect("PolicyId parse is infallible");
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

/// Builder for `PolicyEngine`. Accepts text Cedar sources and/or JSON-encoded
/// CedarJSON policy objects, mixed in any order. Always injects the baseline
/// `permit(principal, action, resource);` so that absence of any matched
/// `forbid` evaluates to allow.
#[derive(Debug, Default)]
pub struct PolicyEngineBuilder {
    text_sources: Vec<String>,
    json_policies: Vec<JsonValue>,
}

impl PolicyEngineBuilder {
    /// Append one or more text Cedar policies. Multiple `forbid`/`permit`
    /// clauses in the string are fine — they'll all be parsed.
    pub fn add_text<S: Into<String>>(mut self, src: S) -> Self {
        self.text_sources.push(src.into());
        self
    }

    /// Append one CedarJSON policy object (the format produced by
    /// `Policy::to_json`). Each call adds exactly one policy.
    pub fn add_json(mut self, json: JsonValue) -> Self {
        self.json_policies.push(json);
        self
    }

    /// Convenience: parse a JSON string and append. Returns the builder back
    /// in the `Ok` arm so it can chain.
    pub fn add_json_str(self, src: &str) -> Result<Self, PolicyError> {
        let v: JsonValue = serde_json::from_str(src)
            .map_err(|e| PolicyError::Parse(format!("invalid JSON: {e}")))?;
        Ok(self.add_json(v))
    }

    /// Finish the builder, producing a ready-to-evaluate `PolicyEngine`.
    pub fn build(self) -> Result<PolicyEngine, PolicyError> {
        let baseline = "@id(\"engine/baseline-allow\")\npermit(principal, action, resource);\n";
        let mut combined = String::new();
        combined.push_str(baseline);
        for src in &self.text_sources {
            combined.push_str(src);
            combined.push('\n');
        }
        PolicyEngine::build_from(combined, self.json_policies)
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
                "uid": { "type": "Protocol", "id": "uniswap-v3" },
                "attrs": {},
                "parents": []
            }
        ])
    }

    fn context_swap(usd_value: &str) -> JsonValue {
        json!({
            "inputAmount": {
                "tokenSymbol": "USDT",
                "raw": "200000000",
                "human": "200.000000",
                "usd": {
                    "value": { "__extn": { "fn": "decimal", "arg": usd_value } },
                    "staleSec": 5,
                }
            }
        })
    }

    #[test]
    fn empty_policy_set_allows_everything() {
        let engine = PolicyEngine::from_sources(Vec::<&str>::new()).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"uniswap-v3""#,
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
              context.inputAmount.usd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"uniswap-v3""#,
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
              context.inputAmount.usd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"uniswap-v3""#,
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
              context.inputAmount.usd.value.greaterThan(decimal("100.00"))
            };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"uniswap-v3""#,
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
        // Both policies fire on a $200 swap. Deny-overrides → Verdict::Fail,
        // and Fail's vec must contain BOTH the deny entry and the warn entry
        // (warn-union: no info loss). Each entry carries its own severity.
        let policy = r#"
            @id("user/large-swap-warning")
            @severity("warn")
            forbid (principal, action == Action::"swap", resource)
            when { context.inputAmount.usd.value.greaterThan(decimal("100.00")) };

            @id("user/huge-swap-deny")
            @severity("deny")
            forbid (principal, action == Action::"swap", resource)
            when { context.inputAmount.usd.value.greaterThan(decimal("150.00")) };
        "#;
        let engine = PolicyEngine::from_sources([policy]).unwrap();
        let v = engine
            .evaluate(
                r#"Wallet::"0xUser""#,
                r#"Action::"swap""#,
                r#"Protocol::"uniswap-v3""#,
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
}
