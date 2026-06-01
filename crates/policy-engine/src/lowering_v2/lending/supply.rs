//! `Lending::Supply` lowering → `Lending::SupplyContext`.

use serde_json::{Map, Value};

use policy_transition::action::lending::SupplyAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_lending_venue, lower_reserve_state, lower_user_lending_state};

/// Lower a `Lending::Supply` action into the `Lending::SupplyContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SupplyAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated → omitted.
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    m.insert(
        "reserveState".into(),
        lower_reserve_state(&action.live_inputs.reserve_state.value),
    );
    m.insert(
        "supplyApy".into(),
        Value::String(action.live_inputs.supply_apy.value.to_string()),
    );
    m.insert(
        "aTokenPriceUsd".into(),
        Value::String(action.live_inputs.a_token_price_usd.value.to_string()),
    );
    m.insert(
        "eligibleAsCollat".into(),
        Value::Bool(action.live_inputs.eligible_as_collat.value),
    );
    m.insert(
        "userStateBefore".into(),
        lower_user_lending_state(&action.live_inputs.user_state_before.value),
    );
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"Supply""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::{Decimal, U256};
    use policy_transition::action::lending::{
        LendingAction, LendingVenue, ReserveState, SupplyAction, SupplyLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        live, onchain_meta, reserve_state, reserve_state_no_caps, usdc, user_state, venue,
        venue_aave_v2, venue_aave_v3_no_market, venue_compound_v2, venue_compound_v3, venue_fluid,
        venue_morpho_blue, venue_morpho_optimizer, venue_spark,
    };

    /// Build a `Supply` body with a chosen `venue` + `reserve_state`, holding all
    /// other fields fixed. Lets each test exercise exactly one
    /// `lower_lending_venue` / `lower_reserve_state` branch through the full gate.
    fn supply_body(venue: LendingVenue, reserve: ReserveState) -> ActionBody {
        ActionBody::Lending(LendingAction::Supply(SupplyAction {
            venue,
            asset: usdc(),
            amount: U256::from(1_000_000_000u64),
            on_behalf_of: Some(super::super::test_support::user()),
            live_inputs: SupplyLiveInputs {
                reserve_state: live(reserve),
                supply_apy: live(Decimal::new("0.0345")),
                a_token_price_usd: live(Decimal::new("1.00")),
                eligible_as_collat: live(true),
                user_state_before: live(user_state()),
            },
        }))
    }

    /// A representative `Supply` of USDC into Aave V3 (on-behalf-of populated).
    #[test]
    fn supply_lowering_conforms_to_schema() {
        let body = supply_body(venue(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `on_behalf_of == None` — exercises the omitted-`onBehalfOf` branch.
    #[test]
    fn supply_without_on_behalf_of_conforms() {
        let body = ActionBody::Lending(LendingAction::Supply(SupplyAction {
            venue: venue(),
            asset: usdc(),
            amount: U256::from(1_000_000_000u64),
            on_behalf_of: None,
            live_inputs: SupplyLiveInputs {
                reserve_state: live(reserve_state()),
                supply_apy: live(Decimal::new("0.0345")),
                a_token_price_usd: live(Decimal::new("1.00")),
                eligible_as_collat: live(false),
                user_state_before: live(user_state()),
            },
        }));
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `ReserveState` with BOTH caps absent — exercises the
    /// `supplyCap`/`borrowCap` omitted branches of `lower_reserve_state`.
    #[test]
    fn supply_reserve_state_without_caps_conforms() {
        let body = supply_body(venue(), reserve_state_no_caps());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    // -- LendingVenue: one test per remaining variant, each driven end-to-end
    //    through the supply gate so the venue's emitted fields are validated. --

    /// `AaveV3` WITHOUT a market id — omitted-`marketId` branch.
    #[test]
    fn supply_venue_aave_v3_no_market_conforms() {
        let body = supply_body(venue_aave_v3_no_market(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `AaveV2` venue — `{ pool }`.
    #[test]
    fn supply_venue_aave_v2_conforms() {
        let body = supply_body(venue_aave_v2(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `Spark` venue — shares the `{ pool }` arm with `AaveV2`.
    #[test]
    fn supply_venue_spark_conforms() {
        let body = supply_body(venue_spark(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `CompoundV3` venue — `{ comet, baseAsset }`.
    #[test]
    fn supply_venue_compound_v3_conforms() {
        let body = supply_body(venue_compound_v3(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `CompoundV2` venue — `{ comptroller }`.
    #[test]
    fn supply_venue_compound_v2_conforms() {
        let body = supply_body(venue_compound_v2(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `MorphoBlue` venue — `{ marketIdStr }`.
    #[test]
    fn supply_venue_morpho_blue_conforms() {
        let body = supply_body(venue_morpho_blue(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `MorphoOptimizer` venue — `{ vault }`.
    #[test]
    fn supply_venue_morpho_optimizer_conforms() {
        let body = supply_body(venue_morpho_optimizer(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }

    /// `Fluid` venue (on Arbitrum) — shares the `{ vault }` arm with
    /// `MorphoOptimizer`; also exercises a non-mainnet `chain` string.
    #[test]
    fn supply_venue_fluid_conforms() {
        let body = supply_body(venue_fluid(), reserve_state());
        super::super::test_support::assert_conforms("supply", &body, &onchain_meta());
    }
}
