//! `Launchpad::Commit` lowering → `Launchpad::CommitContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::launchpad::CommitAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_protocol_ref, lower_sale_state};

/// Lower a `Launchpad::Commit` action into the `Launchpad::CommitContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(action: &CommitAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("platform".into(), lower_protocol_ref(&action.platform));
    m.insert("saleId".into(), Value::String(action.sale_id.clone()));
    m.insert("payToken".into(), lower_token_ref(&action.pay_token));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` are host-populated 3-layer slots — OMITTED.
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert(
        "saleState".into(),
        lower_sale_state(&action.live_inputs.sale_state.value),
    );
    m.insert(
        "userCap".into(),
        Value::String(u256_hex(action.live_inputs.user_cap.value)),
    );
    m.insert(
        "userCommitted".into(),
        Value::String(u256_hex(action.live_inputs.user_committed.value)),
    );
    // `expected_token_price` is `LiveField<Option<Price>>`; inline its inner
    // Option and emit the Decimal-as-string only when present.
    if let Some(price) = &action.live_inputs.expected_token_price.value {
        m.insert("expectedTokenPrice".into(), Value::String(price.to_string()));
    }
    // `custom` is OMITTED — filled later by enrichment.

    Ok(ctx.lowered(r#"Launchpad::Action::"Commit""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::launchpad::{CommitAction, CommitLiveInputs, LaunchpadAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Price, U256};
    use simulation_state::LiveField;

    use super::super::test_support::{now, platform, sale_state, src, usdc, user};

    /// A representative on-chain `Commit`: versioned platform, ERC20 pay token,
    /// a full `SaleState` (caps + window + vest), and an expected price present.
    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = CommitAction {
            platform: platform(),
            sale_id: "sale-42".into(),
            pay_token: usdc(),
            amount: U256::from(1_000_000_000u64),
            recipient: user(),
            live_inputs: CommitLiveInputs {
                sale_state: LiveField::new(sale_state(), src(), now()),
                user_cap: LiveField::new(U256::from(2_000_000_000u64), src(), now()),
                user_committed: LiveField::new(U256::from(500_000_000u64), src(), now()),
                expected_token_price: LiveField::new(Some(Price::new("0.25")), src(), now()),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::Commit(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn commit_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("commit", &body, &meta);
    }
}
