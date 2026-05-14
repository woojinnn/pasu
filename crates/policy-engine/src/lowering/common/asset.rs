use crate::action::{AssetKind, AssetRef, AssetRefWithAmountConstraint};
use crate::context_keys::{ADDRESS, AMOUNT, ASSET, DECIMALS, SYMBOL, TOKEN_ID};
use serde_json::{Map, Value};

use super::amount::amount_constraint_json;

pub(crate) fn asset_ref_json(asset: &AssetRef) -> Value {
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
    Value::Object(out)
}

pub(crate) fn asset_ref_with_amount_json(pair: &AssetRefWithAmountConstraint) -> Value {
    let mut out = Map::new();
    out.insert(ASSET.into(), asset_ref_json(&pair.asset));
    out.insert(AMOUNT.into(), amount_constraint_json(&pair.amount));
    Value::Object(out)
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
