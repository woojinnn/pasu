//! `Token::NftSetApprovalForAll` lowering → `Token::NftSetApprovalForAllContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::NftSetForAllAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::NftSetApprovalForAll` action into the
/// `Token::NftSetApprovalForAllContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &NftSetForAllAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("chain".into(), Value::String(action.chain.to_string()));
    m.insert("contract".into(), Value::String(addr(&action.contract)));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("approved".into(), Value::Bool(action.approved));
    // `custom` is host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"NftSetApprovalForAll""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_state::primitives::ChainId;
    use policy_transition::action::token::{NftSetForAllAction, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{nft_contract, onchain_meta, spender};

    /// Gate a `NftSetApprovalForAll` with the given `approved` flag.
    fn assert_set_for_all_conforms(approved: bool) {
        let body = ActionBody::Token(TokenAction::NftSetApprovalForAll(NftSetForAllAction {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_contract(),
            spender: spender(),
            approved,
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("nft_set_approval_for_all", &body, &meta);
    }

    /// `approved = true` — the grant branch.
    #[test]
    fn nft_set_approval_for_all_lowering_conforms_to_schema() {
        assert_set_for_all_conforms(true);
    }

    /// `approved = false` — the revoke branch (`setApprovalForAll(false)`).
    #[test]
    fn nft_set_approval_for_all_revoke_conforms() {
        assert_set_for_all_conforms(false);
    }
}
