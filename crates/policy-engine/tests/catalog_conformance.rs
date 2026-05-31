//! Catalog-conformance gate — every default-bundle manifest's `policy_rpc`
//! calls must conform to the method DEFINITION in `schema/method-catalog.json`.
//!
//! The sibling `default_policies_v2` gate proves cedar↔manifest-schema
//! consistency (the policy compiles against the schema the manifest
//! synthesizes), but it never checks that a manifest's declared
//! method/params/outputs actually match the *catalog* definition of that
//! method. So a manifest could call a real method with the wrong params or
//! project a return field that does not exist, and nothing caught it.
//!
//! This gate closes that hole. For every shipped default bundle, for every
//! `policy_rpc[]` call, it asserts against the catalog (`methods` ∪ `planned`):
//!
//!   1. the `method` is cataloged at all;
//!   2. every manifest param is a *declared* param of that method;
//!   3. every catalog param that is `required` AND has no `defaultSelector`
//!      is present in the manifest (the host can't fill it for free);
//!   4. every manifest output projects from one of the method's declared
//!      return sources, with a matching (case-insensitive) Cedar type.
//!
//! Rule 4 supports BOTH the scalar `returns` form
//! (`{kind, type, from}`) and a multi-field record form
//! (`{kind:"record", fields:{<name>:{type, from}}}`), the latter used by
//! methods that emit several context fields from one call.
//!
//! STRUCTURAL FLAG (do not fix here): the catalog's `methods` section is
//! byte-coupled to the policy-rpc daemon (the `catalog.test.ts` drift test),
//! so a method that is REAL but sim-server-implemented — e.g.
//! `approval.unlimited_over_balance`, implemented in
//! `crates/simulation/server/src/facts.rs`, not the daemon — currently has to
//! live in the JSON-only `planned` section. Under ADR-009 (policy-rpc
//! retiring, sim-server = fact host) the clean home for sim-server methods is
//! an open structural question (a `sim_server` section? move `methods`
//! ownership to sim-server?). Left untouched on purpose.

use std::fs;
use std::path::{Path, PathBuf};

use policy_engine::policy_rpc::ManifestV2;
use serde_json::Value;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn default_policies_dir() -> PathBuf {
    crate_root().join("tests/fixtures/default_policies_v2")
}

fn method_catalog_path() -> PathBuf {
    crate_root().join("../../schema/method-catalog.json")
}

/// The catalog's `methods` ∪ `planned` as a flat name→definition map.
fn load_catalog_defs() -> Vec<(String, Value)> {
    let raw = fs::read_to_string(method_catalog_path())
        .unwrap_or_else(|e| panic!("read method-catalog.json: {e}"));
    let catalog: Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse method-catalog.json: {e}"));

    let mut defs = Vec::new();
    for section in ["methods", "planned"] {
        if let Some(map) = catalog.get(section).and_then(Value::as_object) {
            for (name, def) in map {
                defs.push((name.clone(), def.clone()));
            }
        }
    }
    defs
}

fn find_def<'a>(defs: &'a [(String, Value)], method: &str) -> Option<&'a Value> {
    defs.iter().find(|(n, _)| n == method).map(|(_, d)| d)
}

