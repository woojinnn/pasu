//! New-model Cedar lowering — `simulation_reducer::action::ActionBody` →
//! [`LoweredAction`] (`Wallet` / `Amm::Action::"…"` / `Protocol` + cedarschema
//! action-context JSON).
//!
//! This is the ADDITIVE counterpart to the legacy [`crate::lowering`] pipeline
//! (which consumes the old `ActionEnvelope`). It targets the new action model
//! directly and produces a context object that conforms to the per-action
//! cedarschema types under `schema/policy-schema/actions/`. The two pipelines
//! run side by side; this module never touches the legacy one.
//!
//! # Scope
//!
//! The first vertical slice handles [`AmmAction::Swap`] fully (plus the shared
//! sub-lowerings reused by future actions: token refs/keys, AMM venue, action
//! meta / nature, EIP-712 domain, swap direction). Every other [`ActionBody`]
//! variant returns [`LowerError::Unsupported`] carrying a `domain/tag` label.
//!
//! # Conventions (mirrors the legacy `lowering::dispatch`)
//!
//! - `principal` = `Wallet::"<tx.from>"`
//! - `resource`  = `Protocol::"<tx.to>"`
//! - `action_uid` is namespaced + `PascalCase`, e.g. `Amm::Action::"Swap"`.
//!
//! # JSON shape rules
//!
//! Cedar 4.10 has no enums/unions: Rust enums are modelled as discriminated
//! records (`{ kind | name | standard: String, …optional }`) and `LiveField<T>`
//! is inlined as its underlying `T`. The cedarschema uses **camelCase** keys, so
//! every record is hand-built (a blind `serde_json::to_value` of the Rust struct
//! would emit `snake_case` keys and whole `LiveField` objects). Optional fields are
//! **omitted** when absent — never emitted as `null`. `Long` fields are plain
//! JSON numbers; `U256`/`U128` values are lower-hex strings (`{:#x}`).

use serde_json::{Map, Value};

use simulation_reducer::action::amm::{
    AmmAction, AmmVenue, BalancerPoolType, SwapAction, SwapDirection,
};
use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
use simulation_state::primitives::Address;
use simulation_state::token::{TokenKey, TokenRef};

/// A lowered action ready for the Cedar engine: the `principal` / `action` /
/// `resource` entity uids (as parseable strings) plus the action-context JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweredAction {
    /// Principal entity uid, e.g. `Wallet::"0xabc…"`.
    pub principal: String,
    /// Action entity uid, namespaced + `PascalCase`, e.g. `Amm::Action::"Swap"`.
    pub action_uid: String,
    /// Resource entity uid, e.g. `Protocol::"0xrouter…"`.
    pub resource: String,
    /// The cedarschema action-context object (conforms to the action's
    /// `*Context` type, e.g. `Amm::SwapContext`).
    pub context: Value,
}

/// Transaction-level fields the lowering needs for the `principal` / `resource`
/// entity uids. Mirrors the legacy dispatch: `principal = Wallet::"<from>"`,
/// `resource = Protocol::"<to>"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxMeta<'a> {
    /// Transaction sender — becomes the `Wallet` principal.
    pub from: &'a str,
    /// Transaction target — becomes the `Protocol` resource.
    pub to: &'a str,
}

/// Error returned when an [`ActionBody`] variant has no new-model lowering yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// The action has no lowering in this slice. Carries a `domain/tag` label
    /// (e.g. `"amm/add_liquidity"`, `"unknown"`).
    Unsupported(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(label) => write!(f, "unsupported action: {label}"),
        }
    }
}

impl std::error::Error for LowerError {}

