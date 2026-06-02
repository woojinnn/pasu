//! Walks the composed Cedar schema (resolved JSON form, via
//! `schema_str_to_json_with_resolved_types`) into a typed field catalog
//! `{ actionId(Pascal) -> [FieldDto] }` for `context.*` / `principal.*` /
//! `resource.*`. Display metadata only — non-authoritative (the dashboard's
//! `blocksToEst` ignores it). Covers the complete Cedar type system; see the
//! spec "Type resolution".
//!
//! Shape notes (confirmed by spike against cedar-policy 4.10):
//! - Top-level keys are namespaces (`""` for the empty namespace).
//! - `actions.<Name>.appliesTo.context` is a `Record`; `principalTypes` /
//!   `resourceTypes` are arrays of namespaced entity names.
//! - Boolean is spelled `Bool`; optional attrs carry `"required": false`.
//! - Common types are NOT inlined: they appear as a bare namespaced reference
//!   `{"type": "Ns::Name"}` and must be resolved against `Ns.commonTypes`
//!   (or `Ns.entityTypes` for entity references).

use std::collections::BTreeMap;

use cedar_policy::schema_str_to_json_with_resolved_types;
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDto {
    pub path: String,
    #[serde(rename = "type")]
    pub cedar_type: String,
    /// primitive | collection | record | entity | extension | unknown
    pub field_kind: String,
    /// base | custom
    pub source: String,
}

const MAX_DEPTH: usize = 64;
const EXTENSION_NAMES: [&str; 4] = ["decimal", "ipaddr", "datetime", "duration"];

/// Build the catalog from cedarschema text. Pure; testable without install state.
pub fn build(schema_text: &str) -> Result<BTreeMap<String, Vec<FieldDto>>, String> {
    let (json, _warnings) = schema_str_to_json_with_resolved_types(schema_text)
        .map_err(|e| format!("schema parse: {e}"))?;
    let top = json.as_object().ok_or("schema json is not an object")?;

    let mut out: BTreeMap<String, Vec<FieldDto>> = BTreeMap::new();
    for ns in top.values() {
        let Some(actions) = ns.get("actions").and_then(Value::as_object) else {
            continue;
        };
        for (action_name, adef) in actions {
            let mut fields: Vec<FieldDto> = Vec::new();
            if let Some(applies) = adef.get("appliesTo") {
                if let Some(ctx) = applies.get("context") {
                    walk(ctx, "context", 0, top, &mut fields);
                }
                for (role, key) in [
                    ("principal", "principalTypes"),
                    ("resource", "resourceTypes"),
                ] {
                    if let Some(types) = applies.get(key).and_then(Value::as_array) {
                        for t in types.iter().filter_map(Value::as_str) {
                            if let Some(shape) = resolve_entity_shape(t, top) {
                                walk(shape, role, 0, top, &mut fields);
                            }
                        }
                    }
                }
            }
            out.entry(action_name.clone()).or_default().extend(fields);
        }
    }
    Ok(out)
}

fn source_of(path: &str) -> String {
    if path.contains(".custom.") || path.ends_with(".custom") {
        "custom".to_string()
    } else {
        "base".to_string()
    }
}

fn leaf(path: &str, cedar_type: impl Into<String>, kind: &str) -> FieldDto {
    FieldDto {
        path: path.to_string(),
        cedar_type: cedar_type.into(),
        field_kind: kind.to_string(),
        source: source_of(path),
    }
}

/// Split a (possibly namespaced) type name into (namespace, bare name).
/// `"Demo::Meta"` -> ("Demo","Meta"); `"Wallet"` -> ("","Wallet").
fn split_ns(name: &str) -> (String, String) {
    match name.rsplit_once("::") {
        Some((ns, n)) => (ns.to_string(), n.to_string()),
        None => (String::new(), name.to_string()),
    }
}

