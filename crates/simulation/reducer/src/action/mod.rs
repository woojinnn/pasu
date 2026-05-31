//! `Action` — a single user-signed (or about-to-be-signed) intent. spec: action-design.md
//!
//! `Action` is the primary input to the simulator. The reducer maps
//! `(Action, State) -> StateDelta` to predict outcomes; the policy engine
//! inspects `Action` / `State` / `StateDelta` together to allow or block.
//!
//! This module defines **types only** (Phase 1). The effect function
//! (`run_action`) is implemented separately under `reducers/`.
//!
//! The module layout mirrors the section structure of action-design.md §3–§9.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId, Time, U256};
use simulation_state::{LiveField, NonceKey};

pub mod airdrop;
pub mod amm;
pub mod hyperliquid_core;
pub mod launchpad;
pub mod lending;
pub mod order_intent;
pub mod perp;
pub mod token;
pub mod view;

pub use airdrop::AirdropAction;
pub use amm::AmmAction;
pub use hyperliquid_core::HyperliquidCoreAction;
pub use launchpad::LaunchpadAction;
pub use lending::LendingAction;
pub use order_intent::OrderIntent;
pub use perp::PerpAction;
pub use token::TokenAction;
pub use view::ActionView;

// ---------------------------------------------------------------------------
// Common helper types
// ---------------------------------------------------------------------------

/// Arbitrary binary payload (calldata, signature, etc.) serialized as a
/// 0x-prefixed hex string. Matches the `B256` convention in the state crate —
/// kept as a `String` alias in Phase 1.
pub type Bytes = String;

/// `EIP-712` domain separator info. Carried by `OffchainSig` natures to
/// support signature verification, replay checks, and audit reproduction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Eip712Domain {
    /// Domain name (e.g. `"UniswapX"`, `"Permit2"`).
    pub name: String,
    /// Optional domain version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub version: Option<String>,
    /// `EIP-155` chain id the domain is bound to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub chain_id: Option<u64>,
    /// Address of the contract that will verify the signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub verifying_contract: Option<Address>,
    /// `EIP-712` salt field — rarely used but part of the spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub salt: Option<Bytes>,
}

// ---------------------------------------------------------------------------
// Top-level Action (§3)
// ---------------------------------------------------------------------------

/// A single user-signed (or about-to-be-signed) intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Action {
    /// When / who / how the action was submitted.
    pub meta: ActionMeta,
    /// Domain-specific body (token / AMM / lending / ...).
    pub body: ActionBody,
}

/// Submission metadata attached to every `Action`: timing, submitter, and
/// the submission "nature" (`OnchainTx` vs `OffchainSig`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ActionMeta {
    /// Wall-clock time at which the `Action` is submitted to the simulator.
    pub submitted_at: Time,
    /// Submitter `Address` — usually the wallet owner.
    #[tsify(type = "string")]
    pub submitter: Address,
    /// Submission shape: on-chain transaction or off-chain signature.
    pub nature: ActionNature,
}

/// How the `Action` enters the system — either a broadcast transaction or
/// an `EIP-712` signature awaiting a matcher / resolver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionNature {
    /// Transaction signing -> broadcast -> immediate on-chain attempt.
    OnchainTx {
        /// Chain on which the transaction is being sent.
        chain: ChainId,
        /// Sequential transaction nonce of `submitter`.
        nonce: u64,
        /// Gas limit declared by the transaction.
        #[tsify(type = "string")]
        gas_limit: U256,
        /// Gas price as a `LiveField`. Under `EIP-1559` both `maxFee` and
        /// `maxPriority` may be needed; Phase 1 keeps a single value and
        /// can be extended into a fee struct in a follow-up spec.
        #[tsify(type = "LiveField<string>")]
        gas_price: LiveField<U256>,
        /// Native value attached to the call (`msg.value`).
        #[tsify(type = "string")]
        value: U256,
    },

    /// `EIP-712` signature only. Not realized until a matcher / resolver
    /// picks it up.
    OffchainSig {
        /// `EIP-712` domain separator for the signed payload.
        domain: Eip712Domain,
        /// Signature validity deadline.
        deadline: Time,
        /// Optional collision / replay key (e.g. `Permit2` nonce).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        nonce_key: Option<NonceKey>,
    },
}

