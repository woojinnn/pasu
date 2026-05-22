//! Build-time JSON Schema constraint extractor.
//!
//! Walks `schema/action-schema/schema/actions/**/*.json` plus
//! `schema/action-schema/schema/common/_common.json`, resolves `$ref` to the
//! common `$defs`, and emits a Rust file that maps `(action, field_path)`
//! to the constraints declared in the JSON Schema:
//!
//! - **enum** (closed-set string values): exposed via
//!   `action_field_enum(action, path) -> Option<&'static [&'static str]>`.
//!   Used by `FieldSpec::allowed_values` and the WASM `FieldDto.allowedValues`
//!   so the UI renders a dropdown and the validator rejects out-of-set
//!   literals.
//!
//! - **pattern** (string regex): exposed via
//!   `action_field_pattern(action, path) -> Option<&'static str>`. Mirrors
//!   the `"pattern"` keyword used on primitives like `Address`
//!   (`^0x[0-9a-fA-F]{40}$`) and `DecimalString` (`^[0-9]+$`). Used by
//!   `FieldSpec::pattern` so the validator catches typo'd inputs at
//!   compile time instead of letting a syntactically valid Cedar policy
//!   silently never match.
//!
//! The generated file is `include!`'d from `schemas::generated` so any
//! action schema module can call these lookups instead of repeating
//! literals that the upstream JSON already declared.
//!
//! Cargo invariants:
//! - `cargo:rerun-if-changed` is emitted for every JSON file inspected,
//!   so a schema edit triggers a rebuild without touching `src/`.
//! - Output lives in `$OUT_DIR/generated_action_constraints.rs`; the
//!   crate source never contains a checked-in copy.

use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let schema_root = manifest_dir
        .join("..")
        .join("..")
        .join("schema")
        .join("action-schema")
        .join("schema");

    println!("cargo:rerun-if-changed={}", schema_root.display());

    let common_path = schema_root.join("common").join("_common.json");
    let common_defs = load_common_defs(&common_path);
    println!("cargo:rerun-if-changed={}", common_path.display());

    let actions_dir = schema_root.join("actions");
    let mut collected: BTreeMap<String, Vec<FieldConstraints>> = BTreeMap::new();
    collect_actions(&actions_dir, &mut collected, &common_defs);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_path = out_dir.join("generated_action_constraints.rs");
    fs::write(&out_path, render_rust(&collected))
        .unwrap_or_else(|e| panic!("write {}: {}", out_path.display(), e));
}

/// One JSON-Schema-declared constraint set for a single dotted leaf path.
/// Both fields may be present, both absent, or any combination — the
/// renderer emits each only when it's set.
#[derive(Debug)]
struct FieldConstraints {
    path: String,
    enum_values: Option<Vec<String>>,
    pattern: Option<String>,
}

impl FieldConstraints {
    fn empty(path: String) -> Self {
        Self {
            path,
            enum_values: None,
            pattern: None,
        }
    }
    fn is_empty(&self) -> bool {
        self.enum_values.is_none() && self.pattern.is_none()
    }
}

fn load_common_defs(path: &Path) -> BTreeMap<String, Value> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let root: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e));
    let defs = root
        .get("$defs")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("missing $defs in {}", path.display()));
    defs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn collect_actions(
    actions_dir: &Path,
    out: &mut BTreeMap<String, Vec<FieldConstraints>>,
    common_defs: &BTreeMap<String, Value>,
) {
    for entry in fs::read_dir(actions_dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {}", actions_dir.display(), e))
    {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_actions(&path, out, common_defs);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());

        let action_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| panic!("bad file name: {}", path.display()));

        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        let root: Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("parse {}: {}", path.display(), e));

        let mut entries: Vec<FieldConstraints> = Vec::new();
        if let Some(props) = root.get("properties").and_then(Value::as_object) {
            for (key, child) in props {
                walk(child, key, common_defs, &mut entries);
            }
        }
        entries.retain(|e| !e.is_empty());
        if !entries.is_empty() {
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            out.insert(action_name, entries);
        }
    }
}

