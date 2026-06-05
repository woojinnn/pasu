//! `Launchpad::ClaimAllocation` lowering → `Launchpad::ClaimAllocationContext`.

use serde_json::{Map, Value};

use policy_transition::action::launchpad::ClaimAllocationAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_protocol_ref;

/// Lower a `Launchpad::ClaimAllocation` action into the
/// `Launchpad::ClaimAllocationContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &ClaimAllocationAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    // `allocated` is `LiveField<(TokenRef, U256)>`; flatten the inner tuple into
    // the parallel `allocatedToken` / `allocatedAmount` fields.
    let (allocated_token, allocated_amount) = &action.live_inputs.allocated.value;

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("platform".into(), lower_protocol_ref(&action.platform));
    m.insert("saleId".into(), Value::String(action.sale_id.clone()));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("allocatedToken".into(), lower_token_ref(allocated_token));
    m.insert(
        "allocatedAmount".into(),
        Value::String(u256_hex(*allocated_amount)),
    );
    // `allocatedAmountNano` / `allocatedAmountUsd` are host-populated — OMITTED.
    m.insert(
        "refundDue".into(),
        Value::String(u256_hex(action.live_inputs.refund_due.value)),
    );
    m.insert(
        "isClaimable".into(),
        Value::Bool(action.live_inputs.is_claimable.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Launchpad::Action::"ClaimAllocation""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::U256;
    use policy_state::LiveField;
    use policy_transition::action::launchpad::{
        ClaimAllocationAction, ClaimAllocationLiveInputs, LaunchpadAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{now, platform, src, usdc, user};

    /// A representative on-chain `ClaimAllocation`: an allocated (token, amount)
    /// pair, a refund owed, and claimable now.
    fn sample() -> (ActionBody, policy_transition::action::ActionMeta) {
        let action = ClaimAllocationAction {
            platform: platform(),
            sale_id: "sale-42".into(),
            recipient: user(),
            live_inputs: ClaimAllocationLiveInputs {
                allocated: LiveField::new((usdc(), U256::from(750_000_000u64)), src(), now()),
                refund_due: LiveField::new(U256::from(50_000_000u64), src(), now()),
                is_claimable: LiveField::new(true, src(), now()),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::ClaimAllocation(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn claim_allocation_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("claim_allocation", &body, &meta);
    }
}
