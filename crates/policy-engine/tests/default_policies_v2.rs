//! Default v2 (ActionBody-model) policy bundles — source-of-truth + consistency gate.
//!
//! `tests/fixtures/default_policies_v2/<id>/{manifest.json, policy.cedar}` is the
//! canonical default v2 policy set the browser extension ships and installs at
//! boot (the cutover's `bundles[]` for `evaluate_action_v2_json`). This mirrors
//! the existing pattern where the v1 seed *decode* bundle ships from a Rust
//! fixture as the single source of truth.
//!
//! The v2 verdict ENGINE (lower → plan → materialize → evaluate) is already
//! proven elsewhere (`policy_rpc::materialize_v2` end-to-end test,
//! `lowering_v2::amm::swap` end-to-end test). This gate pins the SHIPPED
//! ARTIFACTS: every default bundle must be structurally valid AND its
//! `policy.cedar` must compile against the `.cedarschema` its own manifest
//! synthesizes via `compose_per_policy` — the consistency guarantee a default
//! bundle needs before it can be shipped.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine::policy::PolicyEngine;
use policy_engine::policy_rpc::ManifestV2;
use policy_engine::schema::compose_per_policy;

fn default_policies_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/default_policies_v2")
}

/// A bundle is BLOCKED-BY-ACTION iff its `policy.cedar` carries the
/// `// BLOCKED-BY-ACTION` banner — it targets an action surface not yet in the
/// schema (e.g. x402 / EIP-3009 `Token::Erc3009TransferWithAuth`), so it cannot
/// compile against any synthesized schema until that surface lands. Such
/// bundles are staged in-tree (phase-not-classified) but skipped by every
/// consumer until the banner is removed. See default_policies_v2/README.md.
fn is_blocked_by_action(bundle: &Path) -> bool {
    fs::read_to_string(bundle.join("policy.cedar"))
        .map(|s| s.contains("// BLOCKED-BY-ACTION"))
        .unwrap_or(false)
}

/// Collect every policy bundle dir under `root`, at ANY nesting depth. A
/// directory is a bundle iff it directly contains a `manifest.json`; any other
/// directory (`phaseN/`, `phase1/A/`, …) is a grouping dir and is recursed
/// into. Supports the flat `<root>/<id>/`, phased `<root>/<phaseN>/<id>/`, and
/// nested `<root>/<phaseN>/<sub>/<id>/` layouts alike. Non-dir entries
/// (e.g. `.DS_Store`) are skipped. BLOCKED-BY-ACTION bundles are skipped.
fn collect_bundles(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read default_policies_v2 fixture dir") {
            let entry = entry.expect("dir entry");
            if !entry.file_type().expect("file type").is_dir() {
                continue;
            }
            let path = entry.path();
            if path.join("manifest.json").is_file() {
                // a bundle dir — do not descend into it. Skip BLOCKED-BY-ACTION
                // bundles (action surface not yet in schema; staged but inert).
                if !is_blocked_by_action(&path) {
                    out.push(path);
                }
            } else {
                walk(&path, out); // a grouping dir (phaseN, A/B, …) — recurse
            }
        }
    }
    let mut bundles = Vec::new();
    walk(root, &mut bundles);
    bundles
}

/// Every shipped default v2 bundle is internally consistent: the manifest
/// parses + validates, its id matches the on-disk directory, and the policy
/// compiles against the schema the manifest synthesizes.
#[test]
fn default_v2_bundles_are_internally_consistent() {
    let dir = default_policies_dir();
    let mut checked = 0;

    for bundle in collect_bundles(&dir) {
        let id = bundle
            .file_name()
            .expect("bundle dir name")
            .to_string_lossy()
            .into_owned();

        let manifest_json = fs::read_to_string(bundle.join("manifest.json"))
            .unwrap_or_else(|e| panic!("read {id}/manifest.json: {e}"));
        let policy = fs::read_to_string(bundle.join("policy.cedar"))
            .unwrap_or_else(|e| panic!("read {id}/policy.cedar: {e}"));

        let manifest: ManifestV2 = serde_json::from_str(&manifest_json)
            .unwrap_or_else(|e| panic!("parse {id}/manifest.json: {e}"));

        // 1. The manifest id is the bundle directory name (stable on-disk layout).
        assert_eq!(
            manifest.id, id,
            "{id}: manifest id must match its directory name"
        );

        // 2. Structural invariants (schema_version == 2, unique policy_rpc ids,
        //    every custom_context field fed by some output).
        manifest
            .validate()
            .unwrap_or_else(|e| panic!("{id}: manifest invalid: {e}"));

        // 3. The shipped policy compiles against the schema its own manifest
        //    synthesizes — the core consistency guarantee for a default bundle.
        let schema = compose_per_policy(&manifest)
            .unwrap_or_else(|e| panic!("{id}: compose_per_policy failed: {e}"));
        PolicyEngine::build_from_per_policy(&[(policy, schema)]).unwrap_or_else(|e| {
            panic!("{id}: policy.cedar does not compile against its schema: {e}")
        });

        checked += 1;
    }

    assert!(
        checked >= 1,
        "expected at least one default v2 bundle in {dir:?}"
    );
}

