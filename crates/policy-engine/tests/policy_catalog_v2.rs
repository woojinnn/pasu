//! Policy catalog v2 — research-driven example policy+manifest sets (Phase C).
//!
//! `tests/fixtures/policy_catalog_v2/<id>/{manifest.json, policy.cedar}` is a
//! curated catalog of ~50 real-world wallet pre-sign security policies authored
//! against the v2 ActionBody model. Unlike `default_policies_v2/` (the 9 shipped
//! defaults), these are NOT auto-installed — they are a validated *corpus* that
//! (a) demonstrates the policy language across every domain and (b) drives the
//! enrichment-method spec (`browser-extension/backend/service-worker/POLICY_RPC_METHODS.md`).
//!
//! Enrichment policies (those with `policy_rpc` calls populating `context.custom.*`)
//! compile here but stay **dormant** at runtime until a `/v1/rpc` dispatcher serves
//! their methods — the `context.custom has <field>` guard is simply false while the
//! field is absent, so the policy is inert (never a false verdict).
//!
//! This gate pins the same consistency guarantee as the default set: every catalog
//! bundle must be structurally valid AND its `policy.cedar` must compile against the
//! `.cedarschema` its own manifest synthesizes via `compose_per_policy`.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine::policy::PolicyEngine;
use policy_engine::policy_rpc::ManifestV2;
use policy_engine::schema::compose_per_policy;

fn catalog_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/policy_catalog_v2")
}

/// Recursively collect every leaf "set" directory — one that directly contains a
/// `manifest.json`. The catalog is organised into a precedence-ordered bucket tree
/// (`compliance/ > protocol/ > wallet/ > action/`, each sub-categorised), so sets
/// live two-plus levels deep; this walker descends through any depth. Entries whose
/// name starts with `_` (the shared `_methods/` impl-spec library, `_index*`) are
/// skipped — they are not policy sets.
fn walk_catalog_sets(root: &Path) -> Vec<PathBuf> {
    let mut sets = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
        {
            continue;
        }
        if dir.join("manifest.json").is_file() {
            sets.push(dir);
            continue; // a set is a leaf — do not descend into it
        }
        for entry in fs::read_dir(&dir).expect("read catalog dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    sets.sort();
    sets
}

/// Every catalog bundle is internally consistent: the manifest parses + validates,
/// its id matches the on-disk directory, and the policy compiles against the schema
/// the manifest synthesizes.
#[test]
fn policy_catalog_v2_bundles_are_internally_consistent() {
    let dir = catalog_dir();
    let mut checked = 0;
    // Collect EVERY failure rather than panicking on the first — a large catalog is
    // authored/edited in bulk, so one run must surface the full fix-list at once.
    let mut failures: Vec<(String, String)> = Vec::new();

    for bundle in walk_catalog_sets(&dir) {
        let id = bundle
            .file_name()
            .expect("bundle dir name")
            .to_string_lossy()
            .into_owned();
        match check_bundle(&bundle, &id) {
            Ok(()) => checked += 1,
            Err(e) => failures.push((id, e)),
        }
    }

    if !failures.is_empty() {
        let mut msg = format!("{} catalog bundle(s) failed:\n", failures.len());
        for (id, e) in &failures {
            msg.push_str(&format!("  ✗ {id}: {e}\n"));
        }
        panic!("{msg}");
    }

    assert!(
        checked >= 45,
        "expected >= 45 catalog bundles in {dir:?}, found {checked}"
    );
}

/// All three consistency checks for one bundle, returning the first error as a
/// string instead of panicking — so the caller can aggregate failures across the
/// whole catalog and report them together.
fn check_bundle(bundle: &Path, id: &str) -> Result<(), String> {
    let manifest_json = fs::read_to_string(bundle.join("manifest.json"))
        .map_err(|e| format!("read manifest.json: {e}"))?;
    let policy = fs::read_to_string(bundle.join("policy.cedar"))
        .map_err(|e| format!("read policy.cedar: {e}"))?;

    let manifest: ManifestV2 =
        serde_json::from_str(&manifest_json).map_err(|e| format!("parse manifest.json: {e}"))?;

    // 1. The manifest id is the bundle directory name (stable on-disk layout).
    if manifest.id != id {
        return Err(format!(
            "manifest id {:?} must match its directory name",
            manifest.id
        ));
    }

    // 2. Structural invariants (schema_version == 2, unique policy_rpc ids,
    //    every custom_context field fed by some output).
    manifest
        .validate()
        .map_err(|e| format!("manifest invalid: {e}"))?;

    // 3. The policy compiles against the schema its own manifest synthesizes —
    //    the field/type/action-uid correctness gate.
    let schema = compose_per_policy(&manifest).map_err(|e| format!("compose_per_policy: {e}"))?;
    PolicyEngine::build_from_per_policy(&[(policy, schema)])
        .map_err(|e| format!("policy.cedar does not compile against its schema: {e}"))?;

    Ok(())
}