/// Domain-specific body plus cross-cutting variants (`Multicall`, `Unknown`).
#[allow(clippy::large_enum_variant)]
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum ActionBody {
    /// Token-domain action (transfer, approve, permit, ...).
    Token(TokenAction),
    /// AMM-domain action (swap, add/remove liquidity, intent order, ...).
    Amm(AmmAction),
    /// Lending-domain action (supply, borrow, repay, liquidation, ...).
    Lending(LendingAction),
    /// Airdrop-domain action (claim, delegate, ...).
    Airdrop(AirdropAction),
    /// Launchpad-domain action (commit, claim, refund, ...).
    Launchpad(LaunchpadAction),
    /// Perp-domain action (open/close position, funding, ...).
    Perp(PerpAction),
    /// Hyperliquid CORE action (off-chain L1 order / leverage / fund movement),
    /// intercepted from a `/exchange` POST rather than `window.ethereum`.
    HyperliquidCore(HyperliquidCoreAction),

    /// Batched multi-call (e.g. `Uniswap Universal Router`, `Aave`).
    Multicall {
        /// Inner `ActionBody` entries executed atomically as a batch.
        /// `Vec<Self>` recurses; the explicit type override emits `ActionBody[]`
        /// in `.d.ts` (tsify defaults to `Self[]`, which is invalid in TS).
        #[tsify(type = "ActionBody[]")]
        actions: Vec<Self>,
    },

    /// Unidentified call. Policy default: warn / deny.
    Unknown {
        /// Destination contract `Address`.
        #[tsify(type = "string")]
        target: Address,
        /// Chain on which the call is being made.
        chain: ChainId,
        /// Raw call data (hex-encoded).
        #[tsify(type = "string")]
        calldata: Bytes,
        /// Native value attached to the call (`msg.value`).
        #[tsify(type = "string")]
        value: U256,
    },
}

// ===========================================================================
// Smoke tests — verify the `Action` type tree compiles, serializes, and
// deserializes as one piece. Before Phase 2 introduces real effect functions
// we only check *shape stability*.
// ===========================================================================

#[cfg(test)]
mod smoke {
    use super::*;

    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{ChainId, Duration, MarketRef, VenueRef};
    use simulation_state::token::TokenKey;
    use simulation_state::LiveField;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn token_usdc() -> token::TokenAction {
        let token = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        token::TokenAction::Erc20Approve(token::Erc20ApproveAction {
            token,
            spender: Address::from_str("0x00000000000000000000000000000000DeaDBeef").unwrap(),
            amount: U256::from(1_000_000_000u64),
        })
    }

