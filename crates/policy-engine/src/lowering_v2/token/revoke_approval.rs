//! `Token::RevokeApproval` lowering → `Token::RevokeApprovalContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::{RevokeApprovalAction, RevokeScope};

use super::super::common::cedar::{addr, u256_hex};
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
        RevokeScope::Permit2UnorderedNonce {
            chain,
            word_pos,
            mask,
        } => {
            m.insert(
                "kind".into(),
                Value::String("permit2_unordered_nonce".into()),
            );
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("wordPos".into(), Value::String(u256_hex(*word_pos)));
            m.insert("mask".into(), Value::String(u256_hex(*mask)));
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
    use policy_state::primitives::{ChainId, U256};
    use policy_transition::action::token::{RevokeApprovalAction, RevokeScope, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        nft_contract, onchain_meta, sample_erc1155_key, sample_erc20_token, sample_nft_key, spender,
    };

    /// Build a `revoke_approval` body for the given scope and gate it.
    fn assert_scope_conforms(scope: RevokeScope) {
        let body = ActionBody::Token(TokenAction::RevokeApproval(RevokeApprovalAction { scope }));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("revoke_approval", &body, &meta);
    }

    #[test]
    fn revoke_approval_lowering_conforms_to_schema() {
        assert_scope_conforms(RevokeScope::Erc20 {
            token: sample_erc20_token(),
            spender: spender(),
        });
    }

    /// `kind = "nft_single_token"` carries only `nftKey` (a bare `TokenKey`).
    /// Exercise both NFT standards through the `nft_single_token` arm:
    /// ERC721 (`is_nft()` true) and ERC1155 (the merged-arm `else`).
    #[test]
    fn revoke_approval_nft_single_token_kind_conforms() {
        assert_scope_conforms(RevokeScope::NftSingleToken {
            nft_key: sample_nft_key(),
        });
        assert_scope_conforms(RevokeScope::NftSingleToken {
            nft_key: sample_erc1155_key(),
        });
    }

    /// `kind = "nft_set_for_all"` carries `chain` + `contract` + `spender`
    /// (no `token`, no `nftKey`).
    #[test]
    fn revoke_approval_nft_set_for_all_kind_conforms() {
        assert_scope_conforms(RevokeScope::NftSetForAll {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_contract(),
            spender: spender(),
        });
    }

    /// `kind = "permit2_lockdown"` carries `token` + `spender` (same field set
    /// as `erc20` but a distinct discriminator).
    #[test]
    fn revoke_approval_permit2_lockdown_kind_conforms() {
        assert_scope_conforms(RevokeScope::Permit2Lockdown {
            token: sample_erc20_token(),
            spender: spender(),
        });
    }

    /// `kind = "permit2_unordered_nonce"` carries the bitmap location Permit2
    /// invalidates. No token/spender is known for this nonce class.
    #[test]
    fn revoke_approval_permit2_unordered_nonce_kind_conforms() {
        assert_scope_conforms(RevokeScope::Permit2UnorderedNonce {
            chain: ChainId::ethereum_mainnet(),
            word_pos: U256::from(42u64),
            mask: U256::from(0xffu64),
        });
    }
}