/// Lower an [`ActionBody`] to a [`LoweredAction`].
///
/// `meta` is the [`ActionMeta`] carried by the *outer* `Action` (submitter,
/// submission time, on-chain/off-chain nature). It is a separate parameter
/// because the new action model keeps `meta` on `Action`, not on `ActionBody`;
/// the per-action `*Context` types all require a `meta: Core::ActionMeta` field,
/// so it must be threaded in here. `tx` carries only the EVM routing addresses
/// (`from` / `to`), which are not part of the action model.
///
/// For this slice only [`AmmAction::Swap`] is lowered fully; every other variant
/// returns [`LowerError::Unsupported`] with a `domain/tag` label.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for any action variant other than
/// `Amm::Swap`.
pub fn lower_action(
    action: &ActionBody,
    meta: &ActionMeta,
    tx: &TxMeta<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        ActionBody::Amm(AmmAction::Swap(swap)) => Ok(lower_swap(swap, meta, tx)),
        other => Err(LowerError::Unsupported(unsupported_label(other))),
    }
}

/// Build the `domain/tag` label for an unsupported action (used in
/// [`LowerError::Unsupported`]).
fn unsupported_label(action: &ActionBody) -> String {
    match action {
        ActionBody::Token(a) => format!("token/{}", a.action_tag()),
        ActionBody::Amm(a) => format!("amm/{}", a.action_tag()),
        ActionBody::Lending(a) => format!("lending/{}", a.action_tag()),
        ActionBody::Airdrop(a) => format!("airdrop/{}", a.action_tag()),
        ActionBody::Launchpad(a) => format!("launchpad/{}", a.action_tag()),
        ActionBody::Perp(a) => format!("perp/{}", a.action_tag()),
        ActionBody::Multicall { .. } => "multicall".to_owned(),
        ActionBody::Unknown { .. } => "unknown".to_owned(),
    }
}

/// Lower an `Amm::Swap` action into the `Amm::SwapContext` shape.
fn lower_swap(swap: &SwapAction, meta: &ActionMeta, tx: &TxMeta<'_>) -> LoweredAction {
    // `route` is `LiveField<SwapRoute>`; the policy layer sees only the summed
    // estimated-out across all paths (route shape is too complex for Cedar).
    let route_estimated_out = swap
        .live_inputs
        .route
        .value
        .paths
        .iter()
        .fold(simulation_state::primitives::U256::ZERO, |acc, p| {
            acc.saturating_add(p.estimated_out)
        });

    let (recipient, slippage_bp, direction) = (
        addr(&swap.params.recipient),
        i64::from(swap.params.slippage_bp),
        lower_swap_direction(&swap.params.direction),
    );

    let mut ctx = Map::new();
    ctx.insert("meta".into(), lower_action_meta(meta));
    ctx.insert("venue".into(), lower_amm_venue(&swap.venue));
    ctx.insert("tokenIn".into(), lower_token_ref(&swap.params.token_in));
    ctx.insert("tokenOut".into(), lower_token_ref(&swap.params.token_out));
    ctx.insert("direction".into(), direction);
    ctx.insert("recipient".into(), Value::String(recipient));
    ctx.insert("slippageBp".into(), Value::from(slippage_bp));
    ctx.insert(
        "routeEstimatedOut".into(),
        Value::String(u256_hex(route_estimated_out)),
    );
    ctx.insert(
        "expectedAmountOut".into(),
        Value::String(u256_hex(swap.live_inputs.expected_amount_out.value)),
    );
    ctx.insert(
        "priceImpactBp".into(),
        Value::from(i64::from(swap.live_inputs.price_impact_bp.value)),
    );
    ctx.insert(
        "gasEstimate".into(),
        Value::String(u256_hex(swap.live_inputs.gas_estimate.value)),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    LoweredAction {
        principal: format!(r#"Wallet::"{}""#, tx.from),
        action_uid: r#"Amm::Action::"Swap""#.to_owned(),
        resource: format!(r#"Protocol::"{}""#, tx.to),
        context: Value::Object(ctx),
    }
}

// ---------------------------------------------------------------------------
// Shared sub-lowerings (reused by future actions)
// ---------------------------------------------------------------------------

/// Lower a [`TokenRef`] → `{ "key": <TokenKey> }` (`Core::TokenRef`).
fn lower_token_ref(token: &TokenRef) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), lower_token_key(&token.key));
    Value::Object(m)
}

