//! `Launchpad::ClaimVested` lowering → `Launchpad::ClaimVestedContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::launchpad::ClaimVestedAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Launchpad::ClaimVested` action into the
/// `Launchpad::ClaimVestedContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ClaimVestedAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    // `position_id` is a `PositionId` (= String) opaque identifier.
    m.insert("positionId".into(), Value::String(action.position_id.clone()));
    if let Some(amount) = action.amount {
        m.insert("amount".into(), Value::String(u256_hex(amount)));
    }
    m.insert(
        "claimableNow".into(),
        Value::String(u256_hex(action.live_inputs.claimable_now.value)),
    );
    // `next_unlock` is `LiveField<Option<(Time, U256)>>`; both flattened fields
    // are present iff a next unlock remains, otherwise both are omitted.
    if let Some((unlock_time, unlock_amount)) = &action.live_inputs.next_unlock.value {
        m.insert("nextUnlockTime".into(), Value::from(unlock_time.as_unix()));
        m.insert(
            "nextUnlockAmount".into(),
            Value::String(u256_hex(*unlock_amount)),
        );
    }
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Launchpad::Action::"ClaimVested""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::launchpad::{
        ClaimVestedAction, ClaimVestedLiveInputs, LaunchpadAction,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Time, U256};
    use simulation_state::LiveField;

    use super::super::test_support::{now, src};

    /// A representative on-chain `ClaimVested`: an explicit amount and a next
    /// unlock present (exercises both flattened `nextUnlock*` fields).
    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = ClaimVestedAction {
            position_id: "launchpad-alloc-7".into(),
            amount: Some(U256::from(100u64)),
            live_inputs: ClaimVestedLiveInputs {
                claimable_now: LiveField::new(U256::from(250u64), src(), now()),
                next_unlock: LiveField::new(
                    Some((Time::from_unix(1_742_000_000), U256::from(500u64))),
                    src(),
                    now(),
                ),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::ClaimVested(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn claim_vested_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("claim_vested", &body, &meta);
    }
}
