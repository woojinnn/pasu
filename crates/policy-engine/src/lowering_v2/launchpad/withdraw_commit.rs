//! `Launchpad::WithdrawCommit` lowering → `Launchpad::WithdrawCommitContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::launchpad::WithdrawCommitAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_protocol_ref, lower_sale_state};

/// Lower a `Launchpad::WithdrawCommit` action into the
/// `Launchpad::WithdrawCommitContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &WithdrawCommitAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("platform".into(), lower_protocol_ref(&action.platform));
    m.insert("saleId".into(), Value::String(action.sale_id.clone()));
    if let Some(amount) = action.amount {
        m.insert("amount".into(), Value::String(u256_hex(amount)));
    }
    m.insert(
        "withdrawable".into(),
        Value::String(u256_hex(action.live_inputs.withdrawable.value)),
    );
    m.insert(
        "saleState".into(),
        lower_sale_state(&action.live_inputs.sale_state.value),
    );
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Launchpad::Action::"WithdrawCommit""#, Value::Object(m)))
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
        LaunchpadAction, WithdrawCommitAction, WithdrawCommitLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;
    use simulation_state::LiveField;

    use super::super::test_support::{now, platform, sale_state, src};

    /// A representative on-chain `WithdrawCommit`: an explicit withdraw amount,
    /// the withdrawable balance, and the current `SaleState`.
    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = WithdrawCommitAction {
            platform: platform(),
            sale_id: "sale-42".into(),
            amount: Some(U256::from(250_000_000u64)),
            live_inputs: WithdrawCommitLiveInputs {
                withdrawable: LiveField::new(U256::from(500_000_000u64), src(), now()),
                sale_state: LiveField::new(sale_state(), src(), now()),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::WithdrawCommit(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn withdraw_commit_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("withdraw_commit", &body, &meta);
    }
}
