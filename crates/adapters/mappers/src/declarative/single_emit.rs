//! `single_emit` strategy execution (spec §5.2.1).
//!
//! Phase 1A only supported `category="dex" / action="swap"`. Phase 5 added the
//! handful of Universal Router opcode mappings that emit non-swap envelopes
//! (`misc/wrap`, `misc/unwrap`, `misc/transfer`, `misc/permit`) so the
//! `opcode_stream_dispatch` per-opcode rules can reuse this builder.
//!
//! The interpreter:
//!
//!  1. Builds the JSON view of `decoded.args` ([`super::eval::args_to_json`]).
//!  2. Evaluates every `(field_path, ValueExpr)` entry into a JSON value.
//!  3. Materialises a nested `serde_json::Value` tree by splitting each
//!     `field_path` on `.` (so `inputToken.asset.address` becomes
//!     `{ inputToken: { asset: { address: <value> } } }`).
//!  4. Maps that tree into the requested action variant (`SwapAction`,
//!     `WrapAction`, `UnwrapAction`, `TransferAction`, `PermitAction`).
//!
//! The intermediate JSON tree is deliberately ignorant of policy-engine types,
//! and conversion happens only at the action boundary. This keeps the
//! interpreter generic for when category/action expand further.
//!
//! `fee_bps` is intentionally `None` — declarative bundles in the PoC do not
//! emit it. The V2 equivalence test asserts this gap explicitly (the static V2
//! mapper returns `Some(30)` while declarative returns `None`).

use std::collections::BTreeMap;
use std::str::FromStr as _;

use abi_resolver::DecodedCall;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::misc::{
    PermitAction, PermitKind, TransferAction, UnwrapAction, WrapAction,
};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
    AssetRefWithAmountConstraint, Category, DecimalString, Validity, ValiditySource,
};

use crate::mapper::{MapContext, MapperError};

use super::eval::{args_to_json, evaluate};
use super::types::{EmitRule, ValueExpr};

/// Execute a `single_emit` rule against the given decoded call.
///
/// Supported combinations (PoC):
///   * `("dex", "swap")` — Phase 1A
///   * `("misc", "wrap")` / `("misc", "unwrap")` — Phase 5 (UR WRAP_ETH /
///     UNWRAP_WETH opcodes)
///   * `("misc", "transfer")` — Phase 5 (UR SWEEP opcode)
///   * `("misc", "permit")` — Phase 5 (UR PERMIT2_PERMIT opcode)
///
/// Any other combination yields [`MapperError::Internal`].
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    rule: &EmitRule,
) -> Result<ActionEnvelope, MapperError> {
    let (category, action, fields) = match rule {
        EmitRule::SingleEmit {
            category,
            action,
            fields,
        } => (category.as_str(), action.as_str(), fields),
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "single_emit::execute called with non-single_emit rule: {other:?}"
            )))
        }
    };

    let args_json = args_to_json(decoded);
    let tree = build_field_tree(ctx, &args_json, fields)?;

    match (category, action) {
        ("dex", "swap") => Ok(build_swap_envelope(&tree)?),
        ("misc", "wrap") => Ok(build_wrap_envelope(&tree)?),
        ("misc", "unwrap") => Ok(build_unwrap_envelope(&tree)?),
        ("misc", "transfer") => Ok(build_transfer_envelope(&tree)?),
        ("misc", "permit") => Ok(build_permit_envelope(&tree)?),
        (c, a) => Err(MapperError::Internal(anyhow::anyhow!(
            "single_emit category/action {c:?}/{a:?} not implemented in PoC"
        ))),
    }
}

/// Evaluate each `ValueExpr`, then merge the dot-paths into a nested JSON tree.
fn build_field_tree(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    fields: &BTreeMap<String, ValueExpr>,
) -> Result<serde_json::Value, MapperError> {
    let mut root = serde_json::Value::Object(serde_json::Map::new());
    for (path, expr) in fields {
        let value = evaluate(ctx, args_json, expr)?;
        set_nested(&mut root, path, value)?;
    }
    Ok(root)
}