    #[test]
    fn token_approve_round_trip() {
        let action = Action {
            meta: ActionMeta {
                submitted_at: now(),
                submitter: user(),
                nature: ActionNature::OnchainTx {
                    chain: ChainId::ethereum_mainnet(),
                    nonce: 42,
                    gas_limit: U256::from(60_000u64),
                    gas_price: LiveField::new(
                        U256::from(1_000_000_000u64),
                        DataSource::OracleFeed {
                            provider: OracleProvider::Chainlink,
                            feed_id: "ETH/USD".into(),
                        },
                        now(),
                    )
                    .with_ttl(Duration::from_secs(12)),
                    value: U256::ZERO,
                },
            },
            body: ActionBody::Token(token_usdc()),
        };

        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn unknown_call_round_trip() {
        let action = ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: ActionBody = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn intent_order_offchain_sig_round_trip() {
        let sell = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        let buy = simulation_state::token::TokenRef {
            key: TokenKey::Native {
                chain: ChainId::ethereum_mainnet(),
            },
        };

        let intent = amm::AmmAction::SignIntentOrder(amm::SignIntentOrderAction {
            venue: amm::IntentVenue::UniswapX {
                chain: ChainId::ethereum_mainnet(),
                reactor: Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be").unwrap(),
            },
            sell,
            buy,
            sell_amount: U256::from(1_000_000_000u64),
            buy_min: U256::from(300_000_000_000_000_000u64),
            order_kind: amm::IntentOrderKind::Dutch,
            recipient: user(),
            valid_until: Time::from_unix(1_738_001_800),
            live_inputs: amm::SignIntentOrderLiveInputs {
                expected_fill_price: LiveField::new(
                    simulation_state::primitives::Price::new("3720.0"),
                    DataSource::VenueApi {
                        endpoint: "https://api.uniswap.org/v2/quote".into(),
                        parser_id: "uniswapx_quote".into(),
                        auth: None,
                    },
                    now(),
                ),
                competing_orders: LiveField::new(
                    3u32,
                    DataSource::VenueApi {
                        endpoint: "https://api.uniswap.org/v2/orders".into(),
                        parser_id: "uniswapx_active_orders".into(),
                        auth: None,
                    },
                    now(),
                ),
            },
        });

        let action = Action {
            meta: ActionMeta {
                submitted_at: now(),
                submitter: user(),
                nature: ActionNature::OffchainSig {
                    domain: Eip712Domain {
                        name: "UniswapX".into(),
                        version: Some("2".into()),
                        chain_id: Some(1),
                        verifying_contract: Some(
                            Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be")
                                .unwrap(),
                        ),
                        salt: None,
                    },
                    deadline: Time::from_unix(1_738_001_800),
                    nonce_key: Some(simulation_state::NonceKey::OrderHash {
                        hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                            .into(),
                    }),
                },
            },
            body: ActionBody::Amm(intent),
        };

        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);

        // Ensure VenueRef/MarketRef compile-time path also works:
        let _hl = MarketRef {
            symbol: "ETH-USD".into(),
            venue: VenueRef::new("hyperliquid"),
        };
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn split_aggregator_swap_route_round_trip() {
        let chain = ChainId::ethereum_mainnet();
        let usdc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        let wbtc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599").unwrap(),
            },
        };
        let weth = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        };

