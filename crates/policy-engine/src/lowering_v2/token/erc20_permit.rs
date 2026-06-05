//! `Token::Erc20Permit` lowering → `Token::Erc20PermitContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::Erc20PermitAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Erc20Permit` action into the `Token::Erc20PermitContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Erc20PermitAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `deadline` is a unix-seconds Long (JSON number).
    m.insert("deadline".into(), Value::from(action.deadline.as_unix()));
    // `nonce` is a `LiveField<U256>`; inline its inner value as U256 hex.
    m.insert("nonce".into(), Value::String(u256_hex(action.nonce.value)));
    // `amountNano` / `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Erc20Permit""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::{Time, U256};
    use policy_transition::action::token::{Erc20PermitAction, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{live_u256, offchain_meta, sample_erc20_token, spender};

    #[test]
    fn erc20_permit_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::Erc20Permit(Erc20PermitAction {
            token: sample_erc20_token(),
            spender: spender(),
            amount: U256::from(1_000_000_000u64),
            deadline: Time::from_unix(1_738_001_800),
            nonce: live_u256(U256::from(7u64)),
        }));
        // `permit` is an off-chain signature — use the offchain meta.
        let meta = offchain_meta();
        super::super::test_support::assert_conforms("erc20_permit", &body, &meta);
    }
}
