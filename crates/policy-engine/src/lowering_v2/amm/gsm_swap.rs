//! `Amm::GsmSwap` lowering → `Amm::GsmSwapContext`.

use serde_json::{Map, Value};

use policy_transition::action::amm::GsmSwapAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_amm_venue;

/// Lower an `Amm::GsmSwap` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GsmSwapAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_amm_venue(&action.venue));
    m.insert("asset".into(), lower_token_ref(&action.asset));
    m.insert("gho".into(), lower_token_ref(&action.gho));
    m.insert("side".into(), Value::String(action.side.as_str().into()));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));

    Ok(ctx.lowered(r#"Amm::Action::"GsmSwap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId, U256};
    use policy_transition::action::amm::{AmmAction, AmmVenue, GsmSwapAction, GsmSwapSide};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, onchain_meta, sample_token_ref, user};

    #[test]
    fn gsm_swap_conforms() {
        let chain = ChainId::ethereum_mainnet();
        let body = ActionBody::Amm(AmmAction::GsmSwap(GsmSwapAction {
            venue: AmmVenue::AaveGsm {
                chain: chain.clone(),
                gsm: Address::from_str("0x3a3868898305f04bec7fea77becff04c13444112").unwrap(),
            },
            asset: sample_token_ref(&chain),
            gho: sample_token_ref(&chain),
            side: GsmSwapSide::BuyAsset,
            amount: U256::from(1_000_000u64),
            recipient: user(),
        }));
        assert_conforms("gsm_swap", &body, &onchain_meta());
    }
}
