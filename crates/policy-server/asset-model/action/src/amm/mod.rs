//! `AmmAction` — `Swap` / `AddLiquidity` / `RemoveLiquidity` / `CollectFees` / `IntentOrder`. Spec §5.
//! Venue discriminator pattern: a single `AmmAction::Swap` variant plus an `AmmVenue` enum.
//! `run_action` then dispatches per-protocol math via a single `match venue { ... }`.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId, U128, U256};

pub mod add_liquidity;
pub mod collect_fees;
pub mod intent;
pub mod remove_liquidity;
pub mod swap;

pub use self::add_liquidity::*;
pub use self::collect_fees::*;
pub use self::intent::*;
pub use self::remove_liquidity::*;
pub use self::swap::*;

// ---------------------------------------------------------------------------
// Domain enum
// ---------------------------------------------------------------------------

/// Top-level AMM action: swaps, liquidity provisioning, fee collection,
/// and intent-based (off-chain signed) orders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum AmmAction {
    /// Token-for-token swap on a single pool or an aggregator route.
    Swap(SwapAction),
    /// Deposit liquidity into a pool (`Uniswap V2`/`V3`, `Curve`, `Balancer`, ...).
    AddLiquidity(AddLiquidityAction),
    /// Withdraw liquidity from a pool / burn an LP or position NFT.
    RemoveLiquidity(RemoveLiquidityAction),
    /// `Uniswap V3`-style collection of accrued, uncollected fees.
    CollectFees(CollectFeesAction),
    /// Sign an EIP-712 intent order (`UniswapX` / `CowSwap` / `1inch Fusion`, ...).
    SignIntentOrder(SignIntentOrderAction),
    /// Cancel a previously signed intent order.
    CancelIntentOrder(CancelIntentOrderAction),
}

impl AmmAction {
    /// The action's `serde` `action` tag (e.g. `"swap"`, `"sign_intent_order"`).
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Swap(_) => "swap",
            Self::AddLiquidity(_) => "add_liquidity",
            Self::RemoveLiquidity(_) => "remove_liquidity",
            Self::CollectFees(_) => "collect_fees",
            Self::SignIntentOrder(_) => "sign_intent_order",
            Self::CancelIntentOrder(_) => "cancel_intent_order",
        }
    }

    /// The venue `name` of the wrapped action. Every AMM action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Swap(a) => Some(a.venue.name()),
            Self::AddLiquidity(a) => Some(a.venue.name()),
            Self::RemoveLiquidity(a) => Some(a.venue.name()),
            Self::CollectFees(a) => Some(a.venue.name()),
            Self::SignIntentOrder(a) => Some(a.venue.name()),
            Self::CancelIntentOrder(a) => Some(a.venue.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Single-pool venue (`Uniswap V2 / V3 / V4`, `Curve`, `Balancer`, `Trader Joe LB`, `Maverick`)
/// or an aggregator router that orchestrates a multi-hop / split route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum AmmVenue {
    /// `Uniswap V2`-style constant-product pool.
    UniswapV2 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
        /// `Uniswap V2` factory that minted the pool.
        #[tsify(type = "string")]
        factory: Address,
    },
    /// `Uniswap V3` concentrated-liquidity pool.
    UniswapV3 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
        /// Fee tier in basis points x 100 (e.g. 0.05% = 500).
        fee_tier_bp: u32,
    },
    /// `Uniswap V4` singleton pool keyed by `pool_id`.
    UniswapV4 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// `bytes32` pool id encoded as hex.
        pool_id: String,
        /// Singleton `PoolManager` contract.
        #[tsify(type = "string")]
        pool_manager: Address,
        /// Hooks contract attached to the pool (zero address if none).
        #[tsify(type = "string")]
        hooks: Address,
    },
    /// `SushiSwap V2` (a `Uniswap V2` fork).
    SushiV2 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
    },
    /// `Curve V1` stableswap pool.
    CurveV1 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
        /// Number of coins held by the pool.
        n_coins: u8,
        /// Whether this is a meta-pool (paired against a base LP token).
        is_meta: bool,
    },
    /// `Curve V2` cryptoswap pool.
    CurveV2 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
    },
    /// `Balancer V2` vault-routed pool.
    BalancerV2 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// `Balancer V2` `Vault` contract.
        #[tsify(type = "string")]
        vault: Address,
        /// `bytes32` pool id encoded as hex.
        pool_id: String,
        /// Underlying math model.
        pool_type: BalancerPoolType,
    },
    /// `Balancer V3` pool.
    BalancerV3 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool id (hex encoded).
        pool_id: String,
        /// Underlying math model.
        pool_type: BalancerPoolType,
    },
    /// `Trader Joe` Liquidity Book bin-based pool.
    TraderJoeLB {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Liquidity Book pair contract.
        #[tsify(type = "string")]
        pair: Address,
        /// Bin step in basis-point units.
        bin_step: u16,
    },
    /// `Maverick V2` directional pool.
    MaverickV2 {
        /// Chain the pool lives on.
        chain: ChainId,
        /// Pool contract address.
        #[tsify(type = "string")]
        pool: Address,
    },
    /// Aggregator router (e.g. `1inch`, `0x`, `Paraswap`).
    /// The actual executed route is carried in `SwapLiveInputs.route`.
    AggregatorRoute {
        /// Chain the router lives on.
        chain: ChainId,
        /// Router contract the user calls.
        #[tsify(type = "string")]
        router: Address,
        /// 32-byte hex hash of the route calldata.
        route_hash: String,
    },
}

