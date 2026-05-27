//! LP share 모양 — Pooled (V2/Curve/Balancer) vs Concentrated (V3/V4/Joe LB)
//! vs Custom (escape hatch).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::token_ref::TokenRef;
use crate::primitives::{U128, U256, Weight};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareForm {
    Fungible,
    NonFungible,
}

/// LP share 의 가격 분포 모양.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LpShape {
    /// 풀 전체에 비례 — Uniswap V2, Curve V1, Balancer.
    Pooled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weights: Option<Vec<Weight>>,
    },

    /// 가격/틱 구간 집중 — Uniswap V3/V4, Trader Joe LB, Maverick.
    Concentrated {
        range: RangeSpec,
        #[serde(default)]
        fees_owed: Vec<(TokenRef, U256)>,
    },

    /// 위 두 모양에 안 맞는 LP — escape hatch.
    Custom { protocol: String, raw: Value },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RangeSpec {
    /// Uniswap V3/V4 의 tick 단위 집중.
    Tick {
        lower: i32,
        upper: i32,
        liquidity: U128,
    },

    /// Trader Joe LB 의 bin 단위 분포.
    Bin {
        active_id: u32,
        distribution: Vec<(u32, U128)>,
    },

    /// Maverick 등 형식이 다른 경우.
    Custom { protocol: String, raw: Value },
}
