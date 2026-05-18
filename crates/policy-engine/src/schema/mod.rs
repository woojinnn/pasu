//! Cedar policy schema composition.

pub mod action_name;
pub mod aliases;
pub mod composer;
pub mod enriched;
pub mod fragment;
pub mod manifest_fragment;

pub use composer::compose_enriched;
pub use enriched::EnrichedSchema;
pub use fragment::{CedarTypeFragment, CustomFieldSource};
pub use manifest_fragment::manifest_to_cedarschema;

use crate::policy_rpc::{validate_manifests, PolicyManifest, PolicyRpcError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CORE_SCHEMA: &str = include_str!("../../../../schema/policy-schema/core.cedarschema");
const DEX_ADD_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/add_liquidity.cedarschema");
const DEX_BURN_LIQUIDITY_NFT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/burn_liquidity_nft.cedarschema");
const DEX_DECREASE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/decrease_liquidity.cedarschema");
const DEX_DONATE_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/donate.cedarschema");
const DEX_INCREASE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/increase_liquidity.cedarschema");
const DEX_INITIALIZE_POOL_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/initialize_pool.cedarschema");
const DEX_MINT_LIQUIDITY_NFT_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/mint_liquidity_nft.cedarschema");
const DEX_REMOVE_LIQUIDITY_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/remove_liquidity.cedarschema");
const DEX_SWAP_SCHEMA: &str =
    include_str!("../../../../schema/policy-schema/actions/DEX/swap.cedarschema");

/// Composes the shipped core and action Cedar schemas.
#[derive(Debug, Default, Clone)]
pub struct PolicySchemaComposer {
    manifests: Vec<PolicyManifest>,
}

/// Preview of a composed policy schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchemaPreview {
    /// Final Cedar schema text.
    pub schema_text: String,
    /// SHA-256 hash of `schema_text`.
    pub schema_hash: String,
    /// Fields contributed by manifests that were not already present.
    pub added_fields: Vec<AddedContextField>,
}

/// Manifest-added context field metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddedContextField {
    /// Action kind.
    pub action: String,
    /// Context field name.
    pub field: String,
    /// Cedar field type.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Manifest id that contributed the field.
    pub source_manifest: String,
}

impl PolicySchemaComposer {
    /// Construct a schema composer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manifests: Vec::new(),
        }
    }

    /// Return a composer with manifest-driven context extensions.
    ///
    /// # Errors
    ///
    /// Returns an error when manifest schema extensions are invalid or
    /// conflict with the base schema.
    pub fn with_manifests(mut self, manifests: &[PolicyManifest]) -> Result<Self, PolicyRpcError> {
        validate_manifests(manifests)?;
        self.manifests = manifests.to_vec();
        self.try_preview()?;
        Ok(self)
    }

    /// Return the concatenated Cedar schema text.
    #[must_use]
    pub fn compose(&self) -> String {
        self.preview().schema_text
    }

    /// Return the schema preview.
    #[must_use]
    pub fn preview(&self) -> SchemaPreview {
        match self.try_preview() {
            Ok(preview) => preview,
            Err(error) => {
                debug_assert!(
                    false,
                    "PolicySchemaComposer contains invalid manifests: {error}"
                );
                let schema_text = base_schema_text();
                SchemaPreview {
                    schema_hash: schema_hash(&schema_text),
                    schema_text,
                    added_fields: Vec::new(),
                }
            }
        }
    }

    /// Return the schema preview.
    ///
    /// # Errors
    ///
    /// Returns an error when manifest schema extensions are invalid or
    /// conflict with the base schema.
    pub fn try_preview(&self) -> Result<SchemaPreview, PolicyRpcError> {
        let schema_text = compose_schema_text(&self.manifests)?;
        let schema_hash = schema_hash(&schema_text);
        let added_fields = added_fields(BASE_SCHEMA_TEXT, &self.manifests)?;
        Ok(SchemaPreview {
            schema_text,
            schema_hash,
            added_fields,
        })
    }
}

/// Return the SHA-256 hash string for a Cedar schema text.
#[must_use]
pub fn schema_hash(schema_text: &str) -> String {
    let digest = Sha256::digest(schema_text.as_bytes());
    format!("sha256:{digest:x}")
}

const BASE_SCHEMA_TEXT: &str = "";

pub(crate) fn base_schema_text() -> String {
    [
        CORE_SCHEMA,
        DEX_ADD_LIQUIDITY_SCHEMA,
        DEX_BURN_LIQUIDITY_NFT_SCHEMA,
        DEX_DECREASE_LIQUIDITY_SCHEMA,
        DEX_DONATE_SCHEMA,
        DEX_INCREASE_LIQUIDITY_SCHEMA,
        DEX_INITIALIZE_POOL_SCHEMA,
        DEX_MINT_LIQUIDITY_NFT_SCHEMA,
        DEX_REMOVE_LIQUIDITY_SCHEMA,
        DEX_SWAP_SCHEMA,
    ]
    .join("\n")
}

fn compose_schema_text(manifests: &[PolicyManifest]) -> Result<String, PolicyRpcError> {
    let mut schema = base_schema_text();
    for field in added_fields(&schema, manifests)? {
        insert_optional_context_field(&mut schema, &field.action, &field.field, &field.type_name)?;
    }
    Ok(schema)
}

