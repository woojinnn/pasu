//! `ActionView` — borrow-only projection of `ActionBody` for policy triggers.

use super::ActionBody;

/// The fields a policy trigger may match on, projected from an `ActionBody`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionView<'a> {
    /// `ActionBody` domain tag (token/amm/.../multicall/unknown).
    pub domain: &'a str,
    /// Inner action tag (e.g. `"swap"`); `None` for multicall/unknown.
    pub action_tag: Option<&'a str>,
    /// Venue name (e.g. `"uniswap_v3"`); `None` when the action has no venue.
    pub venue_name: Option<&'a str>,
}

impl ActionBody {
    /// Project the trigger-relevant fields. Cheap, borrow-only.
    ///
    /// The returned `domain` / `action_tag` / `venue_name` strings are exactly
    /// the `serde` discriminants the corresponding JSON would carry, so a policy
    /// trigger can match them against raw field values without re-serializing.
    #[must_use]
    pub const fn view(&self) -> ActionView<'_> {
        match self {
            Self::Token(a) => ActionView {
                domain: "token",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Amm(a) => ActionView {
                domain: "amm",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Lending(a) => ActionView {
                domain: "lending",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Airdrop(a) => ActionView {
                domain: "airdrop",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Launchpad(a) => ActionView {
                domain: "launchpad",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Perp(a) => ActionView {
                domain: "perp",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::HyperliquidCore(a) => ActionView {
                domain: "hyperliquid_core",
                action_tag: Some(a.action_tag()),
                venue_name: a.venue_name(),
            },
            Self::Multicall { .. } => ActionView {
                domain: "multicall",
                action_tag: None,
                venue_name: None,
            },
            Self::Unknown { .. } => ActionView {
                domain: "unknown",
                action_tag: None,
                venue_name: None,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    use super::*;
    use crate::{amm, lending, perp, token, AirdropAction, LaunchpadAction};

    use simulation_state::primitives::{Address, ChainId, U256};
    use simulation_state::token::TokenKey;
    use std::str::FromStr;

    fn addr(hex: &str) -> Address {
        Address::from_str(hex).unwrap()
    }

    fn token_ref() -> simulation_state::token::TokenRef {
        simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: addr("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            },
        }
    }

    fn token_approve() -> token::TokenAction {
        token::TokenAction::Erc20Approve(token::Erc20ApproveAction {
            token: token_ref(),
            spender: addr("0x00000000000000000000000000000000DeaDBeef"),
            amount: U256::from(1_000_000_000u64),
        })
    }

    // ---- High-level `view()` shape tests -----------------------------------

    #[test]
    fn view_amm_swap_uniswap_v3() {
        let chain = ChainId::ethereum_mainnet();
        let usdc = token_ref();
        let weth = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: addr("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            },
        };
        let v3 = amm::AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: addr("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
            fee_tier_bp: 500,
        };
        let pool_source = simulation_state::live_field::DataSource::OnchainView {
            chain,
            contract: addr("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
            function: "slot0()".into(),
            decoder_id: "uniswap_v3_slot0".into(),
        };
        let now = simulation_state::primitives::Time::from_unix(1_738_000_000);
        let route = amm::SwapRoute {
            paths: vec![],
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
                recipient: addr("0x000000000000000000000000000000000000a01c"),
                slippage_bp: 50,
            },
            live_inputs: amm::SwapLiveInputs {
                route: simulation_state::LiveField::new(route, pool_source.clone(), now),
                expected_amount_out: simulation_state::LiveField::new(
                    U256::from(305_000_000_000_000_000u64),
                    pool_source.clone(),
                    now,
                ),
                price_impact_bp: simulation_state::LiveField::new(12u32, pool_source.clone(), now),
                gas_estimate: simulation_state::LiveField::new(
                    U256::from(180_000u64),
                    pool_source,
                    now,
                ),
            },
        });
        let body = ActionBody::Amm(swap);
        let view = body.view();
        assert_eq!(view.domain, "amm");
        assert_eq!(view.action_tag, Some("swap"));
        assert_eq!(view.venue_name, Some("uniswap_v3"));
        // serde is the source of truth for the action tag too.
        assert_action_tag(&body);
    }

    #[test]
    fn view_token_approve_has_no_venue() {
        let body = ActionBody::Token(token_approve());
        let view = body.view();
        assert_eq!(view.domain, "token");
        assert_eq!(view.action_tag, Some("erc20_approve"));
        assert_eq!(view.venue_name, None);
    }

    #[test]
    fn view_unknown() {
        let body = ActionBody::Unknown {
            target: addr("0xfeed000000000000000000000000000000000001"),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        };
        let view = body.view();
        assert_eq!(view.domain, "unknown");
        assert_eq!(view.action_tag, None);
        assert_eq!(view.venue_name, None);
    }

    #[test]
    fn view_lending_borrow_aave_v3() {
        let chain = ChainId::new("eip155:10");
        let pool = addr("0x794a61358d6845594f94dc1db02a252b5b4814ad");
        let usdc = simulation_state::token::TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: addr("0x0b2c639c533813f4aa9d7837caf62653d097ff85"),
            },
        };
        let reserve_source = simulation_state::live_field::DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "getReserveData(address)".into(),
            decoder_id: "aave_v3_reserve_data".into(),
        };
        let user_source = simulation_state::live_field::DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "getUserAccountData(address)".into(),
            decoder_id: "aave_v3_user_account_data".into(),
        };
        let now = simulation_state::primitives::Time::from_unix(1_738_000_000);
        let borrow = lending::LendingAction::Borrow(lending::BorrowAction {
            venue: lending::LendingVenue::AaveV3 {
                chain,
                pool,
                market_id: None,
            },
            asset: usdc,
            amount: U256::from(500_000_000u64),
            rate_mode: simulation_state::token::RateMode::Variable,
            on_behalf_of: None,
            live_inputs: lending::BorrowLiveInputs {
                reserve_state: simulation_state::LiveField::new(
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
                    now,
                ),
                user_state_before: simulation_state::LiveField::new(
                    lending::UserLendingState {
                        health_factor: simulation_state::primitives::Decimal::new("2.4"),
                        total_collat_usd: U256::from(10_000u64),
                        total_debt_usd: U256::from(4_000u64),
                        available_borrow_usd: U256::from(3_500u64),
                    },
                    user_source,
                    now,
                ),
                asset_price_usd: simulation_state::LiveField::new(
                    simulation_state::primitives::Decimal::new("1.0"),
                    reserve_source.clone(),
                    now,
                ),
                current_borrow_rate: simulation_state::LiveField::new(
                    simulation_state::primitives::Decimal::new("0.045"),
                    reserve_source.clone(),
                    now,
                ),
                available_liquidity: simulation_state::LiveField::new(
                    U256::from(12_000_000_000_000u64),
                    reserve_source,
                    now,
                ),
            },
        });
        let body = ActionBody::Lending(borrow);
        let view = body.view();
        assert_eq!(view.domain, "lending");
        assert_eq!(view.action_tag, Some("borrow"));
        assert_eq!(view.venue_name, Some("aave_v3"));
        // serde is the source of truth for the action tag too.
        assert_action_tag(&body);
    }

    // ---- AUTHORITATIVE: accessor strings must equal serde output -----------

    /// Read the `["name"]` tag that serde emits for a venue value.
    fn serde_name<V: serde::Serialize>(venue: &V) -> String {
        serde_json::to_value(venue)
            .unwrap()
            .get("name")
            .and_then(serde_json::Value::as_str)
            .expect("venue serializes with a `name` tag")
            .to_owned()
    }

    /// Assert, for one venue value, that its accessor `name()` AND the expected
    /// literal both equal what serde emits as the `["name"]` tag. serde is the
    /// source of truth: comparing the accessor and a literal pins the hardcoded
    /// string to serde with no room for the accessor arm to drift.
    macro_rules! assert_venue {
        ($venue:expr, $expected:literal $(,)?) => {{
            let v = $venue;
            let serde_tag = serde_name(&v);
            assert_eq!(v.name(), serde_tag, "accessor name() must equal serde tag");
            assert_eq!(
                serde_tag, $expected,
                "expected literal must equal serde tag"
            );
        }};
    }

    /// Assert that an `ActionBody`'s inner action tag (the `["action"]` field
    /// serde emits) equals the accessor `action_tag()`.
    fn assert_action_tag(body: &ActionBody) {
        let json = serde_json::to_value(body).unwrap();
        let tag = json
            .get("action")
            .and_then(serde_json::Value::as_str)
            .expect("domain action serializes with an `action` tag");
        assert_eq!(
            Some(tag),
            body.view().action_tag,
            "serde `action` tag must equal accessor `action_tag()`"
        );
    }

    #[test]
    fn accessor_strings_match_serde() {
        let eth = ChainId::ethereum_mainnet();
        let a = |hex: &str| addr(hex);
        let any = a("0x000000000000000000000000000000000000beef");

        // ---- AmmVenue: all 11 variants ----
        assert_venue!(
            amm::AmmVenue::UniswapV2 {
                chain: eth.clone(),
                pool: any,
                factory: any,
            },
            "uniswap_v2",
        );
        assert_venue!(
            amm::AmmVenue::UniswapV3 {
                chain: eth.clone(),
                pool: any,
                fee_tier_bp: 500,
            },
            "uniswap_v3",
        );
        assert_venue!(
            amm::AmmVenue::UniswapV4 {
                chain: eth.clone(),
                pool_id: "0x00".into(),
                pool_manager: any,
                hooks: any,
            },
            "uniswap_v4",
        );
        assert_venue!(
            amm::AmmVenue::SushiV2 {
                chain: eth.clone(),
                pool: any,
            },
            "sushi_v2",
        );
        assert_venue!(
            amm::AmmVenue::CurveV1 {
                chain: eth.clone(),
                pool: any,
                n_coins: 3,
                is_meta: false,
            },
            "curve_v1",
        );
        assert_venue!(
            amm::AmmVenue::CurveV2 {
                chain: eth.clone(),
                pool: any,
            },
            "curve_v2",
        );
        assert_venue!(
            amm::AmmVenue::BalancerV2 {
                chain: eth.clone(),
                vault: any,
                pool_id: "0x00".into(),
                pool_type: amm::BalancerPoolType::Weighted,
            },
            "balancer_v2",
        );
        assert_venue!(
            amm::AmmVenue::BalancerV3 {
                chain: eth.clone(),
                pool_id: "0x00".into(),
                pool_type: amm::BalancerPoolType::Stable,
            },
            "balancer_v3",
        );
        assert_venue!(
            amm::AmmVenue::TraderJoeLB {
                chain: eth.clone(),
                pair: any,
                bin_step: 10,
            },
            "trader_joe_l_b",
        );
        assert_venue!(
            amm::AmmVenue::MaverickV2 {
                chain: eth.clone(),
                pool: any,
            },
            "maverick_v2",
        );
        assert_venue!(
            amm::AmmVenue::AggregatorRoute {
                chain: eth.clone(),
                router: any,
                route_hash: "0x00".into(),
            },
            "aggregator_route",
        );

        // ---- IntentVenue: all 4 variants ----
        assert_venue!(
            amm::IntentVenue::UniswapX {
                chain: eth.clone(),
                reactor: any,
            },
            "uniswap_x",
        );
        assert_venue!(
            amm::IntentVenue::CowSwap {
                chain: eth.clone(),
                settlement: any,
            },
            "cow_swap",
        );
        assert_venue!(
            amm::IntentVenue::OneInchFusion { chain: eth.clone() },
            "one_inch_fusion",
        );
        assert_venue!(amm::IntentVenue::Bebop { chain: eth.clone() }, "bebop");

        // ---- LendingVenue: all 8 variants ----
        assert_venue!(
            lending::LendingVenue::AaveV3 {
                chain: eth.clone(),
                pool: any,
                market_id: None,
            },
            "aave_v3",
        );
        assert_venue!(
            lending::LendingVenue::AaveV2 {
                chain: eth.clone(),
                pool: any,
            },
            "aave_v2",
        );
        assert_venue!(
            lending::LendingVenue::CompoundV3 {
                chain: eth.clone(),
                comet: any,
                base_asset: token_ref(),
            },
            "compound_v3",
        );
        assert_venue!(
            lending::LendingVenue::CompoundV2 {
                chain: eth.clone(),
                comptroller: any,
            },
            "compound_v2",
        );
        assert_venue!(
            lending::LendingVenue::MorphoBlue {
                chain: eth.clone(),
                market_id: "0x00".into(),
            },
            "morpho_blue",
        );
        assert_venue!(
            lending::LendingVenue::MorphoOptimizer {
                chain: eth.clone(),
                vault: any,
            },
            "morpho_optimizer",
        );
        assert_venue!(
            lending::LendingVenue::Spark {
                chain: eth.clone(),
                pool: any,
            },
            "spark",
        );
        assert_venue!(
            lending::LendingVenue::Fluid {
                chain: eth.clone(),
                vault: any,
            },
            "fluid",
        );

        // ---- PerpVenue: all 9 variants ----
        assert_venue!(
            perp::PerpVenue::Hyperliquid { chain: eth.clone() },
            "hyperliquid",
        );
        assert_venue!(perp::PerpVenue::GmxV2 { chain: eth.clone() }, "gmx_v2");
        assert_venue!(perp::PerpVenue::DyDxV4 { chain: eth.clone() }, "dy_dx_v4");
        assert_venue!(perp::PerpVenue::Vertex { chain: eth.clone() }, "vertex");
        assert_venue!(perp::PerpVenue::Aevo { chain: eth.clone() }, "aevo");
        assert_venue!(perp::PerpVenue::Drift { chain: eth.clone() }, "drift");
        assert_venue!(
            perp::PerpVenue::JupiterPerps { chain: eth.clone() },
            "jupiter_perps",
        );
        assert_venue!(
            perp::PerpVenue::Synthetix { chain: eth.clone() },
            "synthetix",
        );
        assert_venue!(
            perp::PerpVenue::Generic {
                chain: eth.clone(),
                contract: any,
            },
            "generic",
        );

        // ---- Domain action tags via serde `["action"]` ----
        // Token (no venue) — covers the consecutive-capital gotchas.
        assert_action_tag(&ActionBody::Token(token_approve()));
        assert_action_tag(&ActionBody::Token(
            token::TokenAction::NftSetApprovalForAll(token::NftSetForAllAction {
                chain: eth.clone(),
                contract: any,
                spender: any,
                approved: true,
            }),
        ));

        // Lending `SetEMode` — serde emits `set_e_mode`, NOT `set_emode`.
        let ds = simulation_state::live_field::DataSource::OnchainView {
            chain: eth.clone(),
            contract: any,
            function: "getUserEMode(address)".into(),
            decoder_id: "aave_v3_user_emode".into(),
        };
        let now = simulation_state::primitives::Time::from_unix(1_738_000_000);
        let set_emode =
            ActionBody::Lending(lending::LendingAction::SetEMode(lending::SetEModeAction {
                venue: lending::LendingVenue::AaveV3 {
                    chain: eth,
                    pool: any,
                    market_id: None,
                },
                category_id: 1,
                live_inputs: lending::SetEModeLiveInputs {
                    category_config: simulation_state::LiveField::new(
                        lending::EModeConfig {
                            ltv_bp: 9000,
                            liquidation_threshold_bp: 9300,
                            liquidation_bonus_bp: 100,
                            price_source: None,
                            assets_in_category: vec![],
                            category: None,
                        },
                        ds.clone(),
                        now,
                    ),
                    user_state_before: simulation_state::LiveField::new(
                        lending::UserLendingState {
                            health_factor: simulation_state::primitives::Decimal::new("2.0"),
                            total_collat_usd: U256::from(10u64),
                            total_debt_usd: U256::from(1u64),
                            available_borrow_usd: U256::from(5u64),
                        },
                        ds,
                        now,
                    ),
                },
            }));
        let set_emode_json = serde_json::to_value(&set_emode).unwrap();
        assert_eq!(
            set_emode_json
                .get("action")
                .and_then(serde_json::Value::as_str),
            Some("set_e_mode"),
            "serde must emit set_e_mode (underscore before E and M)"
        );
        assert_action_tag(&set_emode);

        // Airdrop / Launchpad tags are single-boundary; the accessor is trivially
        // correct, but reference the methods so the imports stay honest.
        let _ = AirdropAction::action_tag;
        let _ = LaunchpadAction::action_tag;
    }
}