/// `set_nested(root, "a.b.c", v)` mutates `root` so `root.a.b.c == v`.
///
/// Each path segment must be a non-empty bareword (no array indexing). The
/// function refuses to overwrite a non-object intermediate (which would
/// indicate two fields disagreeing about the type of a parent).
fn set_nested(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), MapperError> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "field path {path:?} contains empty segment"
        )));
    }
    if segments.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "field path is empty"
        )));
    }

    let mut cursor = root;
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index == segments.len() - 1;
        let map = cursor.as_object_mut().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "field path {path:?}: ancestor at segment {} is not an object",
                index
            ))
        })?;
        if is_last {
            map.insert((*segment).to_owned(), value);
            return Ok(());
        }
        cursor = map
            .entry((*segment).to_owned())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    unreachable!("loop returns on the last segment");
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → SwapAction
// ───────────────────────────────────────────────────────────────────────────

fn build_swap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let input_token = read_asset_with_amount(tree, "inputToken")?;
    let output_token = read_asset_with_amount(tree, "outputToken")?;
    let recipient = read_address(tree, "recipient")?;
    let validity = read_validity(tree)?;
    let swap_mode = derive_swap_mode(&input_token.amount.kind, &output_token.amount.kind);

    let action = SwapAction {
        swap_mode,
        input_token,
        output_token,
        recipient,
        validity,
        fee_bps: None,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    })
}

fn derive_swap_mode(input: &AmountKind, output: &AmountKind) -> SwapMode {
    match (input, output) {
        (AmountKind::Exact, AmountKind::Min) => SwapMode::ExactIn,
        (AmountKind::Max, AmountKind::Exact) => SwapMode::ExactOut,
        _ => SwapMode::Unknown,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → WrapAction / UnwrapAction (Phase 5 — UR WRAP_ETH / UNWRAP_WETH)
// ───────────────────────────────────────────────────────────────────────────

fn build_wrap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let native_asset = read_asset_with_amount(tree, "nativeAsset")?;
    let wrapped_asset = read_asset_with_amount(tree, "wrappedAsset")?;
    let recipient = read_address(tree, "recipient")?;
    let action = WrapAction {
        native_asset,
        wrapped_asset,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Wrap(action),
    })
}

fn build_unwrap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let wrapped_asset = read_asset_with_amount(tree, "wrappedAsset")?;
    let native_asset = read_asset_with_amount(tree, "nativeAsset")?;
    let recipient = read_address(tree, "recipient")?;
    let action = UnwrapAction {
        wrapped_asset,
        native_asset,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Unwrap(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → TransferAction (Phase 5 — UR SWEEP)
// ───────────────────────────────────────────────────────────────────────────

fn build_transfer_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let token = read_asset_with_amount(tree, "token")?;
    let from = read_address(tree, "from")?;
    let recipient = read_address(tree, "recipient")?;
    let action = TransferAction {
        token,
        from,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Transfer(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → PermitAction (Phase 5 — UR PERMIT2_PERMIT)
// ───────────────────────────────────────────────────────────────────────────

fn build_permit_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let permit_kind_str = required_string(tree, "permitKind")
        .map_err(|_| missing_field("$", "permitKind"))?;
    let permit_kind = parse_permit_kind(permit_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "permitKind {permit_kind_str:?} not recognised"
        ))
    })?;

    let token = read_asset_inline(tree, "token")?;
    let owner = read_address(tree, "owner")?;
    let spender = match tree.get("spender") {
        Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("spender {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "spender: expected string, got {other}"
            )));
        }
    };
    let amount = read_amount_inline(tree, "amount")?;
    let validity = read_validity(tree)?
        .ok_or_else(|| MapperError::MissingArgument("validity".to_owned()))?;
    let signature_validity = read_signature_validity(tree)?;

    let action = PermitAction {
        permit_kind,
        token,
        owner,
        spender,
        recipient: None,
        amount,
        requested_amount: None,
        operator: None,
        approved: None,
        validity,
        signature_validity,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Permit(action),
    })
}

fn parse_permit_kind(kind: &str) -> Option<PermitKind> {
    match kind {
        "eip2612" => Some(PermitKind::Eip2612),
        "erc721_permit" => Some(PermitKind::Erc721Permit),
        "erc721_permit_for_all" => Some(PermitKind::Erc721PermitForAll),
        "permit2_single" => Some(PermitKind::Permit2Single),
        "permit2_transfer" => Some(PermitKind::Permit2Transfer),
        _ => None,
    }
}

