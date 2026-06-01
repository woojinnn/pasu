//! Lending-domain lowering: per-action dispatch + the shared `LendingVenue`,
//! `ReserveState`, `UserLendingState`, and `RateMode` lowerings.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::{
    LendingAction, LendingVenue, ReserveState, SetCollateralAction, UserLendingState,
};
use simulation_state::token::RateMode;

use super::common::cedar::{addr, u256_hex};
use super::common::token::lower_token_ref;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod borrow;
mod buy_collateral;
mod delegate_borrow;
mod disable_collateral;
mod enable_collateral;
mod liquidate;
mod repay;
mod set_authorization;
mod set_e_mode;
mod supply;
mod swap_rate_mode;
mod withdraw;

/// Dispatch a [`LendingAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &LendingAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        LendingAction::Supply(a) => supply::lower(a, ctx),
        LendingAction::Withdraw(a) => withdraw::lower(a, ctx),
        LendingAction::Borrow(a) => borrow::lower(a, ctx),
        LendingAction::BuyCollateral(a) => buy_collateral::lower(a, ctx),
        LendingAction::Repay(a) => repay::lower(a, ctx),
        LendingAction::SwapRateMode(a) => swap_rate_mode::lower(a, ctx),
        LendingAction::SetEMode(a) => set_e_mode::lower(a, ctx),
        LendingAction::EnableCollateral(a) => enable_collateral::lower(a, ctx),
        LendingAction::DisableCollateral(a) => disable_collateral::lower(a, ctx),
        LendingAction::DelegateBorrow(a) => delegate_borrow::lower(a, ctx),
        LendingAction::Liquidate(a) => liquidate::lower(a, ctx),
        LendingAction::SetAuthorization(a) => set_authorization::lower(a, ctx),
    }
}

/// Lower a [`LendingVenue`] → `{ name, chain, <per-variant optional fields> }`
/// (`Core::LendingVenue`). Only the fields a variant carries are emitted.
pub(crate) fn lower_lending_venue(venue: &LendingVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        LendingVenue::AaveV3 {
            chain,
            pool,
            market_id,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            if let Some(market_id) = market_id {
                m.insert("marketId".into(), Value::from(i64::from(*market_id)));
            }
        }
        // AaveV2 and Spark both expose only `{ chain, pool }`; the discriminating
        // `name` is already set above, so they share one arm.
        LendingVenue::AaveV2 { chain, pool } | LendingVenue::Spark { chain, pool } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
        }
        LendingVenue::CompoundV3 {
            chain,
            comet,
            base_asset,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("comet".into(), Value::String(addr(comet)));
            m.insert("baseAsset".into(), lower_token_ref(base_asset));
        }
        LendingVenue::CompoundV2 { chain, comptroller } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("comptroller".into(), Value::String(addr(comptroller)));
        }
        LendingVenue::MorphoBlue { chain, market_id } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            // Morpho Blue's market id is a 32-byte hex string → `marketIdStr`.
            m.insert("marketIdStr".into(), Value::String(market_id.clone()));
        }
        // MorphoOptimizer and Fluid both expose only `{ chain, vault }`.
        LendingVenue::MorphoOptimizer { chain, vault } | LendingVenue::Fluid { chain, vault } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("vault".into(), Value::String(addr(vault)));
        }
        // crvUSD / LlamaLend: the per-market `Controller` is the venue's pool.
        // The collateral token is carried on the Rust venue for the reducer but
        // is not needed by Cedar policies (the controller address identifies the
        // market), so only `{ chain, pool }` is lowered.
        LendingVenue::CrvUsd {
            chain, controller, ..
        }
        | LendingVenue::LlamaLend {
            chain, controller, ..
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(controller)));
        }
    }
    Value::Object(m)
}

