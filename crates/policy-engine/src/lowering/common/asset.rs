use crate::action::{AmountConstraint, AssetKind, AssetRef};
use crate::context_keys::{ADDRESS, AMOUNT, ASSET, DECIMALS, SYMBOL, TOKEN_ID};
use serde_json::{Map, Value};

use super::amount::amount_constraint_json;

/// Errors raised by lowering helpers when input data is structurally invalid.
#[derive(Debug)]
pub enum LoweringError {
    /// A required field on `AssetRef` was missing during lowering.
    MissingAssetField {
        /// Name of the missing field (e.g. `"address"`, `"symbol"`, `"decimals"`).
        field: &'static str,
    },
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAssetField { field } => {
                write!(f, "missing required asset field: {field}")
            }
        }
    }
}

impl std::error::Error for LoweringError {}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn asset_ref_json(asset: &AssetRef) -> Result<Value, LoweringError> {
    let mut out = Map::new();
    out.insert("kind".into(), Value::from(asset_kind_str(&asset.kind)));
    out.insert(
        ADDRESS.into(),
        Value::from(
            asset
                .address
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ),
    );
    if let Some(token_id) = &asset.token_id {
        out.insert(TOKEN_ID.into(), Value::from(token_id.to_string()));
    }
    out.insert(
        SYMBOL.into(),
        Value::from(asset.symbol.as_deref().unwrap_or_default()),
    );
    out.insert(
        DECIMALS.into(),
        Value::from(i64::from(asset.decimals.unwrap_or_default())),
    );
    Ok(Value::Object(out))
}

pub(crate) fn asset_ref_with_amount_json(
    asset: &AssetRef,
    amount: &AmountConstraint,
) -> Result<Value, LoweringError> {
    let mut out = Map::new();
    out.insert(ASSET.into(), asset_ref_json(asset)?);
    out.insert(AMOUNT.into(), amount_constraint_json(amount));
    Ok(Value::Object(out))
}

pub(crate) const fn asset_kind_str(kind: &AssetKind) -> &'static str {
    match kind {
        AssetKind::Native => "native",
        AssetKind::Erc20 => "erc20",
        AssetKind::Erc721 => "erc721",
        AssetKind::Erc1155 => "erc1155",
        AssetKind::Unknown => "unknown",
    }
}
