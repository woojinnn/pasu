//! `RemoveLiquidity` action — pooled burn, V3 decrease, or V3 burn.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U128, U256};
use simulation_state::token::{TokenKey, TokenRef};
use simulation_state::LiveField;

use super::{AmmVenue, PoolState};

/// Withdraw liquidity from a pool — pooled burn, V3 decrease, or V3 burn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RemoveLiquidityAction {
    /// Pool venue being withdrawn from.
    pub venue: AmmVenue,
    /// User-signed withdrawal parameters.
    pub params: RemoveLiquidityParams,
    /// Simulation-time inputs (pool snapshot, fees owed).
    pub live_inputs: RemoveLiquidityLiveInputs,
}

/// Variant of a remove-liquidity operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoveLiquidityParams {
    /// `Uniswap V2` / `Curve` / `Balancer`-style proportional burn of an LP token.
    PooledBurn {
        /// LP token being burned.
        lp_token: TokenRef,
        /// Amount of LP token to burn.
        #[tsify(type = "string")]
        lp_amount: U256,
        /// Minimum acceptable output per underlying token.
        #[tsify(type = "Array<[TokenRef, string]>")]
        min_out: Vec<(TokenRef, U256)>,
        /// Recipient of withdrawn tokens.
        #[tsify(type = "string")]
        recipient: Address,
    },
    /// `Uniswap V3` — decrease liquidity on an existing position NFT.
    ConcentratedDecrease {
        /// NFT position key.
        nft_key: TokenKey,
        /// Amount of `V3` liquidity to burn (uint128).
        #[tsify(type = "string")]
        liquidity_burn: U128,
        /// Minimum acceptable amounts (slippage floor) for each token.
        #[tsify(type = "[string, string]")]
        amount_min: (U256, U256),
    },
    /// `Uniswap V3` — burn an empty position NFT (must `liquidity == 0` first).
    ConcentratedBurn {
        /// NFT position key.
        nft_key: TokenKey,
    },
    /// `Curve` StableSwap-NG `remove_liquidity_one_coin` — burn LP for a single
    /// underlying coin `i` (the calldata index `i` is resolved to `token_out`'s
    /// address at manifest-author time via the pool's baked coin list).
    PooledBurnOneCoin {
        /// LP token being burned.
        lp_token: TokenRef,
        /// Amount of LP token to burn.
        #[tsify(type = "string")]
        lp_amount: U256,
        /// The single underlying coin withdrawn (pool `coins[i]`).
        token_out: TokenRef,
        /// Minimum acceptable output of `token_out` (slippage floor).
        #[tsify(type = "string")]
        min_out: U256,
        /// Recipient of the withdrawn token.
        #[tsify(type = "string")]
        recipient: Address,
    },
    /// `Curve` StableSwap-NG `remove_liquidity_imbalance` — withdraw exact
    /// per-coin amounts, capped by `max_lp_burn` LP spent.
    PooledBurnImbalance {
        /// LP token being burned.
        lp_token: TokenRef,
        /// Maximum LP the user is willing to burn.
        #[tsify(type = "string")]
        max_lp_burn: U256,
        /// Exact per-coin amounts to withdraw (`coins[k]`, amount).
        #[tsify(type = "Array<[TokenRef, string]>")]
        amounts_out: Vec<(TokenRef, U256)>,
        /// Recipient of the withdrawn tokens.
        #[tsify(type = "string")]
        recipient: Address,
    },
}

/// Simulation-time inputs for a remove-liquidity action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RemoveLiquidityLiveInputs {
    /// Current pool snapshot.
    pub pool_state: LiveField<PoolState>,
    /// Fees owed to the position at simulation time.
    #[tsify(type = "LiveField<Array<[TokenRef, string]>>")]
    pub fees_owed: LiveField<Vec<(TokenRef, U256)>>,
}