impl AmmVenue {
    /// The venue's `serde` `name` tag (e.g. `"uniswap_v3"`, `"trader_joe_l_b"`).
    /// These strings match the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminants exactly and are verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::UniswapV2 { .. } => "uniswap_v2",
            Self::UniswapV3 { .. } => "uniswap_v3",
            Self::UniswapV4 { .. } => "uniswap_v4",
            Self::SushiV2 { .. } => "sushi_v2",
            Self::CurveV1 { .. } => "curve_v1",
            Self::CurveV2 { .. } => "curve_v2",
            Self::BalancerV2 { .. } => "balancer_v2",
            Self::BalancerV3 { .. } => "balancer_v3",
            Self::TraderJoeLB { .. } => "trader_joe_l_b",
            Self::MaverickV2 { .. } => "maverick_v2",
            Self::AggregatorRoute { .. } => "aggregator_route",
        }
    }
}

/// Math model for a `Balancer V2` / `V3` pool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum BalancerPoolType {
    /// Weighted pool (e.g. 80/20 BAL/WETH).
    Weighted,
    /// Classic stable pool.
    Stable,
    /// Composable stable pool (LP token as a pool asset).
    ComposableStable,
    /// `MetaStable` pool (with price-rate providers).
    MetaStable,
    /// Liquidity Bootstrapping Pool (LBP) with shifting weights.
    LiquidityBootstrapping,
    /// Linear pool (wraps a yield-bearing token at a target rate).
    Linear,
}

// ---------------------------------------------------------------------------
// PoolState — per-venue pool snapshot
// ---------------------------------------------------------------------------

/// Venue-specific pool snapshot consumed by reducer math at simulation time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PoolState {
    /// `Uniswap V2` / Sushi / fork — `x * y = k`.
    XyConstant {
        /// Reserve of the input token.
        #[tsify(type = "string")]
        reserve_in: U256,
        /// Reserve of the output token.
        #[tsify(type = "string")]
        reserve_out: U256,
        /// Pool fee in basis points.
        fee_bp: u32,
    },

    /// `Uniswap V3` / `V4` — concentrated liquidity.
    Concentrated {
        /// `sqrtPriceX96` (`Uniswap V3` convention).
        #[tsify(type = "string")]
        sqrt_price_x96: U256,
        /// Current active tick.
        tick: i32,
        /// Active in-range liquidity (uint128).
        #[tsify(type = "string")]
        liquidity: U128,
        /// Neighboring tick snapshots needed for slippage calculation.
        ticks: Vec<TickSnapshot>,
    },

    /// `Curve V1` stableswap.
    StableV1 {
        /// Per-coin balances (length = `n_coins`).
        #[tsify(type = "Array<string>")]
        balances: Vec<U256>,
        /// Amplification coefficient `A`.
        a: u32,
        /// Pool fee in basis points.
        fee_bp: u32,
    },

    /// `Curve V2` cryptoswap.
    Cryptoswap {
        /// Per-coin balances.
        #[tsify(type = "Array<string>")]
        balances: Vec<U256>,
        /// Per-coin price scale.
        #[tsify(type = "Array<string>")]
        price_scale: Vec<U256>,
        /// `(A, gamma)` packed into a single `U256`.
        #[tsify(type = "string")]
        a_gamma: U256,
        /// Pool fee in basis points.
        fee_bp: u32,
    },

    /// `Balancer` Weighted pool (e.g. 80/20).
    Weighted {
        /// Per-token balances.
        #[tsify(type = "Array<string>")]
        balances: Vec<U256>,
        /// Per-token weights (scaled).
        weights: Vec<u64>,
        /// Pool fee in basis points.
        fee_bp: u32,
    },

    /// `Balancer` Stable / Composable Stable pool.
    Stable {
        /// Per-token balances.
        #[tsify(type = "Array<string>")]
        balances: Vec<U256>,
        /// Amplification coefficient.
        amp: u32,
        /// Pool fee in basis points.
        fee_bp: u32,
    },

    /// `Trader Joe` Liquidity Book pool.
    LiquidityBook {
        /// Currently active bin id.
        active_bin_id: u32,
        /// Adjacent bin snapshots needed for swap simulation.
        bins: Vec<BinState>,
        /// Variable fee component in basis points.
        variable_fee_bp: u32,
    },

    /// `Maverick` directional pool — per-mode payload added in prototyping.
    Maverick {
        /// Known mode identifier (`"mode_left"`, `"mode_right"`, `"mode_both"`, `"mode_dynamic"`).
        mode: String,
        #[tsify(type = "unknown")]
        raw: serde_json::Value,
    },

    Custom {
        /// Protocol identifier.
        protocol: String,
        /// Raw protocol-specific payload.
        #[tsify(type = "unknown")]
        raw: serde_json::Value,
    },
}

/// Snapshot of a single `Uniswap V3` / `V4` tick used for slippage math.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct TickSnapshot {
    /// Tick index.
    pub tick: i32,
    /// Signed `liquidity_net` (can be negative; `Uniswap V3` uses `int128`).
    pub liquidity_net: String,
}

/// Snapshot of a single `Trader Joe` Liquidity Book bin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BinState {
    /// Bin id.
    pub id: u32,
    /// Bin's reserve of the input token.
    #[tsify(type = "string")]
    pub reserve_in: U256,
    /// Bin's reserve of the output token.
    #[tsify(type = "string")]
    pub reserve_out: U256,
}
