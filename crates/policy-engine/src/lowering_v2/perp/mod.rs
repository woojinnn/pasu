//! Perp-domain lowering: per-action dispatch + the shared `PerpVenue` /
//! `MarketRef` / `SizeSpec` / `PerpAccountState` / enum lowerings reused by the
//! per-action leaves.

use serde_json::{Map, Value};

use policy_state::position::{MarginMode, PerpSide};
use policy_state::primitives::{MarketRef, VenueRef};
use policy_transition::action::perp::{
    PerpAccountState, PerpAction, PerpVenue, SizeSpec, StopOrderKind, TimeInForce,
};

use super::common::cedar::{addr, u256_hex};
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod adjust_margin;
mod cancel_order;
mod change_leverage;
mod change_margin_mode;
mod claim_funding;
mod close_position;
mod decrease_position;
mod increase_position;
mod open_position;
mod place_limit_order;
mod place_stop_order;

/// Dispatch a [`PerpAction`] to its per-action lowering.
///
/// # Errors
///
/// Infallible today: every variant has a leaf lowering. The `Result` matches
/// the shared per-action contract so the dispatch stays uniform.
pub(crate) fn lower(action: &PerpAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    match action {
        PerpAction::OpenPosition(a) => open_position::lower(a, ctx),
        PerpAction::ClosePosition(a) => close_position::lower(a, ctx),
        PerpAction::IncreasePosition(a) => increase_position::lower(a, ctx),
        PerpAction::DecreasePosition(a) => decrease_position::lower(a, ctx),
        PerpAction::AdjustMargin(a) => adjust_margin::lower(a, ctx),
        PerpAction::ChangeLeverage(a) => change_leverage::lower(a, ctx),
        PerpAction::ChangeMarginMode(a) => change_margin_mode::lower(a, ctx),
        PerpAction::PlaceLimitOrder(a) => place_limit_order::lower(a, ctx),
        PerpAction::PlaceStopOrder(a) => place_stop_order::lower(a, ctx),
        PerpAction::CancelOrder(a) => cancel_order::lower(a, ctx),
        PerpAction::ClaimFunding(a) => claim_funding::lower(a, ctx),
    }
}

/// Lower a [`PerpVenue`] → `{ name, chain, contract? }` (`Perp::PerpVenue`).
/// Every variant carries a `chain`; only `Generic` carries a `contract`.
pub(crate) fn lower_perp_venue(venue: &PerpVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        PerpVenue::Hyperliquid { chain }
        | PerpVenue::GmxV2 { chain }
        | PerpVenue::DyDxV4 { chain }
        | PerpVenue::Vertex { chain }
        | PerpVenue::Aevo { chain }
        | PerpVenue::Drift { chain }
        | PerpVenue::JupiterPerps { chain }
        | PerpVenue::Synthetix { chain } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
        PerpVenue::Generic { chain, contract } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
        }
    }
    Value::Object(m)
}

/// Lower a [`VenueRef`] → `{ name, chain? }` (`Perp::VenueRef`). The light venue
/// identifier embedded in a [`MarketRef`]; `chain` is omitted when absent.
pub(crate) fn lower_venue_ref(venue: &VenueRef) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name.clone()));
    if let Some(chain) = &venue.chain {
        m.insert("chain".into(), Value::String(chain.to_string()));
    }
    Value::Object(m)
}

/// Lower a [`MarketRef`] → `{ symbol, venue }` (`Perp::MarketRef`).
pub(crate) fn lower_market_ref(market: &MarketRef) -> Value {
    let mut m = Map::new();
    m.insert("symbol".into(), Value::String(market.symbol.clone()));
    m.insert("venue".into(), lower_venue_ref(&market.venue));
    Value::Object(m)
}

