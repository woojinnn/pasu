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