fn read_signature_validity(tree: &serde_json::Value) -> Result<Option<Validity>, MapperError> {
    let Some(validity) = tree.get("signatureValidity") else {
        return Ok(None);
    };
    if validity.is_null() {
        return Ok(None);
    }
    let expires_at_str = required_string(validity, "expiresAt")
        .map_err(|_| missing_field("signatureValidity", "expiresAt"))?;
    let expires_at = DecimalString::from_str(expires_at_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "signatureValidity.expiresAt {expires_at_str:?}: {message}"
        ))
    })?;
    let source_str = required_string(validity, "source")
        .map_err(|_| missing_field("signatureValidity", "source"))?;
    let source = parse_validity_source(source_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "signatureValidity.source {source_str:?} not recognised"
        ))
    })?;
    Ok(Some(Validity { expires_at, source }))
}

fn read_asset_with_amount(
    tree: &serde_json::Value,
    field: &str,
) -> Result<AssetRefWithAmountConstraint, MapperError> {
    let token = required_object(tree, field)?;
    let asset = read_asset(token, field)?;
    let amount = read_amount(token, field)?;
    Ok(AssetRefWithAmountConstraint { asset, amount })
}

/// Read an `AssetRef` directly nested under `tree.<field>` (no intermediate
/// `asset` wrapper). Used by `PermitAction.token`, which is a bare `AssetRef`.
fn read_asset_inline(tree: &serde_json::Value, field: &str) -> Result<AssetRef, MapperError> {
    let inner = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    let object = inner.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {inner}"))
    })?;
    let kind_str = object
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_field(field, "kind"))?;
    let kind = parse_asset_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{field}.kind {kind_str:?} not recognised"
        ))
    })?;
    let address = match object.get("address") {
        Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.address {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.address: expected string, got {other}"
            )));
        }
    };
    Ok(AssetRef {
        kind,
        address,
        token_id: None,
        symbol: None,
        decimals: None,
    })
}

fn read_asset(token: &serde_json::Value, parent: &str) -> Result<AssetRef, MapperError> {
    let asset = required_object(token, "asset").map_err(|_| missing_field(parent, "asset"))?;
    let kind_str = required_string(asset, "kind").map_err(|_| missing_field(parent, "asset.kind"))?;
    let kind = parse_asset_kind(kind_str)
        .ok_or_else(|| MapperError::Internal(anyhow::anyhow!(
            "{parent}.asset.kind {kind_str:?} not recognised in Phase 1A"
        )))?;
    let address = match asset.get("address") {
        Some(serde_json::Value::String(s)) => Some(
            Address::from_str(s).map_err(|message| MapperError::Internal(anyhow::anyhow!(
                "{parent}.asset.address {s:?}: {message}"
            )))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{parent}.asset.address: expected string, got {other}"
            )));
        }
    };
    Ok(AssetRef {
        kind,
        address,
        token_id: None,
        symbol: None,
        decimals: None,
    })
}

fn parse_asset_kind(kind: &str) -> Option<AssetKind> {
    match kind {
        "erc20" => Some(AssetKind::Erc20),
        "erc721" => Some(AssetKind::Erc721),
        "erc1155" => Some(AssetKind::Erc1155),
        "native" => Some(AssetKind::Native),
        "unknown" => Some(AssetKind::Unknown),
        _ => None,
    }
}

/// Read an `AmountConstraint` directly nested under `tree.<field>` (no
/// intermediate `amount` wrapper). Returns `None` when the field is missing or
/// JSON null. Used by `PermitAction.amount`, which is `Option<AmountConstraint>`.
fn read_amount_inline(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<AmountConstraint>, MapperError> {
    let Some(inner) = tree.get(field) else {
        return Ok(None);
    };
    if inner.is_null() {
        return Ok(None);
    }
    let object = inner.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {inner}"))
    })?;
    let kind_str = object
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_field(field, "kind"))?;
    let kind = parse_amount_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{field}.kind {kind_str:?} not recognised"
        ))
    })?;
    let value = match object.get("value") {
        Some(serde_json::Value::String(s)) => Some(DecimalString::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.value {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.value: expected decimal string, got {other}"
            )));
        }
    };
    Ok(Some(AmountConstraint { kind, value }))
}