/// Lower a [`ReserveState`] → `Core::ReserveState`. Optional caps are omitted
/// when absent.
pub(crate) fn lower_reserve_state(state: &ReserveState) -> Value {
    let mut m = Map::new();
    m.insert(
        "totalSupply".into(),
        Value::String(u256_hex(state.total_supply)),
    );
    m.insert(
        "totalBorrow".into(),
        Value::String(u256_hex(state.total_borrow)),
    );
    m.insert(
        "utilizationBp".into(),
        Value::from(i64::from(state.utilization_bp)),
    );
    if let Some(supply_cap) = state.supply_cap {
        m.insert("supplyCap".into(), Value::String(u256_hex(supply_cap)));
    }
    if let Some(borrow_cap) = state.borrow_cap {
        m.insert("borrowCap".into(), Value::String(u256_hex(borrow_cap)));
    }
    m.insert("ltvBp".into(), Value::from(i64::from(state.ltv_bp)));
    m.insert(
        "liquidationThresholdBp".into(),
        Value::from(i64::from(state.liquidation_threshold_bp)),
    );
    m.insert(
        "liquidationBonusBp".into(),
        Value::from(i64::from(state.liquidation_bonus_bp)),
    );
    m.insert(
        "reserveFactorBp".into(),
        Value::from(i64::from(state.reserve_factor_bp)),
    );
    m.insert("isFrozen".into(), Value::Bool(state.is_frozen));
    m.insert("isPaused".into(), Value::Bool(state.is_paused));
    Value::Object(m)
}

/// Lower a [`UserLendingState`] → `Core::UserLendingState`.
pub(crate) fn lower_user_lending_state(state: &UserLendingState) -> Value {
    let mut m = Map::new();
    m.insert(
        "healthFactor".into(),
        Value::String(state.health_factor.to_string()),
    );
    m.insert(
        "totalCollatUsd".into(),
        Value::String(u256_hex(state.total_collat_usd)),
    );
    m.insert(
        "totalDebtUsd".into(),
        Value::String(u256_hex(state.total_debt_usd)),
    );
    m.insert(
        "availableBorrowUsd".into(),
        Value::String(u256_hex(state.available_borrow_usd)),
    );
    Value::Object(m)
}

/// Build the shared context `Map` for a [`SetCollateralAction`]. Both the
/// `EnableCollateral` and `DisableCollateral` actions wrap the same body and
/// produce identical context fields — they differ only in their action uid, so
/// the leaves call this and wrap the result with their own uid.
pub(crate) fn lower_set_collateral_context(
    action: &SetCollateralAction,
    ctx: &LowerCtx<'_>,
) -> Value {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.
    Value::Object(m)
}

/// Map a [`RateMode`] to its `snake_case` schema spelling. The lending schemas
/// declare rate fields as plain `String` (`"variable" | "stable"`), not a
/// discriminated record.
pub(crate) const fn rate_mode_str(mode: &RateMode) -> &'static str {
    match mode {
        RateMode::Variable => "variable",
        RateMode::Stable => "stable",
        RateMode::Fixed => "fixed",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_state::primitives::{Address, ChainId};
    use simulation_state::token::{TokenKey, TokenRef};

    /// `AaveV3` emits `pool` + optional `marketId` (Long); when `market_id` is
    /// `None` the field is omitted. `MorphoBlue` emits `marketIdStr` (String),
    /// never `marketId`.
    #[test]
    fn lending_venue_aave_v3_and_morpho_blue_map_correctly() {
        let chain = ChainId::ethereum_mainnet();
        let pool = Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap();

        let aave = lower_lending_venue(&LendingVenue::AaveV3 {
            chain: chain.clone(),
            pool,
            market_id: Some(2),
        });
        assert_eq!(aave["name"], serde_json::json!("aave_v3"));
        assert_eq!(aave["pool"], serde_json::json!(format!("{pool:#x}")));
        assert_eq!(aave["marketId"], serde_json::json!(2));

        let aave_no_market = lower_lending_venue(&LendingVenue::AaveV3 {
            chain: chain.clone(),
            pool,
            market_id: None,
        });
        assert!(aave_no_market.get("marketId").is_none());

        let morpho = lower_lending_venue(&LendingVenue::MorphoBlue {
            chain,
            market_id: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
        });
        assert_eq!(morpho["name"], serde_json::json!("morpho_blue"));
        assert!(morpho.get("marketId").is_none());
        assert_eq!(
            morpho["marketIdStr"],
            serde_json::json!("0xabc0000000000000000000000000000000000000000000000000000000000000")
        );
    }

    /// `CompoundV3` carries a `baseAsset: Core::TokenRef`, lowered via
    /// `lower_token_ref`.
    #[test]
    fn lending_venue_compound_v3_carries_base_asset() {
        let chain = ChainId::ethereum_mainnet();
        let comet = Address::from_str("0xc3d688b66703497daa19211eedff47f25384cdc3").unwrap();
        let usdc = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();

        let venue = lower_lending_venue(&LendingVenue::CompoundV3 {
            chain: chain.clone(),
            comet,
            base_asset: TokenRef {
                key: TokenKey::Erc20 {
                    chain,
                    address: usdc,
                },
            },
        });
        assert_eq!(venue["name"], serde_json::json!("compound_v3"));
        assert_eq!(venue["comet"], serde_json::json!(format!("{comet:#x}")));
        assert_eq!(
            venue["baseAsset"]["key"]["standard"],
            serde_json::json!("erc20")
        );
    }

    /// `rate_mode_str` matches the serde `snake_case` discriminants.
    #[test]
    fn rate_mode_str_matches_serde_spelling() {
        assert_eq!(rate_mode_str(&RateMode::Variable), "variable");
        assert_eq!(rate_mode_str(&RateMode::Stable), "stable");
        assert_eq!(rate_mode_str(&RateMode::Fixed), "fixed");
    }
}

