//! `Permission::ProtocolAuthorization` lowering.

use serde_json::{Map, Value};

use policy_transition::action::permission::ProtocolAuthorizationAction;

use super::super::common::cedar::addr;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a protocol-level authorization toggle into Cedar context.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// lowering contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &ProtocolAuthorizationAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("chain".into(), Value::String(action.chain.to_string()));
    m.insert("protocol".into(), Value::String(addr(&action.protocol)));
    m.insert(
        "protocolName".into(),
        Value::String(action.protocol_name.clone()),
    );
    m.insert(
        "permission".into(),
        Value::String(action.permission.as_str().into()),
    );
    if let Some(label) = &action.permission_label {
        m.insert("permissionLabel".into(), Value::String(label.clone()));
    }
    if let Some(limit) = &action.permission_limit {
        m.insert("permissionLimit".into(), Value::String(limit.clone()));
    }
    if let Some(authorizer) = &action.authorizer {
        m.insert("authorizer".into(), Value::String(addr(authorizer)));
    }
    m.insert("authorized".into(), Value::String(addr(&action.authorized)));
    m.insert("isAuthorized".into(), Value::Bool(action.is_authorized));

    Ok(ctx.lowered(
        r#"Permission::Action::"ProtocolAuthorization""#,
        Value::Object(m),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId};
    use policy_transition::action::permission::{
        PermissionAction, ProtocolAuthorizationAction, ProtocolPermissionKind,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{onchain_meta, other, submitter};

    fn balancer_vault() -> Address {
        Address::from_str("0xba12222222228d8ba445958a75a0704d566bf2c8").unwrap()
    }

    fn body(is_authorized: bool) -> ActionBody {
        ActionBody::Permission(PermissionAction::ProtocolAuthorization(
            ProtocolAuthorizationAction {
                chain: ChainId::ethereum_mainnet(),
                protocol: balancer_vault(),
                protocol_name: "balancer_v2".into(),
                permission: ProtocolPermissionKind::Relayer,
                permission_label: None,
                permission_limit: None,
                authorizer: Some(submitter()),
                authorized: other(),
                is_authorized,
            },
        ))
    }

    #[test]
    fn protocol_authorization_grant_conforms_to_schema() {
        super::super::test_support::assert_conforms(
            "protocol_authorization",
            &body(true),
            &onchain_meta(),
        );
    }

    #[test]
    fn protocol_authorization_revoke_conforms_to_schema() {
        super::super::test_support::assert_conforms(
            "protocol_authorization",
            &body(false),
            &onchain_meta(),
        );
    }

    #[test]
    fn protocol_authorization_optionals_conform_to_schema() {
        let body = ActionBody::Permission(PermissionAction::ProtocolAuthorization(
            ProtocolAuthorizationAction {
                chain: ChainId::arbitrum(),
                protocol: Address::ZERO,
                protocol_name: "hyperliquid".into(),
                permission: ProtocolPermissionKind::BuilderFee,
                permission_label: Some("builder fee cap".into()),
                permission_limit: Some("0.001%".into()),
                authorizer: Some(submitter()),
                authorized: other(),
                is_authorized: true,
            },
        ));

        super::super::test_support::assert_conforms(
            "protocol_authorization",
            &body,
            &onchain_meta(),
        );
    }
}