/// Lowercase + trim a type spelling so manifest projection spellings
/// ("Decimal"/"Bool"/"Set<String>") compare equal to the catalog's Cedar
/// spellings ("decimal"/"Bool"/...).
fn norm_type(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// What a method's `returns` declaration lets outputs project from.
enum ReturnContract {
    /// Concrete `(from, normalized-type)` pairs an output must match exactly.
    /// Covers the scalar form (`returns.from`/`returns.type`) and the inline
    /// record form (`returns.fields.<name>.{from,type}`).
    Sources(Vec<(String, String)>),
    /// A named Cedar record (`{kind:"record", type:"<Name>"}`) with no inline
    /// per-field `from`/`type` selectors — e.g. `oracle.usd_value`'s
    /// `UsdValuation`. The catalog does not express which result paths are
    /// valid or their types, so an output's exact `from`/`type` can't be
    /// machine-checked here; we only require it projects from `$.result.*`.
    OpaqueRecord,
}

/// Read a method's `returns` into a [`ReturnContract`].
fn return_contract(def: &Value, method: &str) -> ReturnContract {
    let returns = def
        .get("returns")
        .unwrap_or_else(|| panic!("method `{method}` has no `returns` in the catalog"));

    if let Some(fields) = returns.get("fields").and_then(Value::as_object) {
        let sources = fields
            .values()
            .filter_map(|f| {
                let from = f.get("from").and_then(Value::as_str)?;
                let ty = f.get("type").and_then(Value::as_str)?;
                Some((from.to_owned(), norm_type(ty)))
            })
            .collect();
        return ReturnContract::Sources(sources);
    }

    match (
        returns.get("from").and_then(Value::as_str),
        returns.get("type").and_then(Value::as_str),
    ) {
        (Some(from), Some(ty)) => ReturnContract::Sources(vec![(from.to_owned(), norm_type(ty))]),
        // A named-record return (record `kind`/`type`, no inline fields, no
        // scalar `from`) is opaque to this gate — see ReturnContract docs.
        _ => ReturnContract::OpaqueRecord,
    }
}

/// Every shipped default v2 bundle's `policy_rpc` calls conform to the method
/// catalog: cataloged method, declared params only, all host-unfillable
/// required params present, and outputs that project a real return source with
/// the right type.
#[test]
fn default_v2_policy_rpc_conforms_to_method_catalog() {
    let defs = load_catalog_defs();
    let dir = default_policies_dir();
    let mut calls_checked = 0;

    for entry in fs::read_dir(&dir).expect("read default_policies_v2 fixture dir") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let bundle = entry.path();
        let id = bundle
            .file_name()
            .expect("bundle dir name")
            .to_string_lossy()
            .into_owned();

        let manifest_json = fs::read_to_string(bundle.join("manifest.json"))
            .unwrap_or_else(|e| panic!("read {id}/manifest.json: {e}"));
        let manifest: ManifestV2 = serde_json::from_str(&manifest_json)
            .unwrap_or_else(|e| panic!("parse {id}/manifest.json: {e}"));

        for call in &manifest.policy_rpc {
            let method = call.method.as_str();

            // Rule 1 — the method is cataloged at all.
            let def = find_def(&defs, method).unwrap_or_else(|| {
                panic!(
                    "{id}/policy_rpc[{}]: method `{method}` is not in the method catalog \
                     (neither `methods` nor `planned`)",
                    call.id
                )
            });

            let catalog_params = def
                .get("params")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("catalog method `{method}` has no `params` object"));

            // Rule 2 — every manifest param is a declared param of the method.
            for key in call.params.keys() {
                assert!(
                    catalog_params.contains_key(key),
                    "{id}/policy_rpc[{}]: param `{key}` is not a declared param of `{method}` \
                     (declared: {:?})",
                    call.id,
                    catalog_params.keys().collect::<Vec<_>>()
                );
            }

            // Rule 3 — every required param the host can't default is present.
            for (pname, pdef) in catalog_params {
                let required = pdef.get("required").and_then(Value::as_bool) == Some(true);
                let has_default = pdef.get("defaultSelector").is_some();
                if required && !has_default {
                    assert!(
                        call.params.contains_key(pname),
                        "{id}/policy_rpc[{}]: required param `{pname}` of `{method}` is missing \
                         (no defaultSelector to fill it for free)",
                        call.id
                    );
                }
            }

            // Rule 4 — every output projects a declared return source + type.
            match return_contract(def, method) {
                ReturnContract::Sources(sources) => {
                    for out in &call.outputs {
                        let out_type = norm_type(out.type_name.cedar_type());
                        let matched = sources
                            .iter()
                            .any(|(from, ty)| *from == out.from && *ty == out_type);
                        assert!(
                            matched,
                            "{id}/policy_rpc[{}]: output `{}` projects from `{}` as `{}`, which is \
                             not a declared return source of `{method}` (allowed: {sources:?})",
                            call.id, out.field, out.from, out_type
                        );
                    }
                }
                ReturnContract::OpaqueRecord => {
                    for out in &call.outputs {
                        assert!(
                            out.from.starts_with("$.result"),
                            "{id}/policy_rpc[{}]: output `{}` of named-record method `{method}` \
                             must project from `$.result.*` (got `{}`)",
                            call.id,
                            out.field,
                            out.from
                        );
                    }
                }
            }

            calls_checked += 1;
        }
    }

    assert!(
        calls_checked >= 1,
        "expected at least one default v2 policy_rpc call to check in {dir:?}"
    );
}