fn added_fields(
    schema_text: &str,
    manifests: &[PolicyManifest],
) -> Result<Vec<AddedContextField>, PolicyRpcError> {
    let base = if schema_text.is_empty() {
        base_schema_text()
    } else {
        schema_text.to_owned()
    };
    let base_declared = collect_context_fields(&base)?;
    let mut declared = BTreeMap::new();
    let mut added = Vec::new();

    for manifest in manifests {
        for (action, fields) in &manifest.context_extensions {
            validate_action(action)?;
            for (field, type_name) in fields {
                validate_field_name(field)?;
                let canonical_type = canonical_type(type_name)?;
                let key = (action.clone(), field.clone());
                if let Some(base_type) = base_declared.get(&key) {
                    if base_type != canonical_type {
                        return Err(PolicyRpcError::Schema(format!(
                            "context extension {action}.{field} has type {canonical_type}, but base schema declares {base_type}"
                        )));
                    }
                    continue;
                }
                if let Some(existing) = declared.get(&key) {
                    if existing != canonical_type {
                        return Err(PolicyRpcError::Schema(format!(
                            "context field {action}.{field} already has type {existing}, not {canonical_type}"
                        )));
                    }
                    continue;
                }
                declared.insert(key, canonical_type.to_owned());
                added.push(AddedContextField {
                    action: action.clone(),
                    field: field.clone(),
                    type_name: canonical_type.to_owned(),
                    source_manifest: manifest.id.clone(),
                });
            }
        }
    }

    Ok(added)
}

fn collect_context_fields(
    schema_text: &str,
) -> Result<BTreeMap<(String, String), String>, PolicyRpcError> {
    let mut fields = BTreeMap::new();
    for (action, type_name) in ACTION_CONTEXT_TYPES {
        let block = type_block(schema_text, type_name)?;
        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("type ") || trimmed == "};" {
                continue;
            }
            let Some((name, field_type)) = parse_field_line(trimmed) else {
                continue;
            };
            fields.insert(
                ((*action).to_owned(), name.to_owned()),
                field_type.to_owned(),
            );
        }
    }
    Ok(fields)
}

fn parse_field_line(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_suffix(',').unwrap_or(line);
    let (name, field_type) = line.split_once(':')?;
    Some((name.trim().trim_end_matches('?'), field_type.trim()))
}

fn insert_optional_context_field(
    schema: &mut String,
    action: &str,
    field: &str,
    type_name: &str,
) -> Result<(), PolicyRpcError> {
    let context_type = context_type_for_action(action)?;
    let start = schema
        .find(&format!("type {context_type} = {{"))
        .ok_or_else(|| PolicyRpcError::Schema(format!("missing context type `{context_type}`")))?;
    let relative_end = schema[start..].find("};").ok_or_else(|| {
        PolicyRpcError::Schema(format!("unterminated context type `{context_type}`"))
    })?;
    let insert_at = start + relative_end;
    schema.insert_str(insert_at, &format!("  {field}?: {type_name},\n"));
    Ok(())
}

fn type_block<'a>(schema_text: &'a str, type_name: &str) -> Result<&'a str, PolicyRpcError> {
    let start = schema_text
        .find(&format!("type {type_name} = {{"))
        .ok_or_else(|| PolicyRpcError::Schema(format!("missing context type `{type_name}`")))?;
    let relative_end = schema_text[start..].find("};").ok_or_else(|| {
        PolicyRpcError::Schema(format!("unterminated context type `{type_name}`"))
    })?;
    Ok(&schema_text[start..start + relative_end + 2])
}

fn context_type_for_action(action: &str) -> Result<&'static str, PolicyRpcError> {
    ACTION_CONTEXT_TYPES
        .iter()
        .find_map(|(candidate, type_name)| (*candidate == action).then_some(*type_name))
        .ok_or_else(|| {
            PolicyRpcError::Schema(format!("unknown context extension action `{action}`"))
        })
}

fn validate_action(action: &str) -> Result<(), PolicyRpcError> {
    context_type_for_action(action).map(|_| ())
}

fn validate_field_name(field: &str) -> Result<(), PolicyRpcError> {
    let mut chars = field.chars();
    let Some(first) = chars.next() else {
        return Err(PolicyRpcError::Schema(
            "field name must not be empty".to_owned(),
        ));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(PolicyRpcError::Schema(format!(
            "invalid context field name `{field}`"
        )));
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return Err(PolicyRpcError::Schema(format!(
            "invalid context field name `{field}`"
        )));
    }
    Ok(())
}

fn canonical_type(type_name: &str) -> Result<&'static str, PolicyRpcError> {
    match type_name {
        "String" => Ok("String"),
        "Long" => Ok("Long"),
        "Bool" => Ok("Bool"),
        "decimal" | "Decimal" => Ok("decimal"),
        "UsdValuation" => Ok("UsdValuation"),
        "WindowStats" => Ok("WindowStats"),
        "Set<String>" => Ok("Set<String>"),
        other => Err(PolicyRpcError::Schema(format!(
            "unsupported context field type `{other}`"
        ))),
    }
}

const ACTION_CONTEXT_TYPES: &[(&str, &str)] = &[
    ("add_liquidity", "AddLiquidityContext"),
    ("burn_liquidity_nft", "BurnLiquidityNftContext"),
    ("decrease_liquidity", "DecreaseLiquidityContext"),
    ("donate", "DonateContext"),
    ("increase_liquidity", "IncreaseLiquidityContext"),
    ("initialize_pool", "InitializePoolContext"),
    ("mint_liquidity_nft", "MintLiquidityNftContext"),
    ("remove_liquidity", "RemoveLiquidityContext"),
    ("swap", "SwapContext"),
];