// ---------------------------------------------------------------------------
// Shared test support: sample builders + the conformance-gate helper. Leaf
// tests build a representative `(body, meta)` for their action and pass it to
// `assert_conforms`, which composes the per-policy schema and STRICTLY checks
// the lowered context against it (the gate).
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::lending::{LendingVenue, ReserveState, UserLendingState};
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Decimal, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::{LiveField, NonceKey};

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    pub(crate) fn other() -> Address {
        Address::from_str("0x000000000000000000000000000000000000b02d").unwrap()
    }

    /// A live-data source standing in for an on-chain view read.
    pub(crate) fn src() -> DataSource {
        DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            function: "getReserveData(address)".into(),
            decoder_id: "aave_v3_reserve_data".into(),
        }
    }

    /// An oracle source for price/rate live fields.
    pub(crate) fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "USDC/USD".into(),
        }
    }

    /// Wrap a value in a `LiveField` with the on-chain-view source.
    pub(crate) fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(value, src(), now())
    }

    /// An `AaveV3` venue on Ethereum mainnet (with a market id, to exercise the
    /// optional `marketId` field).
    pub(crate) fn venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: Some(1),
        }
    }

    /// An `AaveV3` venue WITHOUT a market id — exercises the `marketId == None`
    /// branch of `lower_lending_venue` through the end-to-end schema gate.
    pub(crate) fn venue_aave_v3_no_market() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    /// An `AaveV2` venue — emits `{ name, chain, pool }`.
    pub(crate) fn venue_aave_v2() -> LendingVenue {
        LendingVenue::AaveV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x7d2768de32b0b80b7a3454c06bdac94a69ddc7a9").unwrap(),
        }
    }

    /// A `Spark` venue — shares the `{ name, chain, pool }` arm with `AaveV2`.
    pub(crate) fn venue_spark() -> LendingVenue {
        LendingVenue::Spark {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xc13e21b648a5ee794902342038ff3adab66be987").unwrap(),
        }
    }

    /// A `CompoundV3` venue — emits `{ name, chain, comet, baseAsset }`.
    pub(crate) fn venue_compound_v3() -> LendingVenue {
        LendingVenue::CompoundV3 {
            chain: ChainId::ethereum_mainnet(),
            comet: Address::from_str("0xc3d688b66703497daa19211eedff47f25384cdc3").unwrap(),
            base_asset: usdc(),
        }
    }

    /// A `CompoundV2` venue — emits `{ name, chain, comptroller }`.
    pub(crate) fn venue_compound_v2() -> LendingVenue {
        LendingVenue::CompoundV2 {
            chain: ChainId::ethereum_mainnet(),
            comptroller: Address::from_str("0x3d9819210a31b4961b30ef54be2aed79b9c9cd3b").unwrap(),
        }
    }

    /// A `MorphoBlue` venue — emits `{ name, chain, marketIdStr }` (32-byte hex).
    pub(crate) fn venue_morpho_blue() -> LendingVenue {
        LendingVenue::MorphoBlue {
            chain: ChainId::ethereum_mainnet(),
            market_id: "0xb323495f7e4148be5643a4ea4a8221eef163e4bccfdedc2a6f4696baacbc86cc".into(),
        }
    }

    /// A `MorphoOptimizer` venue — shares the `{ name, chain, vault }` arm with
    /// `Fluid`.
    pub(crate) fn venue_morpho_optimizer() -> LendingVenue {
        LendingVenue::MorphoOptimizer {
            chain: ChainId::ethereum_mainnet(),
            vault: Address::from_str("0x186514400e52270cef3d80e1c6f8d10a75d14334").unwrap(),
        }
    }

    /// A `Fluid` venue — emits `{ name, chain, vault }`.
    pub(crate) fn venue_fluid() -> LendingVenue {
        LendingVenue::Fluid {
            chain: ChainId::arbitrum(),
            vault: Address::from_str("0x0c8c77b7ff4c2af7f6cebbe67350a490e3dd6cb3").unwrap(),
        }
    }

    /// A `ReserveState` with BOTH optional caps ABSENT — exercises the
    /// `supply_cap == None` / `borrow_cap == None` branches of
    /// `lower_reserve_state` (the `reserve_state()` builder has both present).
    pub(crate) fn reserve_state_no_caps() -> ReserveState {
        ReserveState {
            total_supply: U256::from(1_000_000_000_000u64),
            total_borrow: U256::from(600_000_000_000u64),
            utilization_bp: 6000,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 8000,
            liquidation_threshold_bp: 8500,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1000,
            is_frozen: false,
            is_paused: false,
        }
    }

    /// A USDC `TokenRef` on Ethereum mainnet.
    pub(crate) fn usdc() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    /// A WETH `TokenRef` on Ethereum mainnet.
    pub(crate) fn weth() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    /// A representative `ReserveState` (with caps populated).
    pub(crate) fn reserve_state() -> ReserveState {
        ReserveState {
            total_supply: U256::from(1_000_000_000_000u64),
            total_borrow: U256::from(600_000_000_000u64),
            utilization_bp: 6000,
            supply_cap: Some(U256::from(2_000_000_000_000u64)),
            borrow_cap: Some(U256::from(1_500_000_000_000u64)),
            ltv_bp: 8000,
            liquidation_threshold_bp: 8500,
            liquidation_bonus_bp: 500,
            reserve_factor_bp: 1000,
            is_frozen: false,
            is_paused: false,
        }
    }

    /// A representative aggregated `UserLendingState`.
    pub(crate) fn user_state() -> UserLendingState {
        UserLendingState {
            health_factor: Decimal::new("1.85"),
            total_collat_usd: U256::from(50_000_000_000u64),
            total_debt_usd: U256::from(20_000_000_000u64),
            available_borrow_usd: U256::from(15_000_000_000u64),
        }
    }

    /// An on-chain-transaction `ActionMeta` (Ethereum mainnet).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 7,
                gas_limit: U256::from(300_000u64),
                gas_price: LiveField::new(U256::from(20_000_000_000u64), oracle_src(), now()),
                value: U256::ZERO,
            },
        }
    }

    /// An off-chain-signature `ActionMeta` (EIP-712 + a nonce key). Exercises
    /// the `lower_eip712` + `nonceKey` branch of `lower_nature` through the full
    /// dispatch, widening the lending gate beyond the on-chain nature.
    pub(crate) fn offchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "AaveV3-CreditDelegation".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
                    ),
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

    /// THE GATE: compose the per-policy schema for `tag`, lower `body`/`meta`,
    /// and STRICTLY construct the Cedar context against the schema. A wrong
    /// rename / missing required field / wrong type ERRORS here.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered =
            crate::lowering_v2::lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
