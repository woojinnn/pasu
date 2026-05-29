//! `Perp::ClaimFunding` lowering → `Perp::ClaimFundingContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::perp::ClaimFundingAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_market_ref, lower_perp_venue};

/// Lower a `ClaimFundingAction` into the `Perp::ClaimFundingContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the shared per-action
/// `lower` contract.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ClaimFundingAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let li = &action.live_inputs;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_perp_venue(&action.venue));
    // `market` is `None` to claim from all markets — OMITTED when absent.
    if let Some(market) = &action.market {
        m.insert("market".into(), lower_market_ref(market));
    }
    // ClaimFundingLiveInputs flattened. `claimable` (Vec<(TokenRef, U256)>) →
    // Set<{ token, amount }>.
    let claimable: Vec<Value> = li
        .claimable
        .value
        .iter()
        .map(|(token, amount)| {
            let mut e = Map::new();
            e.insert("token".into(), lower_token_ref(token));
            e.insert("amount".into(), Value::String(u256_hex(*amount)));
            Value::Object(e)
        })
        .collect();
    m.insert("claimable".into(), Value::Array(claimable));
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Perp::Action::"ClaimFunding""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use simulation_reducer::action::perp::{ClaimFundingAction, ClaimFundingLiveInputs, PerpAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_market, sample_token, sample_venue,
    };

    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = ClaimFundingAction {
            venue: sample_venue(),
            // Claim from a single market (exercises the Some arm).
            market: Some(sample_market()),
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![(sample_token(), U256::from(1_234_567u64))]),
            },
        };
        (
            ActionBody::Perp(PerpAction::ClaimFunding(action)),
            onchain_meta(),
        )
    }

    #[test]
    fn claim_funding_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("claim_funding", &body, &meta);
    }
}
