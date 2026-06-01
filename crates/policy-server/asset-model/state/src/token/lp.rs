//! LP share 모양 — Pooled (V2/Curve/Balancer) vs Concentrated (V3/V4/Joe LB)
//! vs Custom (escape hatch).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsify_next::Tsify;

use super::token_ref::TokenRef;
use crate::primitives::{Weight, U128, U256};

/// LP share 가 fungible (ERC20) 인지 NFT (ERC721) 인지.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ShareForm {
    /// Uniswap V2 LP, Curve LP 등 ERC20 share.
    Fungible,
    /// Uniswap V3/V4 LP NFT 등 ERC721 share.
    NonFungible,
}

/// LP share 의 가격 분포 모양.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LpShape {
    /// 풀 전체에 비례 — Uniswap V2, Curve V1, Balancer.
    Pooled {
        /// pool 의 자산 별 weight (Balancer 등). 균등은 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        weights: Option<Vec<Weight>>,
    },

    /// 가격/틱 구간 집중 — Uniswap V3/V4, Trader Joe LB, Maverick.
    Concentrated {
        /// 본 share 의 집중 구간.
        range: RangeSpec,
        /// share 가 누적한 미수령 fee — (token, base unit) pair.
        #[serde(default)]
        #[tsify(type = "Array<[TokenRef, string]>")]
        fees_owed: Vec<(TokenRef, U256)>,
    },

    /// 위 두 모양에 안 맞는 LP — escape hatch.
    Custom {
        /// 프로토콜 식별자 (display 용).
        protocol: String,
        /// 프로토콜 고유 raw JSON.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}

/// `LpShape::Concentrated` 의 구간 표현.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RangeSpec {
    /// Uniswap V3/V4 의 tick 단위 집중.
    Tick {
        /// tick 하단 (포함).
        lower: i32,
        /// tick 상단 (포함).
        upper: i32,
        /// 구간 내 유동성 (U128).
        #[tsify(type = "string")]
        liquidity: U128,
    },

    /// Trader Joe LB 의 bin 단위 분포.
    Bin {
        /// 현재 활성 bin id.
        active_id: u32,
        /// bin id → 본 share 가 보유한 유동성 분포.
        #[tsify(type = "Array<[number, string]>")]
        distribution: Vec<(u32, U128)>,
    },

    /// Maverick 등 형식이 다른 경우.
    Custom {
        /// 프로토콜 식별자 (display 용).
        protocol: String,
        /// 프로토콜 고유 raw JSON.
        #[tsify(type = "unknown")]
        raw: Value,
    },
}
