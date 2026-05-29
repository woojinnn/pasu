//! `Token::Erc20Transfer` lowering → `Token::Erc20TransferContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::token::Erc20TransferAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Erc20Transfer` action into the `Token::Erc20TransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Erc20TransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    // `amountNano` / `amountUsd` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"Erc20Transfer""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::token::{Erc20TransferAction, TokenAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;
    use simulation_state::token::TokenRef;

    use super::super::test_support::{
        onchain_meta, recipient, sample_erc1155_token, sample_erc20_token, sample_native_key,
    };

    /// Gate an `Erc20Transfer` carrying the given `token` ref. The `token` slot
    /// is the only place `lower_token_ref` / `lower_token_key` runs in this
    /// action, so varying the `TokenKey` standard exercises every `Core::TokenKey`
    /// discriminator arm end-to-end against the schema.
    fn assert_transfer_token_conforms(token: TokenRef) {
        let body = ActionBody::Token(TokenAction::Erc20Transfer(Erc20TransferAction {
            token,
            recipient: recipient(),
            amount: U256::from(1_234_567u64),
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("erc20_transfer", &body, &meta);
    }

    /// ERC20 `token` (`standard = "erc20"`, carries `address`).
    #[test]
    fn erc20_transfer_lowering_conforms_to_schema() {
        assert_transfer_token_conforms(sample_erc20_token());
    }

    /// Native `token` (`standard = "native"`) — the `lower_token_key` arm that
    /// emits NEITHER `address` nor `contract`/`tokenId`.
    #[test]
    fn erc20_transfer_native_token_key_conforms() {
        assert_transfer_token_conforms(TokenRef {
            key: sample_native_key(),
        });
    }

    /// ERC1155 `token` (`standard = "erc1155"`) — the `{ contract, tokenId }`
    /// arm reached via `lower_token_ref`.
    #[test]
    fn erc20_transfer_erc1155_token_key_conforms() {
        assert_transfer_token_conforms(sample_erc1155_token());
    }
}
