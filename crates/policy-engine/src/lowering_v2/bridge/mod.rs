//! Bridge-domain lowering: cross-chain outbound send/deposit.

use policy_transition::action::bridge::BridgeAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod send;

/// Dispatch a [`BridgeAction`] to its per-action lowering.
pub(crate) fn lower(
    action: &BridgeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        BridgeAction::Send(a) => send::lower(a, ctx),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::LiveField;
    use policy_transition::action::bridge::BridgeRecipient;
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature};

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x5c7bcd6e7de5423a257d81b442095a1a6ced35c5";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn src_token() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    pub(crate) fn dst_token() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId("eip155:10".into()),
                address: Address::from_str("0x0b2c639c533813f4aa9d7837caf62653d097ff85").unwrap(),
            },
        }
    }

    pub(crate) fn evm_recipient() -> BridgeRecipient {
        BridgeRecipient::Evm {
            address: Address::from_str("0x000000000000000000000000000000000000b02d").unwrap(),
        }
    }

    fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        }
    }

    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: Address::from_str(FROM).unwrap(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 3,
                gas_limit: U256::from(150_000u64),
                gas_price: LiveField::new(U256::from(20_000_000_000u64), oracle_src(), now()),
                value: U256::ZERO,
            },
        }
    }

    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("bridge-{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.domain": { "eq": "bridge" }, "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered =
            crate::lowering_v2::lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