/// Lower a [`SizeSpec`] → discriminated `{ kind, … }` (`Perp::SizeSpec`). The
/// host-populated `amountUsdNano` projection is omitted.
pub(crate) fn lower_size_spec(size: &SizeSpec) -> Value {
    let mut m = Map::new();
    match size {
        SizeSpec::BaseAmount { amount } => {
            m.insert("kind".into(), Value::String("base_amount".into()));
            m.insert("amount".into(), Value::String(u256_hex(*amount)));
        }
        SizeSpec::QuoteAmount { amount_usd } => {
            m.insert("kind".into(), Value::String("quote_amount".into()));
            m.insert("amountUsd".into(), Value::String(u256_hex(*amount_usd)));
            // `amountUsdNano` is host-populated — OMITTED.
        }
        SizeSpec::LeverageImplied {
            collateral,
            leverage,
        } => {
            m.insert("kind".into(), Value::String("leverage_implied".into()));
            m.insert("collateral".into(), Value::String(u256_hex(*collateral)));
            m.insert("leverage".into(), Value::String(leverage.0.clone()));
        }
    }
    Value::Object(m)
}

/// Lower a [`TimeInForce`] → `{ kind, until? }` (`Perp::TimeInForce`). `until`
/// (unix seconds) is present only for the `Gtd` variant.
pub(crate) fn lower_time_in_force(tif: &TimeInForce) -> Value {
    let mut m = Map::new();
    match tif {
        TimeInForce::Gtc => {
            m.insert("kind".into(), Value::String("gtc".into()));
        }
        TimeInForce::Ioc => {
            m.insert("kind".into(), Value::String("ioc".into()));
        }
        TimeInForce::Fok => {
            m.insert("kind".into(), Value::String("fok".into()));
        }
        TimeInForce::PostOnly => {
            m.insert("kind".into(), Value::String("post_only".into()));
        }
        TimeInForce::Gtd { until } => {
            m.insert("kind".into(), Value::String("gtd".into()));
            m.insert("until".into(), Value::from(until.as_unix()));
        }
    }
    Value::Object(m)
}

/// Lower a [`PerpAccountState`] → `{ totalCollateralUsd, usedMarginUsd,
/// freeMarginUsd, openPositions }` (`Perp::PerpAccountState`). `open_positions`
/// (`Vec<(MarketRef, U256)>`) becomes a `Set<{ market, sizeBase }>`.
pub(crate) fn lower_perp_account_state(state: &PerpAccountState) -> Value {
    let mut m = Map::new();
    m.insert(
        "totalCollateralUsd".into(),
        Value::String(u256_hex(state.total_collateral_usd)),
    );
    m.insert(
        "usedMarginUsd".into(),
        Value::String(u256_hex(state.used_margin_usd)),
    );
    m.insert(
        "freeMarginUsd".into(),
        Value::String(u256_hex(state.free_margin_usd)),
    );
    let open: Vec<Value> = state
        .open_positions
        .iter()
        .map(|(market, size_base)| {
            let mut pos = Map::new();
            pos.insert("market".into(), lower_market_ref(market));
            pos.insert("sizeBase".into(), Value::String(u256_hex(*size_base)));
            Value::Object(pos)
        })
        .collect();
    m.insert("openPositions".into(), Value::Array(open));
    Value::Object(m)
}

/// Map a [`PerpSide`] to its `snake_case` schema spelling (`"long"`/`"short"`).
pub(crate) const fn perp_side(side: &PerpSide) -> &'static str {
    match side {
        PerpSide::Long => "long",
        PerpSide::Short => "short",
    }
}

/// Map a [`MarginMode`] to its `snake_case` schema spelling
/// (`"cross"`/`"isolated"`).
pub(crate) const fn margin_mode(mode: &MarginMode) -> &'static str {
    match mode {
        MarginMode::Cross => "cross",
        MarginMode::Isolated => "isolated",
    }
}

