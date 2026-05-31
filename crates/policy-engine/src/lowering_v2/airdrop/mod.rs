//! Airdrop-domain lowering: per-action dispatch (`Claim` / `Delegate`).

use simulation_reducer::action::airdrop::AirdropAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod claim;
mod delegate;

/// Dispatch an [`AirdropAction`] to its per-action lowering.
///
/// # Errors
///
/// Propagates the per-action `lower` result (infallible today).
pub(crate) fn lower(
    action: &AirdropAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        AirdropAction::Claim(a) => claim::lower(a, ctx),
        AirdropAction::Delegate(a) => delegate::lower(a, ctx),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    use crate::lowering_v2::{lower_action, TxMeta};

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn submitter() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A representative on-chain `DataSource` for `LiveField` construction.
    pub(crate) fn onchain_source() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Pyth,
            feed_id: "x".into(),
        }
    }

    /// A sample ERC-20 `TokenRef` on the given chain.
    pub(crate) fn sample_token_ref(chain: &ChainId) -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    /// An on-chain-transaction [`ActionMeta`].
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

    /// THE GATE: lower the action and strictly construct its Cedar context
    /// against the per-policy-composed schema. A rename / missing required
    /// field / wrong type ERRORS here.
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
