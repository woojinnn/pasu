//! `Token::RevokeApproval` lowering → `Token::RevokeApprovalContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::token::{RevokeApprovalAction, RevokeScope};

use super::super::common::cedar::addr;
use super::super::common::token::{lower_token_key, lower_token_ref};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::RevokeApproval` action into the `Token::RevokeApprovalContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &RevokeApprovalAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("scope".into(), lower_revoke_scope(&action.scope));
    // `custom` is host-populated — OMITTED here.

    Ok(ctx.lowered(r#"Token::Action::"RevokeApproval""#, Value::Object(m)))
}

/// Lower a [`RevokeScope`] → discriminated `{ kind, token?, spender?, nftKey?,
/// chain?, contract? }` (`Token::RevokeScope`). Only the fields a variant carries
/// are emitted; the rest are omitted (the Cedar record's optionals).
fn lower_revoke_scope(scope: &RevokeScope) -> Value {
    let mut m = Map::new();
    match scope {
        RevokeScope::Erc20 { token, spender } => {
            m.insert("kind".into(), Value::String("erc20".into()));
            m.insert("token".into(), lower_token_ref(token));
            m.insert("spender".into(), Value::String(addr(spender)));
        }
        RevokeScope::NftSingleToken { nft_key } => {
            m.insert("kind".into(), Value::String("nft_single_token".into()));
            m.insert("nftKey".into(), lower_token_key(nft_key));
        }
        RevokeScope::NftSetForAll {
            chain,
            contract,
            spender,
        } => {
            m.insert("kind".into(), Value::String("nft_set_for_all".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
            m.insert("spender".into(), Value::String(addr(spender)));
        }
        RevokeScope::Permit2Lockdown { token, spender } => {
            m.insert("kind".into(), Value::String("permit2_lockdown".into()));
            m.insert("token".into(), lower_token_ref(token));
            m.insert("spender".into(), Value::String(addr(spender)));
        }
    }
    Value::Object(m)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::token::{RevokeApprovalAction, RevokeScope, TokenAction};
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{onchain_meta, sample_erc20_token, spender};

    #[test]
    fn revoke_approval_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::RevokeApproval(RevokeApprovalAction {
            scope: RevokeScope::Erc20 {
                token: sample_erc20_token(),
                spender: spender(),
            },
        }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("revoke_approval", &body, &meta);
    }
}