// ── N5 — deny × optional-enrichment ship-gate ──────────────────────────────
// An enrichment call that is `optional` (skipped when a param selector is
// missing), or whose output is not `required`, may leave its `context.custom.*`
// field ABSENT at evaluation time. A pure-static `warn` policy guarding such a
// field with `context.custom has X` is fine — it simply doesn't warn. But a
// `forbid` (`@severity("deny")`) whose firing hinges on that field will silently
// NOT deny when the field is absent — a fail-open. This gate forbids shipping
// such a bundle. (Conservative + bundle-level: a bundle mixing a deny policy on
// a required field with a warn policy on an optional field could over-flag, but
// shipped defaults are one policy each.)

/// Custom fields a deny policy must not hinge on: those NOT guaranteed present
/// (every feeder is an `optional` call or a non-`required` output) AND
/// referenced by a `@severity("deny")` policy. Returns the offending field names.
fn deny_optional_violations(manifest: &ManifestV2, policy: &str) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut all_fed: BTreeSet<String> = BTreeSet::new();
    let mut guaranteed: BTreeSet<String> = BTreeSet::new();
    for spec in &manifest.policy_rpc {
        for out in &spec.outputs {
            all_fed.insert(out.field.clone());
            if out.required && !spec.optional {
                guaranteed.insert(out.field.clone());
            }
        }
    }

    // Only a deny (forbid) policy can fail OPEN on a missing field.
    if !policy.contains("@severity(\"deny\")") {
        return Vec::new();
    }

    all_fed
        .difference(&guaranteed)
        .filter(|f| {
            policy.contains(&format!("context.custom.{f}"))
                || policy.contains(&format!("context.custom has {f}"))
        })
        .cloned()
        .collect()
}

/// No shipped default deny policy may hinge solely on an optional/non-required
/// enrichment field (it would silently skip → never deny → fail-open).
#[test]
fn no_default_deny_policy_depends_only_on_optional_enrichment() {
    let dir = default_policies_dir();
    for entry in fs::read_dir(&dir).expect("read default_policies_v2 fixture dir") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let bundle = entry.path();
        let id = bundle
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let manifest_json = fs::read_to_string(bundle.join("manifest.json"))
            .unwrap_or_else(|e| panic!("read {id}/manifest.json: {e}"));
        let policy = fs::read_to_string(bundle.join("policy.cedar"))
            .unwrap_or_else(|e| panic!("read {id}/policy.cedar: {e}"));
        let manifest: ManifestV2 = serde_json::from_str(&manifest_json)
            .unwrap_or_else(|e| panic!("parse {id}/manifest.json: {e}"));

        let violations = deny_optional_violations(&manifest, &policy);
        assert!(
            violations.is_empty(),
            "{id}: deny policy hinges on optional/non-required enrichment field(s) \
             {violations:?} — they may be absent at eval → forbid never fires \
             (fail-open). Make the feeding output required + the call non-optional, \
             or downgrade the policy to warn."
        );
    }
}

/// The gate actually detects a violation (a deny forbidding on an `optional`
/// call's non-`required` output).
#[test]
fn deny_optional_gate_detects_a_synthetic_violation() {
    let manifest: ManifestV2 = serde_json::from_str(
        r#"{
            "id": "synthetic-deny-optional",
            "schema_version": 2,
            "policy_rpc": [{
                "id": "rep",
                "method": "address.reputation",
                "params": {},
                "outputs": [{
                    "kind": "context", "field": "flagged", "type": "Bool",
                    "from": "$.result.flagged", "required": false
                }],
                "optional": true
            }],
            "custom_context": { "fields": { "flagged": "Bool" } }
        }"#,
    )
    .expect("synthetic manifest parses");
    let policy = "@id(\"x\")\n@severity(\"deny\")\nforbid(principal, action, resource) \
         when { context has custom && context.custom has flagged && context.custom.flagged };\n";

    let violations = deny_optional_violations(&manifest, policy);
    assert!(
        violations.contains(&"flagged".to_string()),
        "gate must flag the deny-on-optional 'flagged' field, got {violations:?}"
    );

    // Control: the SAME field fed by a required, non-optional output is fine.
    let ok_manifest: ManifestV2 = serde_json::from_str(
        r#"{
            "id": "synthetic-deny-required",
            "schema_version": 2,
            "policy_rpc": [{
                "id": "rep", "method": "address.reputation", "params": {},
                "outputs": [{
                    "kind": "context", "field": "flagged", "type": "Bool",
                    "from": "$.result.flagged", "required": true
                }],
                "optional": false
            }],
            "custom_context": { "fields": { "flagged": "Bool" } }
        }"#,
    )
    .expect("control manifest parses");
    assert!(
        deny_optional_violations(&ok_manifest, policy).is_empty(),
        "a deny on a required+non-optional field must NOT be flagged"
    );
}