/// Resolve a namespaced entity name to its `shape` node, if any.
fn resolve_entity_shape<'a>(qualified: &str, top: &'a Map<String, Value>) -> Option<&'a Value> {
    let (ns, name) = split_ns(qualified);
    top.get(&ns)?.get("entityTypes")?.get(&name)?.get("shape")
}

/// Recursively emit leaves for a resolved type node at `path`.
fn walk(node: &Value, path: &str, depth: usize, top: &Map<String, Value>, out: &mut Vec<FieldDto>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(t) = node.get("type").and_then(Value::as_str) else {
        out.push(leaf(path, "", "unknown"));
        return;
    };
    match t {
        "Bool" | "Boolean" => out.push(leaf(path, "Boolean", "primitive")),
        "Long" => out.push(leaf(path, "Long", "primitive")),
        "String" => out.push(leaf(path, "String", "primitive")),
        "Extension" => {
            let n = node
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("extension");
            out.push(leaf(path, n, "extension"));
        }
        "Entity" => {
            let n = node.get("name").and_then(Value::as_str).unwrap_or("Entity");
            out.push(leaf(path, format!("Entity<{n}>"), "entity"));
        }
        "Set" => {
            let spelling = node
                .get("element")
                .map(type_spelling)
                .unwrap_or_else(|| "Set".to_string());
            out.push(leaf(path, format!("Set<{spelling}>"), "collection"));
        }
        "Record" => {
            out.push(leaf(path, "Record", "record"));
            if let Some(attrs) = node.get("attributes").and_then(Value::as_object) {
                for (k, v) in attrs {
                    walk(v, &format!("{path}.{k}"), depth + 1, top, out);
                }
            }
        }
        "EntityOrCommon" => {
            let n = node.get("name").and_then(Value::as_str).unwrap_or("");
            resolve_ref(n, path, depth, top, out);
        }
        // Bare (possibly namespaced) reference to a common type / entity /
        // extension, e.g. "Demo::Meta" or "decimal".
        other => resolve_ref(other, path, depth, top, out),
    }
}

/// Resolve a (possibly namespaced) type reference: extension name -> extension
/// leaf; common type -> recurse; entity -> entity leaf; otherwise unknown leaf.
fn resolve_ref(
    qualified: &str,
    path: &str,
    depth: usize,
    top: &Map<String, Value>,
    out: &mut Vec<FieldDto>,
) {
    let (ns, name) = split_ns(qualified);
    if EXTENSION_NAMES.contains(&name.as_str()) {
        out.push(leaf(path, name, "extension"));
        return;
    }
    if let Some(ct) = top
        .get(&ns)
        .and_then(|n| n.get("commonTypes"))
        .and_then(|c| c.get(&name))
    {
        walk(ct, path, depth + 1, top, out);
        return;
    }
    if top
        .get(&ns)
        .and_then(|n| n.get("entityTypes"))
        .and_then(|e| e.get(&name))
        .is_some()
    {
        out.push(leaf(path, format!("Entity<{qualified}>"), "entity"));
        return;
    }
    out.push(leaf(path, qualified, "unknown"));
}

/// Short Cedar-ish spelling for a (possibly nested) type node, for Set elements.
fn type_spelling(node: &Value) -> String {
    match node.get("type").and_then(Value::as_str) {
        Some("Bool") | Some("Boolean") => "Boolean".to_string(),
        Some("Long") => "Long".to_string(),
        Some("String") => "String".to_string(),
        Some("Set") => format!(
            "Set<{}>",
            node.get("element")
                .map(type_spelling)
                .unwrap_or_else(|| "_".to_string())
        ),
        Some("Entity") => format!(
            "Entity<{}>",
            node.get("name").and_then(Value::as_str).unwrap_or("_")
        ),
        Some("Extension") => node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("ext")
            .to_string(),
        Some("Record") => "Record".to_string(),
        Some(other) => other.to_string(),
        None => "_".to_string(),
    }
}
