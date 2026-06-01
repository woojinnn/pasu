//! `AddLiquidity` action — pooled deposit, V3 mint, or V3 increase.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, Price, U256};
use simulation_state::token::{RangeSpec, TokenKey, TokenRef};
use simulation_state::LiveField;

use super::{AmmVenue, PoolState};

/// Deposit liquidity into a pool — pooled deposit, V3 mint, or V3 increase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AddLiquidityAction {
    /// Pool venue receiving the deposit.
    pub venue: AmmVenue,
    /// User-signed deposit parameters.
    pub params: AddLiquidityParams,
    /// Simulation-time inputs (pool snapshot, current price).
    pub live_inputs: AddLiquidityLiveInputs,
}

/// Variant of an add-liquidity operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AddLiquidityParams {
    /// `Uniswap V2` / `Curve` / `Balancer`-style proportional deposit.
    Pooled {
        /// Tokens deposited; for weighted pools length must match the pool's token count.
        #[tsify(type = "Array<[TokenRef, string]>")]
        tokens: Vec<(TokenRef, U256)>,
        /// Minimum LP tokens out.
        #[tsify(type = "string")]
        min_lp_out: U256,
        /// Recipient of the LP tokens.
        #[tsify(type = "string")]
        recipient: Address,
    },

    /// `Uniswap V3` — mint a new position NFT.
    ConcentratedMint {
        /// Token pair of the pool.
        pool_pair: (TokenRef, TokenRef),
        /// Desired amounts for each token in the pair.
        #[tsify(type = "[string, string]")]
        amount_desired: (U256, U256),
        /// Minimum acceptable amounts (slippage floor) for each token.
        #[tsify(type = "[string, string]")]
        amount_min: (U256, U256),
        /// Tick range for the new position.
        range: RangeSpec,
        /// Recipient of the position NFT.
        #[tsify(type = "string")]
        recipient: Address,
    },

    /// `Uniswap V3` — add liquidity to an existing position NFT.
    ConcentratedIncrease {
        /// NFT position key.
        nft_key: TokenKey,
        /// Desired amounts for each token in the pair.
        #[tsify(type = "[string, string]")]
        amount_desired: (U256, U256),
        /// Minimum acceptable amounts (slippage floor) for each token.
        #[tsify(type = "[string, string]")]
        amount_min: (U256, U256),
    },
}

/// Simulation-time inputs for an add-liquidity action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AddLiquidityLiveInputs {
    /// Current pool snapshot.
    pub pool_state: LiveField<PoolState>,
    /// Current pool price — used to validate the chosen range.
    pub current_price: LiveField<Price>,
}
