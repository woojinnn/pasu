//! `Token::UnwrapNative` lowering → `Token::UnwrapNativeContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::UnwrapNativeAction;

use super::super::common::cedar::u256_hex;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::UnwrapNative` action into the `Token::UnwrapNativeContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &UnwrapNativeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"UnwrapNative""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::token::{TokenAction, UnwrapNativeAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{onchain_meta, sample_erc20_token};

    /// Unwrap carrying an ERC20 wrapper `token` ref (e.g. WETH). Exercises the
    /// `meta` + `token` (`lower_token_ref`) + `amount` (`u256_hex`) lowering
    /// against the synthesized `unwrap_native` per-policy schema.
    #[test]
    fn unwrap_native_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::UnwrapNative(UnwrapNativeAction {
            token: sample_erc20_token(),
            amount: U256::from(100_000_000_000_000u64),
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("unwrap_native", &body, &meta);
    }
}
