//! Phase 2 Task 2.3 — every shipped `extensions/<cat>/<action>.policy-rpc.json`
//! must round-trip through `manifest_to_cedarschema`. This proves that:
//!
//! 1. The manifest deserializes against `PolicyManifest` (so every
//!    `outputs[].type` is in the closed `ProjectionType` set).
//! 2. The manifest passes all 10 manifest validation rules for its action,
//!    including Rule 4 (no collision with base context fields) and Rule 10
//!    (declared `context_extensions` match derived outputs - which after
//!    Phase 2 means the block must be absent or empty since the composer
//!    derives it).

use policy_engine::policy_rpc::PolicyManifest;
use policy_engine::schema::manifest_to_cedarschema;
use std::path::Path;

#[test]
fn every_extension_manifest_validates_via_manifest_to_cedarschema() {
    let root = Path::new("../../schema/policy-schema/extensions");
    assert!(root.exists(), "extensions root missing: {}", root.display());

    let mut count = 0_usize;
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.expect("walkdir entry");
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.ends_with(".policy-rpc.json") {
            continue;
        }
        // Derive snake-case action from filename `<action>.policy-rpc.json`.
        let action = name.trim_end_matches(".policy-rpc.json");

        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let manifest: PolicyManifest = serde_json::from_str(&text)
            .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));

        // D2 / Rule 10 (Phase 2): the composer derives `context_extensions`,
        // so shipped manifests must not declare it. Old hand-authored blocks
        // are stale and would drift from the manifest's actual outputs.
        assert!(
            manifest.context_extensions.is_empty(),
            "{}: context_extensions is retired in Phase 2; the composer derives it from outputs",
            path.display()
        );

        manifest_to_cedarschema(action, &manifest)
            .unwrap_or_else(|err| panic!("validate {}: {err}", path.display()));

        count += 1;
    }
    assert!(count > 0, "expected at least one extension manifest");
}
