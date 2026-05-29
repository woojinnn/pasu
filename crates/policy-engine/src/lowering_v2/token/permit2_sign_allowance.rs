//! `Token::Permit2SignAllowance` lowering → `Token::Permit2SignAllowanceContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::token::Permit2SignAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Permit2SignAllowance` action into the
/// `Token::Permit2SignAllowanceContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Permit2SignAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `expiresAt` / `sigDeadline` are unix-seconds Longs (JSON numbers).
    m.insert("expiresAt".into(), Value::from(action.expires_at.as_unix()));
    m.insert(
        "sigDeadline".into(),
        Value::from(action.sig_deadline.as_unix()),
    );
    // `nonce` is a `LiveField<(U256, u8)>`; flatten its inner `(word, bit)`.
    let (word, bit) = action.nonce.value;
    let mut nonce = Map::new();
    nonce.insert("word".into(), Value::String(u256_hex(word)));
    nonce.insert("bit".into(), Value::from(i64::from(bit)));
    m.insert("nonce".into(), Value::Object(nonce));
    // `amountNano` / `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(
        r#"Token::Action::"Permit2SignAllowance""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::token::{Permit2SignAction, TokenAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Time, U256};

    use super::super::test_support::{
        live_nonce_pair, offchain_meta, sample_erc20_token, spender,
    };

    #[test]
    fn permit2_sign_allowance_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::Permit2SignAllowance(Permit2SignAction {
            token: sample_erc20_token(),
            spender: spender(),
            amount: U256::from(750_000_000u64),
            expires_at: Time::from_unix(1_740_000_000),
            sig_deadline: Time::from_unix(1_738_001_800),
            nonce: live_nonce_pair(U256::from(3u64), 5),
        }));
        // `permit2` signed allowance is an off-chain signature — offchain meta.
        let meta = offchain_meta();
        super::super::test_support::assert_conforms("permit2_sign_allowance", &body, &meta);
    }
}