/// Lower a [`TokenKey`] → discriminated `{ standard, chain, address?, contract?,
/// tokenId? }` (`Core::TokenKey`).
fn lower_token_key(key: &TokenKey) -> Value {
    let mut m = Map::new();
    match key {
        TokenKey::Native { chain } => {
            m.insert("standard".into(), Value::String("native".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
        TokenKey::Erc20 { chain, address } => {
            m.insert("standard".into(), Value::String("erc20".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("address".into(), Value::String(addr(address)));
        }
        // Erc721 and Erc1155 share the `{ contract, tokenId }` shape and differ
        // only by the `standard` discriminator.
        TokenKey::Erc721 {
            chain,
            contract,
            token_id,
        }
        | TokenKey::Erc1155 {
            chain,
            contract,
            token_id,
        } => {
            let standard = if key.is_nft() { "erc721" } else { "erc1155" };
            m.insert("standard".into(), Value::String(standard.into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
            m.insert("tokenId".into(), Value::String(u256_hex(*token_id)));
        }
    }
    Value::Object(m)
}

/// Lower an [`AmmVenue`] → `{ name, chain, <per-variant optional fields> }`
/// (`Amm::AmmVenue`). Only the fields a variant carries are emitted.
// Flat exhaustive dispatch over 11 venue variants: splitting it would hide the
// 1:1 variant→field-set mapping that is the whole point of this function.
#[allow(clippy::too_many_lines)]
fn lower_amm_venue(venue: &AmmVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        AmmVenue::UniswapV2 {
            chain,
            pool,
            factory,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("factory".into(), Value::String(addr(factory)));
        }
        AmmVenue::UniswapV3 {
            chain,
            pool,
            fee_tier_bp,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("feeTierBp".into(), Value::from(i64::from(*fee_tier_bp)));
        }
        AmmVenue::UniswapV4 {
            chain,
            pool_id,
            pool_manager,
            hooks,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert("poolManager".into(), Value::String(addr(pool_manager)));
            m.insert("hooks".into(), Value::String(addr(hooks)));
        }
        // SushiV2 / CurveV2 / MaverickV2 all expose only `{ chain, pool }`; the
        // discriminating `name` is already set above, so they share one arm.
        AmmVenue::SushiV2 { chain, pool }
        | AmmVenue::CurveV2 { chain, pool }
        | AmmVenue::MaverickV2 { chain, pool } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
        }
        AmmVenue::CurveV1 {
            chain,
            pool,
            n_coins,
            is_meta,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("nCoins".into(), Value::from(i64::from(*n_coins)));
            m.insert("isMeta".into(), Value::Bool(*is_meta));
        }
        AmmVenue::BalancerV2 {
            chain,
            vault,
            pool_id,
            pool_type,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("vault".into(), Value::String(addr(vault)));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert(
                "poolType".into(),
                Value::String(balancer_pool_type(pool_type).into()),
            );
        }
        AmmVenue::BalancerV3 {
            chain,
            pool_id,
            pool_type,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert(
                "poolType".into(),
                Value::String(balancer_pool_type(pool_type).into()),
            );
        }
        AmmVenue::TraderJoeLB {
            chain,
            pair,
            bin_step,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pair".into(), Value::String(addr(pair)));
            m.insert("binStep".into(), Value::from(i64::from(*bin_step)));
        }
        AmmVenue::AggregatorRoute {
            chain,
            router,
            route_hash,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("router".into(), Value::String(addr(router)));
            m.insert("routeHash".into(), Value::String(route_hash.clone()));
        }
    }
    Value::Object(m)
}

/// Map a [`BalancerPoolType`] to its `snake_case` cedarschema `poolType` spelling.
const fn balancer_pool_type(pool_type: &BalancerPoolType) -> &'static str {
    match pool_type {
        BalancerPoolType::Weighted => "weighted",
        BalancerPoolType::Stable => "stable",
        BalancerPoolType::ComposableStable => "composable_stable",
        BalancerPoolType::MetaStable => "meta_stable",
        BalancerPoolType::LiquidityBootstrapping => "liquidity_bootstrapping",
        BalancerPoolType::Linear => "linear",
    }
}

/// Lower an [`ActionMeta`] → `{ submittedAt, submitter, nature }`
/// (`Core::ActionMeta`).
fn lower_action_meta(meta: &ActionMeta) -> Value {
    let mut m = Map::new();
    // `submittedAt` is a unix-seconds Long (JSON number).
    m.insert(
        "submittedAt".into(),
        Value::from(meta.submitted_at.as_unix()),
    );
    m.insert("submitter".into(), Value::String(addr(&meta.submitter)));
    m.insert("nature".into(), lower_nature(&meta.nature));
    Value::Object(m)
}

/// Lower an [`ActionNature`] → discriminated `{ kind, … }` (`Core::ActionNature`).
fn lower_nature(nature: &ActionNature) -> Value {
    let mut m = Map::new();
    match nature {
        ActionNature::OnchainTx {
            chain,
            nonce,
            gas_limit,
            gas_price,
            value,
        } => {
            m.insert("kind".into(), Value::String("onchain_tx".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("nonce".into(), Value::from(*nonce));
            m.insert("gasLimit".into(), Value::String(u256_hex(*gas_limit)));
            // gas_price is a LiveField<U256>: inline its inner value.
            m.insert("gasPrice".into(), Value::String(u256_hex(gas_price.value)));
            m.insert("value".into(), Value::String(u256_hex(*value)));
        }
        ActionNature::OffchainSig {
            domain,
            deadline,
            nonce_key,
        } => {
            m.insert("kind".into(), Value::String("offchain_sig".into()));
            m.insert("domain".into(), lower_eip712(domain));
            m.insert("deadline".into(), Value::from(deadline.as_unix()));
            if let Some(nonce_key) = nonce_key {
                // `nonceKey` is a String slot for an app-specific key; the Rust
                // `NonceKey` is an enum, so serialize it and stringify (compact
                // JSON) into the slot. Omitted entirely when absent.
                if let Ok(serialized) = serde_json::to_string(nonce_key) {
                    m.insert("nonceKey".into(), Value::String(serialized));
                }
            }
        }
    }
    Value::Object(m)
}

/// Lower an [`Eip712Domain`] → `{ name, version?, chainId?, verifyingContract?,
/// salt? }` (`Core::Eip712Domain`). Absent optionals are omitted.
fn lower_eip712(domain: &Eip712Domain) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(domain.name.clone()));
    if let Some(version) = &domain.version {
        m.insert("version".into(), Value::String(version.clone()));
    }
    if let Some(chain_id) = domain.chain_id {
        m.insert("chainId".into(), Value::from(chain_id));
    }
    if let Some(verifying_contract) = &domain.verifying_contract {
        m.insert(
            "verifyingContract".into(),
            Value::String(addr(verifying_contract)),
        );
    }
    if let Some(salt) = &domain.salt {
        m.insert("salt".into(), Value::String(salt.clone()));
    }
    Value::Object(m)
}

/// Lower a [`SwapDirection`] → discriminated `{ kind, … }` (`Amm::SwapDirection`).
/// The optional nano/usd sibling fields are host-populated and omitted here.
fn lower_swap_direction(direction: &SwapDirection) -> Value {
    let mut m = Map::new();
    match direction {
        SwapDirection::ExactInput {
            amount_in,
            min_amount_out,
        } => {
            m.insert("kind".into(), Value::String("exact_input".into()));
            m.insert("amountIn".into(), Value::String(u256_hex(*amount_in)));
            m.insert(
                "minAmountOut".into(),
                Value::String(u256_hex(*min_amount_out)),
            );
        }
        SwapDirection::ExactOutput {
            max_amount_in,
            amount_out,
        } => {
            m.insert("kind".into(), Value::String("exact_output".into()));
            m.insert(
                "maxAmountIn".into(),
                Value::String(u256_hex(*max_amount_in)),
            );
            m.insert("amountOut".into(), Value::String(u256_hex(*amount_out)));
        }
    }
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Lower-hex (`0x…`) rendering of a U256 (alloy `LowerHex`).
fn u256_hex(v: simulation_state::primitives::U256) -> String {
    format!("{v:#x}")
}

/// Lowercase `0x`-hex rendering of an [`Address`] (alloy `LowerHex`), matching
/// the spec's "always lowercase" address convention.
fn addr(a: &Address) -> String {
    format!("{a:#x}")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::redundant_clone
)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_reducer::action::amm::{
        AmmAction, AmmVenue, PoolState, RouteHop, RoutePath, SwapAction, SwapDirection,
        SwapLiveInputs, SwapParams, SwapRoute,
    };
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Duration, Time, U128, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::{LiveField, NonceKey};

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A UniswapV3 single-hop ExactInput USDC→WETH swap on Arbitrum
    /// (OnchainTx), parameterized by slippage bp. Mirrors the
    /// `uniswap_v3_arbitrum_single_hop_round_trip` smoke test.
    fn sample_swap_action(slippage_bp: u32) -> (ActionBody, ActionMeta) {
        let chain = ChainId::arbitrum();
        let usdc = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xaf88d065e77c8cc2239327c5edb3a432268e5831").unwrap(),
            },
        };
        let weth = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x82af49447d8a07e3bd95bd0d56f35241523fbab1").unwrap(),
            },
        };
        let pool = Address::from_str("0xc6962004f452be9203591991d15f6b388e09e8d0").unwrap();

        let v3 = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool,
            fee_tier_bp: 500,
        };

        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };

        let pool_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "slot0()".into(),
            decoder_id: "uniswap_v3_slot0".into(),
        };

        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10000,
                hops: vec![RouteHop {
                    token_in: usdc.clone(),
                    token_out: weth.clone(),
                    venue: v3.clone(),
                    pool_state,
                    effective_fee_bp: 5,
                    estimated_out: U256::from(305_000_000_000_000_000u64),
                }],
                estimated_out: U256::from(305_000_000_000_000_000u64),
            }],
            aggregator: None,
        };

        let swap = AmmAction::Swap(SwapAction {
            venue: v3,
            params: SwapParams {
                token_in: usdc,
                token_out: weth,
                direction: SwapDirection::ExactInput {
                    amount_in: U256::from(1_000_000_000u64),
                    min_amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user(),
                slippage_bp,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, pool_source.clone(), now())
                    .with_ttl(Duration::from_secs(12)),
                expected_amount_out: LiveField::new(
                    U256::from(305_000_000_000_000_000u64),
                    pool_source.clone(),
                    now(),
                ),
                price_impact_bp: LiveField::new(12u32, pool_source, now()),
                gas_estimate: LiveField::new(
                    U256::from(180_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "gas/arbitrum".into(),
                    },
                    now(),
                ),
            },
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain,
                nonce: 42,
                gas_limit: U256::from(200_000u64),
                gas_price: LiveField::new(
                    U256::from(100_000_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "ETH/USD".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        };

        (ActionBody::Amm(swap), meta)
    }

    /// A second, deliberately different swap to widen the conformance gate:
    /// AggregatorRoute venue (router/routeHash branch), ExactOutput direction,
    /// Native `tokenOut`, and an OffchainSig meta (exercises `lower_eip712` +
    /// `lower_nature` offchain branch + `nonceKey`).
    fn sample_swap_aggregator_offchain() -> (ActionBody, ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let usdc = TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        // Native tokenOut exercises the TokenKey::Native branch.
        let eth = TokenRef {
            key: TokenKey::Native {
                chain: chain.clone(),
            },
        };
        let router = Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap();

        let venue = AmmVenue::AggregatorRoute {
            chain: chain.clone(),
            router,
            route_hash: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
        };

        let hop_venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };

        let src = DataSource::VenueApi {
            endpoint: "https://api.1inch.dev/swap/v6.0/1/swap".into(),
            parser_id: "oneinch_v6_route".into(),
            auth: None,
        };

        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10000,
                hops: vec![RouteHop {
                    token_in: usdc.clone(),
                    token_out: eth.clone(),
                    venue: hop_venue,
                    pool_state,
                    effective_fee_bp: 5,
                    estimated_out: U256::from(300_000_000_000_000_000u64),
                }],
                estimated_out: U256::from(300_000_000_000_000_000u64),
            }],
            aggregator: None,
        };

        let swap = AmmAction::Swap(SwapAction {
            venue,
            params: SwapParams {
                token_in: usdc,
                token_out: eth,
                direction: SwapDirection::ExactOutput {
                    max_amount_in: U256::from(1_100_000_000u64),
                    amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user(),
                slippage_bp: 75,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, src.clone(), now()),
                expected_amount_out: LiveField::new(
                    U256::from(300_000_000_000_000_000u64),
                    src.clone(),
                    now(),
                ),
                price_impact_bp: LiveField::new(18u32, src.clone(), now()),
                gas_estimate: LiveField::new(U256::from(280_000u64), src, now()),
            },
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Permit2".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x000000000022d473030f116ddee9f6b43ac78ba3").unwrap(),
                    ),
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
                }),
            },
        };

        (ActionBody::Amm(swap), meta)
    }

    /// Synthesize the swap per-policy schema (core + amm/swap, no custom fields)
    /// via the v2 manifest path — exactly what the host composes at runtime.
    fn swap_schema_text() -> String {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": "swap-schema",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "swap" } } }
        }))
        .unwrap();
        crate::schema::compose_per_policy(&manifest).unwrap()
    }

    /// THE GATE: a wrong rename / missing required field / wrong type makes the
    /// strict (schema-`Some`) `Context::from_json_value` ERROR here.
    #[test]
    fn swap_lowering_conforms_to_schema() {
        let (body, meta) = sample_swap_action(50);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();

        assert_eq!(lowered.action_uid, "Amm::Action::\"Swap\"");
        assert_eq!(lowered.principal, format!("Wallet::\"{FROM}\""));
        assert_eq!(lowered.resource, format!("Protocol::\"{TO}\""));

        let schema_text = swap_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        // STRICT context construction — this is the gate.
        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("lowered swap context must conform to Amm::SwapContext");
    }

    /// Widen the gate: the aggregator-route + ExactOutput + Native + OffchainSig
    /// sample must ALSO conform (covers the AggregatorRoute venue branch, the
    /// `TokenKey::Native` branch, the `ExactOutput` direction, and the
    /// `OffchainSig` nature with `lower_eip712` + `nonceKey`).
    #[test]
    fn swap_lowering_aggregator_offchain_conforms_to_schema() {
        let (body, meta) = sample_swap_aggregator_offchain();
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();

        let schema_text = swap_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("aggregator/offchain swap context must conform to Amm::SwapContext");
    }

    /// Prove the full lower → eval path: a `@severity("warn")` forbid on
    /// `slippageBp > 100` passes for slippage 50 and warns for slippage 150.
    #[test]
    fn swap_end_to_end_warn_and_pass() {
        let policy = "@id(\"swap-slippage\")\n@severity(\"warn\")\n\
            forbid(principal, action == Amm::Action::\"Swap\", resource)\n\
            when { context.slippageBp > 100 };\n";
        let schema_text = swap_schema_text();
        let engine =
            crate::policy::PolicyEngine::build_from_per_policy(&[(policy.to_owned(), schema_text)])
                .unwrap();

        // slippage 50 → Pass.
        let (body, meta) = sample_swap_action(50);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &serde_json::json!([]),
                &lowered.context,
            )
            .unwrap();
        assert_eq!(
            verdict,
            crate::policy::Verdict::Pass,
            "slippage 50 must pass"
        );

        // slippage 150 → Warn.
        let (body, meta) = sample_swap_action(150);
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let verdict = engine
            .evaluate(
                &lowered.principal,
                &lowered.action_uid,
                &lowered.resource,
                &serde_json::json!([]),
                &lowered.context,
            )
            .unwrap();
        assert!(
            matches!(verdict, crate::policy::Verdict::Warn(_)),
            "slippage 150 must warn, got {verdict:?}"
        );
    }

    /// `lower_action` on a non-swap body returns `Unsupported`.
    #[test]
    fn non_swap_returns_unsupported() {
        let body = ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        };
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 0,
                gas_limit: U256::from(21_000u64),
                gas_price: LiveField::new(
                    U256::from(1u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "x".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        };

        let err = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap_err();
        assert!(matches!(err, LowerError::Unsupported(_)));
        match err {
            LowerError::Unsupported(label) => assert_eq!(label, "unknown"),
        }
    }

    /// Directly exercise the refactored sub-lowerings that no full-`Action`
    /// sample reaches: the `TokenKey::Erc721`/`Erc1155` arm (merged behind
    /// `is_nft()`), the `Native` arm, and the merged `{chain, pool}` venue group
    /// + `CurveV1` (`nCoins`/`isMeta`).
    #[test]
    fn token_key_and_venue_sub_lowerings_map_correctly() {
        let chain = ChainId::ethereum_mainnet();
        let contract = Address::from_str("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599").unwrap();

        // Native → standard "native", no address/contract/tokenId.
        let native = lower_token_key(&TokenKey::Native {
            chain: chain.clone(),
        });
        assert_eq!(native["standard"], serde_json::json!("native"));
        assert!(native.get("address").is_none());
        assert!(native.get("tokenId").is_none());

        // Erc721 → standard "erc721" via the is_nft() branch, with tokenId hex.
        let nft = lower_token_key(&TokenKey::Erc721 {
            chain: chain.clone(),
            contract,
            token_id: U256::from(255u64),
        });
        assert_eq!(nft["standard"], serde_json::json!("erc721"));
        assert_eq!(nft["tokenId"], serde_json::json!("0xff"));
        assert!(nft.get("address").is_none());

        // Erc1155 → standard "erc1155" (the other half of the merged arm).
        let sft = lower_token_key(&TokenKey::Erc1155 {
            chain: chain.clone(),
            contract,
            token_id: U256::from(1u64),
        });
        assert_eq!(sft["standard"], serde_json::json!("erc1155"));
        assert_eq!(sft["tokenId"], serde_json::json!("0x1"));

        // Merged `{chain, pool}` venue group: SushiV2 emits name "sushi_v2"
        // with only chain + pool (no extra venue fields leak in).
        let sushi = lower_amm_venue(&AmmVenue::SushiV2 {
            chain: chain.clone(),
            pool: contract,
        });
        assert_eq!(sushi["name"], serde_json::json!("sushi_v2"));
        assert_eq!(sushi["pool"], serde_json::json!(format!("{contract:#x}")));
        assert!(sushi.get("factory").is_none());
        assert!(sushi.get("feeTierBp").is_none());

        // CurveV1 carries nCoins (Long) + isMeta (Bool).
        let curve = lower_amm_venue(&AmmVenue::CurveV1 {
            chain,
            pool: contract,
            n_coins: 3,
            is_meta: true,
        });
        assert_eq!(curve["name"], serde_json::json!("curve_v1"));
        assert_eq!(curve["nCoins"], serde_json::json!(3));
        assert_eq!(curve["isMeta"], serde_json::json!(true));
    }
}