        let v3_05 = amm::AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let v3_30 = amm::AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0xcbcdf9626bc03e24f779434178a73a0b4bad62ed").unwrap(),
            fee_tier_bp: 3000,
        };
        let curve = amm::AmmVenue::CurveV2 {
            chain: chain.clone(),
            pool: Address::from_str("0xd51a44d3fae010294c616388b506acda1bfaae46").unwrap(),
        };

        let pool_state_concentrated = amm::PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: simulation_state::primitives::U128::from(0u64),
            ticks: vec![],
        };
        let pool_state_cryptoswap = amm::PoolState::Cryptoswap {
            balances: vec![U256::from(0u64); 3],
            price_scale: vec![U256::from(0u64); 2],
            a_gamma: U256::from(0u64),
            fee_bp: 4,
        };

        let route = amm::SwapRoute {
            paths: vec![
                amm::RoutePath {
                    share_bp: 6000,
                    hops: vec![
                        amm::RouteHop {
                            token_in: usdc.clone(),
                            token_out: weth.clone(),
                            venue: v3_05,
                            pool_state: pool_state_concentrated.clone(),
                            effective_fee_bp: 5,
                            estimated_out: U256::from(7_900_000_000_000_000_000u64),
                        },
                        amm::RouteHop {
                            token_in: weth,
                            token_out: wbtc.clone(),
                            venue: v3_30,
                            pool_state: pool_state_concentrated,
                            effective_fee_bp: 30,
                            estimated_out: U256::from(45_200_000u64),
                        },
                    ],
                    estimated_out: U256::from(45_200_000u64),
                },
                amm::RoutePath {
                    share_bp: 4000,
                    hops: vec![amm::RouteHop {
                        token_in: usdc.clone(),
                        token_out: wbtc.clone(),
                        venue: curve,
                        pool_state: pool_state_cryptoswap,
                        effective_fee_bp: 4,
                        estimated_out: U256::from(29_800_000u64),
                    }],
                    estimated_out: U256::from(29_800_000u64),
                },
            ],
            aggregator: Some(amm::AggregatorMeta {
                aggregator: amm::AggregatorKind::OneInchV6,
                router: Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap(),
                executor: Some(
                    Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap(),
                ),
                raw_calldata_hash:
                    "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
                permit_bundled: false,
                referrer: None,
                referrer_fee_bp: 0,
            }),
        };

        let swap = amm::AmmAction::Swap(amm::SwapAction {
            venue: amm::AmmVenue::AggregatorRoute {
                chain,
                router: Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap(),
                route_hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                    .into(),
            },
            params: amm::SwapParams {
                token_in: usdc,
                token_out: wbtc,
                direction: amm::SwapDirection::ExactInput {
                    amount_in: U256::from(50_000_000_000u64),
                    min_amount_out: U256::from(74_000_000u64),
                },
                recipient: user(),
                slippage_bp: 100,
            },
            live_inputs: amm::SwapLiveInputs {
                route: LiveField::new(
                    route,
                    DataSource::VenueApi {
                        endpoint: "https://api.1inch.dev/swap/v6.0/1/swap".into(),
                        parser_id: "oneinch_v6_route".into(),
                        auth: None,
                    },
                    now(),
                ),
                expected_amount_out: LiveField::new(
                    U256::from(75_000_000u64),
                    DataSource::VenueApi {
                        endpoint: "https://api.1inch.dev/swap/v6.0/1/quote".into(),
                        parser_id: "oneinch_v6_quote".into(),
                        auth: None,
                    },
                    now(),
                ),
                price_impact_bp: LiveField::new(
                    24u32,
                    DataSource::VenueApi {
                        endpoint: "https://api.1inch.dev/swap/v6.0/1/quote".into(),
                        parser_id: "oneinch_v6_quote".into(),
                        auth: None,
                    },
                    now(),
                ),
                gas_estimate: LiveField::new(
                    U256::from(280_000u64),
                    DataSource::VenueApi {
                        endpoint: "https://api.1inch.dev/swap/v6.0/1/quote".into(),
                        parser_id: "oneinch_v6_quote".into(),
                        auth: None,
                    },
                    now(),
                ),
            },
        });

        let json = serde_json::to_string(&swap).unwrap();
        let back: amm::AmmAction = serde_json::from_str(&json).unwrap();
        assert_eq!(swap, back);
    }

    // PDF §11 fixture #5: Uniswap V3 USDC → WETH single hop on Arbitrum.
    // OnchainTx, single path, single hop, no aggregator. Live route source = pool slot0.
    #[allow(clippy::too_many_lines)]
    #[test]
    fn uniswap_v3_arbitrum_single_hop_round_trip() {
        let chain = ChainId::arbitrum();
        // Arbitrum native USDC (Circle official).
        let usdc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xaf88d065e77c8cc2239327c5edb3a432268e5831").unwrap(),
            },
        };
        // Arbitrum WETH (canonical bridge wrapper).
        let weth = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x82af49447d8a07e3bd95bd0d56f35241523fbab1").unwrap(),
            },
        };
        // Placeholder pool address — PDF §11 uses `0xPoolUsdcEth_005`.
        let pool = Address::from_str("0xc6962004f452be9203591991d15f6b388e09e8d0").unwrap();

        let v3 = amm::AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool,
            fee_tier_bp: 500,
        };

        let pool_state = amm::PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: simulation_state::primitives::U128::from(0u64),
            ticks: vec![],
        };

        let pool_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "slot0()".into(),
            decoder_id: "uniswap_v3_slot0".into(),
        };

        let route = amm::SwapRoute {
            paths: vec![amm::RoutePath {
                share_bp: 10000,
                hops: vec![amm::RouteHop {
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

        let swap = amm::AmmAction::Swap(amm::SwapAction {
            venue: v3,
            params: amm::SwapParams {
                token_in: usdc,
                token_out: weth,
                direction: amm::SwapDirection::ExactInput {
                    amount_in: U256::from(1_000_000_000u64),
                    min_amount_out: U256::from(300_000_000_000_000_000u64),
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: amm::SwapLiveInputs {
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

        let action = Action {
            meta: ActionMeta {
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
            },
            body: ActionBody::Amm(swap),
        };

        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    // PDF §11 fixture #6: Aave V3 borrow USDC on Optimism. OnchainTx, Variable rate mode,
    // health_factor 2.4 from `getUserAccountData`.
    #[allow(clippy::too_many_lines)]
    #[test]
    fn aave_v3_borrow_optimism_round_trip() {
        let chain = ChainId::new("eip155:10");
        // Optimism native USDC (Circle official).
        let usdc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x0b2c639c533813f4aa9d7837caf62653d097ff85").unwrap(),
            },
        };
        // Aave V3 Pool — placeholder (PDF §11 uses `0xAavePool`).
        let pool = Address::from_str("0x794a61358d6845594f94dc1db02a252b5b4814ad").unwrap();

        let reserve_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "getReserveData(address)".into(),
            decoder_id: "aave_v3_reserve_data".into(),
        };
        let user_source = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "getUserAccountData(address)".into(),
            decoder_id: "aave_v3_user_account_data".into(),
        };

        let borrow = lending::LendingAction::Borrow(lending::BorrowAction {
            venue: lending::LendingVenue::AaveV3 {
                chain: chain.clone(),
                pool,
                market_id: None,
            },
            asset: usdc,
            amount: U256::from(500_000_000u64),
            rate_mode: simulation_state::token::RateMode::Variable,
            on_behalf_of: None,
            live_inputs: lending::BorrowLiveInputs {
                reserve_state: LiveField::new(
                    lending::ReserveState {
                        total_supply: U256::from(50_000_000_000_000u64),
                        total_borrow: U256::from(30_000_000_000_000u64),
                        utilization_bp: 6000,
                        supply_cap: None,
                        borrow_cap: None,
                        ltv_bp: 7500,
                        liquidation_threshold_bp: 8500,
                        liquidation_bonus_bp: 500,
                        reserve_factor_bp: 1000,
                        is_frozen: false,
                        is_paused: false,
                    },
                    reserve_source.clone(),
                    now(),
                ),
                user_state_before: LiveField::new(
                    lending::UserLendingState {
                        health_factor: simulation_state::primitives::Decimal::new("2.4"),
                        total_collat_usd: U256::from(10_000u64),
                        total_debt_usd: U256::from(4_000u64),
                        available_borrow_usd: U256::from(3_500u64),
                    },
                    user_source,
                    now(),
                ),
                asset_price_usd: LiveField::new(
                    simulation_state::primitives::Decimal::new("1.0"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    now(),
                ),
                current_borrow_rate: LiveField::new(
                    simulation_state::primitives::Decimal::new("0.045"),
                    reserve_source.clone(),
                    now(),
                ),
                available_liquidity: LiveField::new(
                    U256::from(12_000_000_000_000u64),
                    reserve_source,
                    now(),
                ),
            },
        });

        let action = Action {
            meta: ActionMeta {
                submitted_at: now(),
                submitter: user(),
                nature: ActionNature::OnchainTx {
                    chain,
                    nonce: 13,
                    gas_limit: U256::from(350_000u64),
                    gas_price: LiveField::new(
                        U256::from(1_000_000u64),
                        DataSource::OracleFeed {
                            provider: OracleProvider::Pyth,
                            feed_id: "gas/optimism".into(),
                        },
                        now(),
                    ),
                    value: U256::ZERO,
                },
            },
            body: ActionBody::Lending(borrow),
        };

        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    // PDF §11 fixture #7: Hyperliquid open long ETH-USD 5x. OffchainSig (Hyperliquid is an
    // off-chain orderbook), domain.name = "Hyperliquid", chain identifier = "hl-mainnet"
    // (non-EIP-155 CAIP-2).
    #[test]
    fn hyperliquid_open_long_eth_5x_round_trip() {
        let chain = ChainId::new("hl-mainnet");
        // Placeholder collateral USDC — Hyperliquid USDC bridge token, PDF §11 doesn't pin
        // an address. Real Hyperliquid USDC bridges via Arbitrum; left placeholder here
        // because the fixture verifies serde round-trip, not on-chain identity.
        let usdc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0x000000000000000000000000000000000000beef").unwrap(),
            },
        };

        let market = MarketRef {
            symbol: "ETH-USD".into(),
            venue: VenueRef::new("hyperliquid"),
        };

        let venue_source = |parser_id: &str| DataSource::VenueApi {
            endpoint: "https://api.hyperliquid.xyz/info".into(),
            parser_id: parser_id.into(),
            auth: None,
        };

        let open = perp::PerpAction::OpenPosition(perp::OpenPerpAction {
            venue: perp::PerpVenue::Hyperliquid { chain },
            market,
            side: simulation_state::position::PerpSide::Long,
            size: perp::SizeSpec::BaseAmount {
                amount: U256::from(2_000_000_000_000_000_000u64),
            },
            leverage: simulation_state::primitives::Decimal::new("5.0"),
            collateral: (usdc, U256::from(1_500_000_000u64)),
            margin_mode: simulation_state::position::MarginMode::Cross,
            slippage_bp: 30,
            reduce_only: false,
            live_inputs: perp::OpenPerpLiveInputs {
                mark_price: LiveField::new(
                    simulation_state::primitives::Price::new("3750.0"),
                    venue_source("hl_mids"),
                    now(),
                ),
                oracle_price: LiveField::new(
                    simulation_state::primitives::Price::new("3751.2"),
                    venue_source("hl_oracle"),
                    now(),
                ),
                funding_rate: LiveField::new(
                    simulation_state::primitives::Decimal::new("0.0001"),
                    venue_source("hl_funding"),
                    now(),
                ),
                available_oi: LiveField::new(
                    U256::from(1_000_000_000_000_000u64),
                    venue_source("hl_oi"),
                    now(),
                ),
                max_leverage: LiveField::new(
                    simulation_state::primitives::Decimal::new("20.0"),
                    venue_source("hl_market_meta"),
                    now(),
                ),
                initial_margin_bp: LiveField::new(500u32, venue_source("hl_market_meta"), now()),
                maintenance_bp: LiveField::new(250u32, venue_source("hl_market_meta"), now()),
                fee_taker_bp: LiveField::new(5u32, venue_source("hl_fees"), now()),
                fee_maker_bp: LiveField::new(1u32, venue_source("hl_fees"), now()),
                user_account_state: LiveField::new(
                    perp::PerpAccountState {
                        total_collateral_usd: U256::from(1_500u64),
                        used_margin_usd: U256::ZERO,
                        free_margin_usd: U256::from(1_500u64),
                        open_positions: vec![],
                    },
                    venue_source("hl_account"),
                    now(),
                ),
            },
        });

        let action = Action {
            meta: ActionMeta {
                submitted_at: now(),
                submitter: user(),
                nature: ActionNature::OffchainSig {
                    domain: Eip712Domain {
                        name: "Hyperliquid".into(),
                        version: Some("1".into()),
                        chain_id: None,
                        verifying_contract: None,
                        salt: None,
                    },
                    deadline: Time::from_unix(1_738_000_060),
                    nonce_key: Some(simulation_state::NonceKey::OrderHash {
                        hash: "0xfeed00000000000000000000000000000000000000000000000000000000ffff"
                            .into(),
                    }),
                },
            },
            body: ActionBody::Perp(open),
        };

        let json = serde_json::to_string(&action).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }
}
