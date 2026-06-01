//! `SignLimitOrderAction` — off-chain EIP-712 maker-sign of a Pendle limit `Order`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;

use super::YieldVenue;

/// Direction of a Pendle limit order (`IPLimitOrderType.OrderType`).
///
/// The maker commits `making_amount` of the input side and receives the output
/// side at or better than the signed limit rate. Tags map to the on-chain enum
/// ordinals (`SY_FOR_PT` = 0 … `YT_FOR_SY` = 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum LimitOrderType {
    /// `SY_FOR_PT` (0): maker gives SY, receives PT.
    SyForPt,
    /// `PT_FOR_SY` (1): maker gives PT, receives SY.
    PtForSy,
    /// `SY_FOR_YT` (2): maker gives SY, receives YT.
    SyForYt,
    /// `YT_FOR_SY` (3): maker gives YT, receives SY.
    YtForSy,
}

/// Off-chain maker-sign of a Pendle limit `Order` (EIP-712, `PendleLimitRouter`,
/// domain `"Pendle Limit Order Protocol"` v1).
///
/// The user signs as `maker`; the order is filled later by a relayer/taker. The
/// pre-sign surface is what the maker commits: direction, the SY token side, the
/// YT (market identity), how much is committed, who receives the output, and how
/// long the signed order stays valid. `salt`/`nonce`/`lnImpliedRate`/
/// `failSafeRate`/`permit` are order-mechanics and not modeled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SignLimitOrderAction {
    /// Yield venue (Pendle V2 on a given chain).
    pub venue: YieldVenue,
    /// Order direction (`Order.orderType`).
    pub order_type: LimitOrderType,
    /// SY token side of the order (`Order.token`).
    pub token: TokenRef,
    /// Yield Token identifying the market (`Order.YT`), address.
    #[tsify(type = "string")]
    pub yt: Address,
    /// Order maker / signer (`Order.maker`), address.
    #[tsify(type = "string")]
    pub maker: Address,
    /// Output recipient (`Order.receiver`), address.
    #[tsify(type = "string")]
    pub receiver: Address,
    /// Amount the maker commits (`Order.makingAmount`), U256.
    #[tsify(type = "string")]
    pub making_amount: U256,
    /// Order expiry as a unix timestamp (`Order.expiry`), U256.
    #[tsify(type = "string")]
    pub expiry: U256,
}
