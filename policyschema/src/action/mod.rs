//! Action 레이어 — `Action`이 정책의 기본 단위. 각 `Action`은 다음을 갖는다:
//!  - `category` (cross-cutting 분류: Swap | Lending | LiquidStaking | Aggregation | Sign | Unknown)
//!  - `type` (구체적인 atomic 종류: Swap | Supply | Stake | SignPermit2Approve | …)
//!  - `fields` (타입별 페이로드 — `SwapFields` | `LendingFields` | `StakingFields` | `SignFields`)
//!  - `extensionIds` (`Extension[]` 안의 프로토콜 특수 데이터 참조)

pub mod category;
pub mod fields;
pub mod kind;

pub use category::ActionCategory;
pub use fields::{
    ActionFields, AggregationFields, GovernanceFields, HopRef, InterestRateMode, LendingFields,
    LiquidStakingFields, LiquidityFields, NftFields, RestakingFields, RwaFields, SettlementKind,
    SignFields, SignSemantic, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
    TokenAmount, TokenAmountWithExpiry, UnknownFields, UtilityFields, VaultFields,
};
pub use kind::ActionType;

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub category: ActionCategory,
    #[serde(rename = "type")]
    pub action_type: ActionType,
    #[serde(rename = "primaryTargetId")]
    pub primary_target_id: String,
    #[serde(rename = "relatedTargetIds", default)]
    pub related_target_ids: Vec<String>,
    #[serde(rename = "derivedFromCallIds", default)]
    pub derived_from_call_ids: Vec<String>,
    #[serde(rename = "parentActionId", skip_serializing_if = "Option::is_none")]
    pub parent_action_id: Option<String>,
    pub fields: ActionFields,
    #[serde(rename = "extensionIds", default)]
    pub extension_ids: Vec<String>,
    pub confidence: Confidence,
}