fn read_amount(
    token: &serde_json::Value,
    parent: &str,
) -> Result<AmountConstraint, MapperError> {
    let amount =
        required_object(token, "amount").map_err(|_| missing_field(parent, "amount"))?;
    let kind_str = required_string(amount, "kind")
        .map_err(|_| missing_field(parent, "amount.kind"))?;
    let kind = parse_amount_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{parent}.amount.kind {kind_str:?} not recognised in Phase 1A"
        ))
    })?;
    let value = match amount.get("value") {
        Some(serde_json::Value::String(s)) => Some(
            DecimalString::from_str(s).map_err(|message| MapperError::Internal(anyhow::anyhow!(
                "{parent}.amount.value {s:?}: {message}"
            )))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{parent}.amount.value: expected decimal string, got {other}"
            )));
        }
    };
    Ok(AmountConstraint { kind, value })
}

fn parse_amount_kind(kind: &str) -> Option<AmountKind> {
    match kind {
        "exact" => Some(AmountKind::Exact),
        "min" => Some(AmountKind::Min),
        "max" => Some(AmountKind::Max),
        "unlimited" => Some(AmountKind::Unlimited),
        "estimated" => Some(AmountKind::Estimated),
        "unknown" => Some(AmountKind::Unknown),
        "portion" => Some(AmountKind::Portion),
        _ => None,
    }
}

fn read_address(tree: &serde_json::Value, field: &str) -> Result<Address, MapperError> {
    let raw = required_string(tree, field).map_err(|_| missing_field("$", field))?;
    Address::from_str(raw).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!("{field} {raw:?}: {message}"))
    })
}

fn read_validity(tree: &serde_json::Value) -> Result<Option<Validity>, MapperError> {
    let Some(validity) = tree.get("validity") else {
        return Ok(None);
    };
    let object = validity.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "validity must be an object, got {validity}"
        ))
    })?;
    let expires_at_str =
        required_string(validity, "expiresAt").map_err(|_| missing_field("validity", "expiresAt"))?;
    let expires_at = DecimalString::from_str(expires_at_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "validity.expiresAt {expires_at_str:?}: {message}"
        ))
    })?;
    let source_str =
        required_string(validity, "source").map_err(|_| missing_field("validity", "source"))?;
    let source = parse_validity_source(source_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "validity.source {source_str:?} not recognised in Phase 1A"
        ))
    })?;
    let _ = object; // suppress unused warning if validators expand
    Ok(Some(Validity { expires_at, source }))
}

fn parse_validity_source(source: &str) -> Option<ValiditySource> {
    match source {
        "tx-deadline" => Some(ValiditySource::TxDeadline),
        "signature-deadline" => Some(ValiditySource::SignatureDeadline),
        "grant-expiration" => Some(ValiditySource::GrantExpiration),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON helpers
// ───────────────────────────────────────────────────────────────────────────

fn required_object<'a>(
    tree: &'a serde_json::Value,
    field: &str,
) -> Result<&'a serde_json::Value, MapperError> {
    let value = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    if !value.is_object() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected object, got {value}"
        )));
    }
    Ok(value)
}

fn required_string<'a>(
    tree: &'a serde_json::Value,
    field: &str,
) -> Result<&'a str, MapperError> {
    tree.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))
}

fn missing_field(parent: &str, field: &str) -> MapperError {
    MapperError::MissingArgument(format!("{parent}.{field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_nested_builds_dot_path_tree() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "a.b.c", json!(1)).unwrap();
        set_nested(&mut root, "a.b.d", json!(2)).unwrap();
        set_nested(&mut root, "a.e", json!(3)).unwrap();
        assert_eq!(
            root,
            json!({
                "a": {
                    "b": { "c": 1, "d": 2 },
                    "e": 3
                }
            })
        );
    }

    #[test]
    fn set_nested_rejects_empty_segment() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "a..c", json!(1)).unwrap_err();
        assert!(err.to_string().contains("empty segment"));
    }
}