/// Map a [`StopOrderKind`] to its `snake_case` schema spelling.
pub(crate) const fn stop_order_kind(kind: &StopOrderKind) -> &'static str {
    match kind {
        StopOrderKind::StopMarket => "stop_market",
        StopOrderKind::StopLimit => "stop_limit",
        StopOrderKind::TakeProfit => "take_profit",
        StopOrderKind::TakeProfitLimit => "take_profit_limit",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, Price, SignedI256, Time, VenueRef, U256,
    };
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{LiveField, NonceKey};
    use policy_transition::action::perp::{
        PerpAccountState, PerpPositionLive, PerpVenue, SizeSpec,
    };
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::lowering_v2::{lower_action, TxMeta};

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    pub(crate) fn perp_contract() -> Address {
        Address::from_str("0x3333333333333333333333333333333333333333").unwrap()
    }

    /// A `Generic` venue (exercises the `contract` arm of `lower_perp_venue`).
    pub(crate) fn sample_venue() -> PerpVenue {
        PerpVenue::Generic {
            chain: ChainId::arbitrum(),
            contract: perp_contract(),
        }
    }

    /// All 9 [`PerpVenue`] variants paired with their schema `name` tag, so a
    /// single test can drive every venue arm through `assert_conforms`. The 8
    /// `{ chain }`-only arms plus the `Generic` `{ chain, contract }` arm.
    pub(crate) fn all_venues() -> Vec<(&'static str, PerpVenue)> {
        let chain = ChainId::arbitrum();
        vec![
            (
                "hyperliquid",
                PerpVenue::Hyperliquid {
                    chain: chain.clone(),
                },
            ),
            (
                "gmx_v2",
                PerpVenue::GmxV2 {
                    chain: chain.clone(),
                },
            ),
            (
                "dy_dx_v4",
                PerpVenue::DyDxV4 {
                    chain: chain.clone(),
                },
            ),
            (
                "vertex",
                PerpVenue::Vertex {
                    chain: chain.clone(),
                },
            ),
            (
                "aevo",
                PerpVenue::Aevo {
                    chain: chain.clone(),
                },
            ),
            (
                "drift",
                PerpVenue::Drift {
                    chain: chain.clone(),
                },
            ),
            (
                "jupiter_perps",
                PerpVenue::JupiterPerps {
                    chain: chain.clone(),
                },
            ),
            (
                "synthetix",
                PerpVenue::Synthetix {
                    chain: chain.clone(),
                },
            ),
            (
                "generic",
                PerpVenue::Generic {
                    chain,
                    contract: perp_contract(),
                },
            ),
        ]
    }

    /// A `BaseAmount` size spec (exercises the `amount` arm of
    /// `lower_size_spec`).
    pub(crate) fn sample_size_base() -> SizeSpec {
        SizeSpec::BaseAmount {
            amount: U256::from(2_000_000_000u64),
        }
    }

    /// A `QuoteAmount` size spec (exercises the `amountUsd` arm of
    /// `lower_size_spec`).
    pub(crate) fn sample_size_quote() -> SizeSpec {
        SizeSpec::QuoteAmount {
            amount_usd: U256::from(5_000_000_000u64),
        }
    }

    /// A [`PerpAccountState`] with **no** open positions (exercises the empty
    /// `openPositions` set arm of `lower_perp_account_state`).
    pub(crate) fn sample_account_state_empty() -> PerpAccountState {
        PerpAccountState {
            total_collateral_usd: U256::from(10_000_000_000u64),
            used_margin_usd: U256::ZERO,
            free_margin_usd: U256::from(10_000_000_000u64),
            open_positions: vec![],
        }
    }

    /// A [`PerpPositionLive`] with `liq_price` **absent** (exercises the `None`
    /// arm of `lower_perp_position_live`'s `liqPrice` field).
    pub(crate) fn sample_position_live_no_liq() -> PerpPositionLive {
        PerpPositionLive {
            size_base: U256::from(1_000_000_000u64),
            notional_usd: U256::from(3_000_000_000u64),
            entry_price: Price::new("3000"),
            mark_price: Price::new("3050"),
            liq_price: None,
            unrealized_pnl: SignedI256::try_from(50i64).unwrap(),
        }
    }

    /// `ETH-USD` market on a `hyperliquid` `VenueRef` (with a `chain`).
    pub(crate) fn sample_market() -> MarketRef {
        MarketRef {
            symbol: "ETH-USD".into(),
            venue: VenueRef {
                name: "hyperliquid".into(),
                chain: Some(ChainId::arbitrum()),
            },
        }
    }

    /// USDC collateral token on Arbitrum.
    pub(crate) fn sample_token() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::arbitrum(),
                address: Address::from_str("0xaf88d065e77c8cc2239327c5edb3a432268e5831").unwrap(),
            },
        }
    }

    /// A `LeverageImplied` size spec (exercises the `collateral`+`leverage` arm).
    pub(crate) fn sample_size() -> SizeSpec {
        SizeSpec::LeverageImplied {
            collateral: U256::from(1_000_000_000u64),
            leverage: Decimal::new("5"),
        }
    }

    pub(crate) fn sample_account_state() -> PerpAccountState {
        PerpAccountState {
            total_collateral_usd: U256::from(10_000_000_000u64),
            used_margin_usd: U256::from(2_000_000_000u64),
            free_margin_usd: U256::from(8_000_000_000u64),
            open_positions: vec![(sample_market(), U256::from(1_000_000_000u64))],
        }
    }

    pub(crate) fn sample_position_live() -> PerpPositionLive {
        PerpPositionLive {
            size_base: U256::from(1_000_000_000u64),
            notional_usd: U256::from(3_000_000_000u64),
            entry_price: Price::new("3000"),
            mark_price: Price::new("3050"),
            liq_price: Some(Price::new("2500")),
            unrealized_pnl: SignedI256::try_from(50i64).unwrap(),
        }
    }

    /// A `Pyth` oracle [`DataSource`] for `LiveField` construction.
    pub(crate) fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Pyth,
            feed_id: "ETH/USD".into(),
        }
    }

    /// Wrap a value in a `LiveField` sourced from the Pyth oracle.
    pub(crate) fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(value, oracle_src(), now())
    }

    /// An on-chain `ActionMeta` (Arbitrum `OnchainTx`).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::arbitrum(),
                nonce: 7,
                gas_limit: U256::from(500_000u64),
                gas_price: live(U256::from(100_000_000u64)),
                value: U256::ZERO,
            },
        }
    }

    /// An off-chain `ActionMeta` (`OffchainSig`) for venue-signed orders.
    pub(crate) fn offchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Hyperliquid".into(),
                    version: Some("1".into()),
                    chain_id: Some(42_161),
                    verifying_contract: None,
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                        .into(),
                }),
            },
        }
    }

    /// THE GATE: synthesize the per-policy schema for `tag`, lower the action,
    /// and strictly construct the Cedar context against it. A wrong rename,
    /// missing required field, or wrong type errors here.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered = lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use policy_state::primitives::ChainId;

    /// The merged `{ chain }` venue arm emits only `name` + `chain` (no
    /// `contract` leaks in), and the `Generic` arm adds `contract`.
    #[test]
    fn perp_venue_merged_group_and_generic_map_correctly() {
        let chain = ChainId::arbitrum();

        let hl = lower_perp_venue(&PerpVenue::Hyperliquid {
            chain: chain.clone(),
        });
        assert_eq!(hl["name"], serde_json::json!("hyperliquid"));
        assert_eq!(hl["chain"], serde_json::json!(chain.to_string()));
        assert!(hl.get("contract").is_none());

        let generic = lower_perp_venue(&test_support::sample_venue());
        assert_eq!(generic["name"], serde_json::json!("generic"));
        assert_eq!(
            generic["contract"],
            serde_json::json!(format!("{:#x}", test_support::perp_contract()))
        );
    }
}
