//! `ActionEnvelope` and `ActionFields` tagged union.
//!
//! `ActionFields` carries the per-action payload as a `#[serde(tag = "_kind")]`
//! union so JSON serialization produces `{ "_kind": "swap", ... }` that
//! validates against `schema_demo/schema/actions/<kind>.json`.

use serde::{Deserialize, Serialize};

use super::actions::{
    AddLiquidityAction, ApproveAction, RemoveLiquidityAction, SwapAction, UnwrapAction, WrapAction,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Dex,
    Lending,
    Rwa,
    LiquidStaking,
    Restaking,
    Yield,
    Misc,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "_kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ActionFields {
    Swap(SwapAction),
    Wrap(WrapAction),
    Unwrap(UnwrapAction),
    Approve(ApproveAction),
    AddLiquidity(AddLiquidityAction),
    RemoveLiquidity(RemoveLiquidityAction),
}

impl ActionFields {
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::Swap(_) => "swap",
            Self::Wrap(_) => "wrap",
            Self::Unwrap(_) => "unwrap",
            Self::Approve(_) => "approve",
            Self::AddLiquidity(_) => "add_liquidity",
            Self::RemoveLiquidity(_) => "remove_liquidity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub action: String,
    pub category: Category,
    pub fields: ActionFields,
}

impl ActionEnvelope {
    pub fn new(category: Category, fields: ActionFields) -> Self {
        Self {
            action: fields.action_name().to_string(),
            category,
            fields,
        }
    }
}
