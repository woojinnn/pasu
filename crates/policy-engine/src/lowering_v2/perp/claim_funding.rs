//! `Perp::ClaimFunding` lowering → `Perp::ClaimFundingContext`.

use serde_json::{Map, Value};

use policy_transition::action::perp::ClaimFundingAction;

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
    use policy_state::primitives::U256;
    use policy_transition::action::perp::{ClaimFundingAction, ClaimFundingLiveInputs, PerpAction};
    use policy_transition::action::ActionBody;

    use policy_state::primitives::MarketRef;

    use super::super::test_support::{
        assert_conforms, live, onchain_meta, sample_market, sample_token, sample_venue,
    };

    /// Build a `ClaimFunding` body with the requested optional `market`.
    fn build(market: Option<MarketRef>) -> ActionBody {
        let action = ClaimFundingAction {
            venue: sample_venue(),
            market,
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![(sample_token(), U256::from(1_234_567u64))]),
            },
        };
        ActionBody::Perp(PerpAction::ClaimFunding(action))
    }

    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        // Claim from a single market (exercises the Some arm).
        (build(Some(sample_market())), onchain_meta())
    }

    #[test]
    fn claim_funding_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        assert_conforms("claim_funding", &body, &meta);
    }

    /// `market = None` — claim from all markets; the `market` key is omitted (the
    /// absent arm of the optional `market`).
    #[test]
    fn claim_funding_all_markets_conforms() {
        let body = build(None);
        assert_conforms("claim_funding", &body, &onchain_meta());
    }
}
