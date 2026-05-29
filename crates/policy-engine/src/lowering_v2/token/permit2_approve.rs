//! `Token::Permit2Approve` lowering → `Token::Permit2ApproveContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::token::Permit2ApproveAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Permit2Approve` action into the `Token::Permit2ApproveContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Permit2ApproveAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `expiresAt` is a unix-seconds Long (JSON number).
    m.insert("expiresAt".into(), Value::from(action.expires_at.as_unix()));
    // `amountNano` / `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Permit2Approve""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::token::{Permit2ApproveAction, TokenAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Time, U256};

    use super::super::test_support::{onchain_meta, sample_erc20_token, spender};

    #[test]
    fn permit2_approve_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::Permit2Approve(Permit2ApproveAction {
            token: sample_erc20_token(),
            spender: spender(),
            amount: U256::from(500_000_000u64),
            expires_at: Time::from_unix(1_740_000_000),
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("permit2_approve", &body, &meta);
    }
}
