//! Permission-domain lowering: protocol authorization grants/revocations.

use policy_transition::action::permission::PermissionAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod protocol_authorization;

/// Dispatch a [`PermissionAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract.
pub(crate) fn lower(
    action: &PermissionAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        PermissionAction::ProtocolAuthorization(a) => protocol_authorization::lower(a, ctx),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::LiveField;
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature};

    use crate::lowering_v2::{lower_action, TxMeta};

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn submitter() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    pub(crate) fn other() -> Address {
        Address::from_str("0x000000000000000000000000000000000000beef").unwrap()
    }

    pub(crate) fn onchain_source() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Pyth,
            feed_id: "x".into(),
        }
    }

    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: submitter(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 7,
                gas_limit: U256::from(200_000u64),
                gas_price: LiveField::new(U256::from(100_000_000u64), onchain_source(), now()),
                value: U256::ZERO,
            },
        }
    }

    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered = lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