/// Recursively walk a schema node, flattening composite types to dotted
/// leaf paths and recording any `enum` and/or `pattern` constraint
/// encountered. `$ref` is resolved against the common `$defs`; primitive
/// scalar defs (`Address`, `DecimalString`, `Hex`) thus get their pattern
/// surfaced on every leaf that references them, no manual mirroring.
///
/// JSON-Schema features the policy builder doesn't model (allOf/oneOf,
/// if-then, array `items`) are intentionally ignored: enum/pattern on
/// scalar leaves is all our generator can act on, so deeper structure
/// would be dead constraints.
fn walk(
    node: &Value,
    path: &str,
    common_defs: &BTreeMap<String, Value>,
    out: &mut Vec<FieldConstraints>,
) {
    let resolved = resolve_ref(node, common_defs);
    let effective = resolved.as_ref().unwrap_or(node);

    // Collect leaf-level constraints first (enum/pattern can coexist with
    // properties only via allOf, which we don't traverse — so a node that
    // has properties is treated purely as a record).
    let mut leaf = FieldConstraints::empty(path.to_string());

    if let Some(values) = effective.get("enum").and_then(Value::as_array) {
        let strings: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if !strings.is_empty() {
            leaf.enum_values = Some(strings);
        }
    }
    if let Some(pat) = effective.get("pattern").and_then(Value::as_str) {
        leaf.pattern = Some(pat.to_string());
    }
    if !leaf.is_empty() {
        out.push(leaf);
    }

    // Descend into nested records.
    if let Some(props) = effective.get("properties").and_then(Value::as_object) {
        for (key, child) in props {
            let next_path = format!("{path}.{key}");
            walk(child, &next_path, common_defs, out);
        }
    }
}

fn resolve_ref(node: &Value, common_defs: &BTreeMap<String, Value>) -> Option<Value> {
    let r = node.get("$ref").and_then(Value::as_str)?;
    let after_hash = r.split_once('#').map(|(_, t)| t).unwrap_or("");
    let name = after_hash.strip_prefix("/$defs/")?;
    common_defs.get(name).cloned()
}

fn render_rust(entries: &BTreeMap<String, Vec<FieldConstraints>>) -> String {
    let mut out = String::new();
    out.push_str("// @generated by build.rs from schema/action-schema/**/*.json — do not edit.\n");
    out.push_str("//\n");
    out.push_str("// Two lookups derived from the upstream JSON Schema, both keyed by the\n");
    out.push_str("// (action, dotted_leaf_path) pair. Each returns `None` when the path\n");
    out.push_str("// is unknown or carries no constraint of that kind.\n\n");

    // --- enum lookup --------------------------------------------------
    out.push_str(
        "/// Closed-set string enum declared on this field via the JSON\n\
         /// Schema `\"enum\"` keyword. Order matches the JSON declaration so\n\
         /// UIs render a stable dropdown.\n",
    );
    out.push_str("pub fn action_field_enum(action: &str, path: &str) -> Option<&'static [&'static str]> {\n");
    out.push_str("    match (action, path) {\n");
    for (action, items) in entries {
        for item in items {
            if let Some(values) = item.enum_values.as_ref() {
                out.push_str(&format!(
                    "        ({:?}, {:?}) => Some(&[",
                    action, item.path
                ));
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&format!("{:?}", v));
                }
                out.push_str("]),\n");
            }
        }
    }
    out.push_str("        _ => None,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    // --- pattern lookup -----------------------------------------------
    out.push_str(
        "/// Regex the operand string must match on this field, mirrored from\n\
         /// the JSON Schema `\"pattern\"` keyword. Resolved through any `$ref`\n\
         /// so primitive defs like `Address` propagate their pattern to every\n\
         /// leaf that references them.\n",
    );
    out.push_str("pub fn action_field_pattern(action: &str, path: &str) -> Option<&'static str> {\n");
    out.push_str("    match (action, path) {\n");
    for (action, items) in entries {
        for item in items {
            if let Some(pat) = item.pattern.as_ref() {
                // `{:?}` for the pattern emits a normal Rust string literal
                // with escapes — `\d` in the JSON becomes `"\\d"` here.
                // That round-trips through regex::Regex correctly; using a
                // raw-string prefix would double-escape backslashes.
                out.push_str(&format!(
                    "        ({:?}, {:?}) => Some({:?}),\n",
                    action, item.path, pat
                ));
            }
        }
    }
    out.push_str("        _ => None,\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}
