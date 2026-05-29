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
    use simulation_reducer::action::launchpad::{
        CommitAction, CommitLiveInputs, LaunchpadAction, SaleState,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Price, U256};
    use simulation_state::LiveField;

    use super::super::test_support::{
        now, platform, sale_state, sale_state_custom_vest, sale_state_linear_bare_vest,
        sale_state_minimal, src, usdc, user,
    };

    /// Build a `Commit` body parameterized by the `SaleState` to lower and
    /// whether an `expected_token_price` is present. Lets each branch test pick
    /// the exact `SaleState`/price combination it wants to exercise.
    fn commit_with(
        sale: SaleState,
        price: Option<Price>,
    ) -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let action = CommitAction {
            platform: platform(),
            sale_id: "sale-42".into(),
            pay_token: usdc(),
            amount: U256::from(1_000_000_000u64),
            recipient: user(),
            live_inputs: CommitLiveInputs {
                sale_state: LiveField::new(sale, src(), now()),
                user_cap: LiveField::new(U256::from(2_000_000_000u64), src(), now()),
                user_committed: LiveField::new(U256::from(500_000_000u64), src(), now()),
                expected_token_price: LiveField::new(price, src(), now()),
            },
        };
        (
            ActionBody::Launchpad(LaunchpadAction::Commit(action)),
            super::super::test_support::onchain_meta(),
        )
    }

    /// A representative on-chain `Commit`: versioned platform, ERC20 pay token,
    /// a full `SaleState` (caps + window + Stepped vest), and a price present.
    fn sample() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        commit_with(sale_state(), Some(Price::new("0.25")))
    }

    #[test]
    fn commit_lowering_conforms_to_schema() {
        let (body, meta) = sample();
        super::super::test_support::assert_conforms("commit", &body, &meta);
    }

    /// `expected_token_price == None`: the `expectedTokenPrice` field is omitted.
    /// Combined with a MINIMAL `SaleState`, this covers the omit-branch of every
    /// `SaleState` optional (`hardCap`/`softCap`/`vestSchedule` all absent).
    #[test]
    fn commit_no_price_minimal_sale_state_conforms() {
        let (body, meta) = commit_with(sale_state_minimal(), None);
        super::super::test_support::assert_conforms("commit", &body, &meta);
    }

    /// `VestCurve::Linear` with `cliff`/`end` ABSENT and a mixed-cap `SaleState`
    /// (hardCap present, softCap absent). Exercises the linear vest curve plus
    /// the cliff/end omit-branches in `lower_vest_schedule`.
    #[test]
    fn commit_linear_bare_vest_conforms() {
        let (body, meta) = commit_with(sale_state_linear_bare_vest(), Some(Price::new("1")));
        super::super::test_support::assert_conforms("commit", &body, &meta);
    }

    /// `VestCurve::Custom` with `cliff` present / `end` absent and a mixed-cap
    /// `SaleState` (hardCap absent, softCap present). Exercises the custom vest
    /// curve `description` branch.
    #[test]
    fn commit_custom_vest_conforms() {
        let (body, meta) = commit_with(sale_state_custom_vest(), None);
        super::super::test_support::assert_conforms("commit", &body, &meta);
    }
}
