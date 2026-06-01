//! `Token::NftTransfer` lowering → `Token::NftTransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::NftTransferAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_key;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::NftTransfer` action into the `Token::NftTransferContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &NftTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    // `nftKey` is a bare `TokenKey` — lower via `lower_token_key` (not the ref).
    m.insert("nftKey".into(), lower_token_key(&action.nft_key));
    // `amount` is the only genuinely-optional populated field: ERC1155 quantity;
    // ABSENT for ERC721 (implicitly 1). Insert only when Some — never JSON null.
    if let Some(amount) = action.amount {
        m.insert("amount".into(), Value::String(u256_hex(amount)));
    }
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    // `amountNano` / `custom` are host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"NftTransfer""#, Value::Object(m)))
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
    use policy_state::token::TokenKey;
    use policy_transition::action::token::{NftTransferAction, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{onchain_meta, recipient, sample_erc1155_key, sample_nft_key};

    /// Gate an `NftTransfer` with the given key + optional amount.
    fn assert_transfer_conforms(nft_key: TokenKey, amount: Option<U256>) {
        let body = ActionBody::Token(TokenAction::NftTransfer(NftTransferAction {
            nft_key,
            amount,
            recipient: recipient(),
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("nft_transfer", &body, &meta);
    }

    /// `amount = Some(..)` — the ERC1155-quantity branch (field present).
    #[test]
    fn nft_transfer_lowering_conforms_to_schema() {
        assert_transfer_conforms(sample_erc1155_key(), Some(U256::from(2u64)));
    }

    /// `amount = None` — the ERC721 branch (field OMITTED, never JSON null).
    /// Pairs with an ERC721 `nftKey` (`standard = "erc721"`).
    #[test]
    fn nft_transfer_erc721_no_amount_conforms() {
        assert_transfer_conforms(sample_nft_key(), None);
    }
}
