//! `Swap` action — single-pool or aggregator-routed token-for-token swap.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{AmmVenue, PoolState};

/// A token-for-token swap on a single pool or aggregator route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapAction {
    /// Entry contract the user calls (router / pool / aggregator).
    pub venue: AmmVenue,
    /// Deterministic user-signed intent.
    pub params: SwapParams,
    /// Inputs fetched at simulation time.
    pub live_inputs: SwapLiveInputs,
}

/// User-signed swap intent (amounts, slippage, recipient — but not the path).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapParams {
    /// Token the user is selling.
    pub token_in: TokenRef,
    /// Token the user is buying. `None` when the output token is not statically
    /// known from calldata (e.g. 1inch unoswap — token_out is the pool's other
    /// token, requiring an on-chain pool read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub token_out: Option<TokenRef>,
    /// Exact-in / exact-out direction and limits.
    pub direction: SwapDirection,
    /// Recipient of the output tokens.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Slippage tolerance in basis points, applied across the whole route.
    pub slippage_bp: u32,
}

/// User intent is just amount-in/out plus a limit — the actual *route* lives in `SwapLiveInputs.route`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SwapDirection {
    /// Sell an exact `amount_in`, requiring at least `min_amount_out`.
    ExactInput {
        /// Exact amount of `token_in` the user sells.
        #[tsify(type = "string")]
        amount_in: U256,
        /// Minimum acceptable amount of `token_out`.
        #[tsify(type = "string")]
        min_amount_out: U256,
    },
    /// Buy an exact `amount_out`, spending at most `max_amount_in`.
    ExactOutput {
        /// Maximum amount of `token_in` the user is willing to spend.
        #[tsify(type = "string")]
        max_amount_in: U256,
        /// Exact amount of `token_out` to receive.
        #[tsify(type = "string")]
        amount_out: U256,
    },
}

/// Simulation-time inputs for a swap: actual route, expected output, price impact, gas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapLiveInputs {
    /// Concrete executed route. Single-pool / single-hop has `paths.len() == 1 && hops.len() == 1`.
    /// Aggregator / multi-hop cases express split and cross-protocol routes here.
    pub route: LiveField<SwapRoute>,
    /// Estimated `token_out` summed across all paths.
    #[tsify(type = "LiveField<string>")]
    pub expected_amount_out: LiveField<U256>,
    /// Estimated price impact in basis points.
    pub price_impact_bp: LiveField<u32>,
    /// Estimated gas cost of the swap.
    #[tsify(type = "LiveField<string>")]
    pub gas_estimate: LiveField<U256>,
}

/// Concrete execution route for a swap — unified split + multi-hop + cross-protocol representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SwapRoute {
    /// Parallel paths. `Σ paths[i].share_bp == 10000`.
    pub paths: Vec<RoutePath>,
    /// Aggregator metadata; `None` for single-pool venues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub aggregator: Option<AggregatorMeta>,
}

/// One parallel branch of a `SwapRoute` — a serial sequence of hops carrying a share of the input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RoutePath {
    /// This path's share of the swap input in basis points. `10000` for a single path.
    pub share_bp: u32,
    /// Serial sequence of hops along this path.
    pub hops: Vec<RouteHop>,
    /// Estimated output produced by this path.
    #[tsify(type = "string")]
    pub estimated_out: U256,
}

/// One hop in a `RoutePath` — a single-pool venue swapping `token_in` -> `token_out`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct RouteHop {
    /// Hop input token.
    pub token_in: TokenRef,
    /// Hop output token.
    pub token_out: TokenRef,
    /// Single-pool venue executing this hop.
    pub venue: AmmVenue,
    /// Pool snapshot used by reducer math.
    pub pool_state: PoolState,
    /// Effective pool fee in basis points for this hop.
    pub effective_fee_bp: u32,
    /// Estimated output of this hop.
    #[tsify(type = "string")]
    pub estimated_out: U256,
}

/// Aggregator-specific metadata attached to a `SwapRoute`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AggregatorMeta {
    /// Aggregator product / version.
    pub aggregator: AggregatorKind,
    /// Router contract the user directly calls.
    #[tsify(type = "string")]
    pub router: Address,
    /// Separated executor (e.g. `1inch v6` splits router and executor) — critical for policy evaluation
    /// since policies typically whitelist known-safe executors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub executor: Option<Address>,
    /// 32-byte hex hash of raw calldata, for audit / replay verification.
    pub raw_calldata_hash: String,
    /// Whether a `Permit2` (or similar) approval is bundled with the swap.
    pub permit_bundled: bool,
    /// Optional referrer address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub referrer: Option<Address>,
    /// Referrer fee in basis points.
    pub referrer_fee_bp: u32,
}

/// Identity of an aggregator product (used inside `AggregatorMeta`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AggregatorKind {
    /// `1inch` Aggregation Router v5.
    OneInchV5,
    /// `1inch` Aggregation Router v6 (router / executor split).
    OneInchV6,
    /// `0x` Settler.
    ZeroExV2,
    /// `Paraswap` v5.
    ParaswapV5,
    /// `Paraswap` v6.
    ParaswapV6,
    /// `Kyberswap` aggregator v2.
    KyberswapV2,
    /// `Odos` router.
    Odos,
    /// `OKX` DEX aggregator.
    OkxAggregator,
    /// `Uniswap` `UniversalRouter` (can mix `V2`/`V3`/`V4`).
    UniswapUniversalRouter,
    /// `CoW Swap` direct solver settlement.
    CowSwapSolver,
    /// Unknown aggregator — `name` is a protocol identifier.
    Custom {
        /// Free-form aggregator name.
        name: String,
    },
}
