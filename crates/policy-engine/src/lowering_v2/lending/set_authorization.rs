//! `Lending::SetAuthorization` lowering → `Lending::SetAuthorizationContext`.

use serde_json::{Map, Value};

use policy_transition::action::lending::SetAuthorizationAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Lending::SetAuthorization` action into the
/// `Lending::SetAuthorizationContext` shape. This action carries no live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SetAuthorizationAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("chain".into(), Value::String(action.chain.to_string()));
    m.insert("protocol".into(), Value::String(addr(&action.protocol)));
    if let Some(authorizer) = &action.authorizer {
        m.insert("authorizer".into(), Value::String(addr(authorizer)));
    }
    m.insert("authorized".into(), Value::String(addr(&action.authorized)));
    m.insert("isAuthorized".into(), Value::Bool(action.is_authorized));
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Lending::Action::"SetAuthorization""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId};
    use policy_transition::action::lending::{LendingAction, SetAuthorizationAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{offchain_meta, onchain_meta, other};

    fn morpho() -> Address {
        Address::from_str("0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb").unwrap()
    }

    fn body(is_authorized: bool) -> ActionBody {
        ActionBody::Lending(LendingAction::SetAuthorization(SetAuthorizationAction {
            chain: ChainId::ethereum_mainnet(),
            protocol: morpho(),
            authorizer: None,
            authorized: other(),
            is_authorized,
        }))
    }

    /// On-chain `setAuthorization` granting control to another address.
    #[test]
    fn set_authorization_grant_conforms_to_schema() {
        super::super::test_support::assert_conforms(
            "set_authorization",
            &body(true),
            &onchain_meta(),
        );
    }

    /// Off-chain `Authorization` EIP-712 (revoke) — exercises the offchain-sig
    /// nature through the lending gate.
    #[test]
    fn set_authorization_revoke_offchain_conforms() {
        super::super::test_support::assert_conforms(
            "set_authorization",
            &body(false),
            &offchain_meta(),
        );
    }
}
