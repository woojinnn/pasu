//! `Token::NftApprove` lowering → `Token::NftApproveContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::token::NftApproveAction;

use super::super::common::cedar::addr;
use super::super::common::token::lower_token_key;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::NftApprove` action into the `Token::NftApproveContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &NftApproveAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    // `nftKey` is a bare `TokenKey` — lower via `lower_token_key` (not the ref).
    m.insert("nftKey".into(), lower_token_key(&action.nft_key));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    // `custom` is host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"NftApprove""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::token::{NftApproveAction, TokenAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::token::TokenKey;

    use super::super::test_support::{
        onchain_meta, sample_erc1155_key, sample_nft_key, spender,
    };

    /// Gate an `NftApprove` carrying the given `nftKey`.
    fn assert_approve_conforms(nft_key: TokenKey) {
        let body = ActionBody::Token(TokenAction::NftApprove(NftApproveAction {
            nft_key,
            spender: spender(),
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("nft_approve", &body, &meta);
    }

    /// ERC721 `nftKey` (`standard = "erc721"`).
    #[test]
    fn nft_approve_lowering_conforms_to_schema() {
        assert_approve_conforms(sample_nft_key());
    }

    /// ERC1155 `nftKey` (`standard = "erc1155"`) — the other half of the merged
    /// `lower_token_key` arm, reachable via the per-token-id ERC1155 approval.
    #[test]
    fn nft_approve_erc1155_key_conforms() {
        assert_approve_conforms(sample_erc1155_key());
    }
}
