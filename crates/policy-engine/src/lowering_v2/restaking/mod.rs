//! Restaking-domain lowering: per-action dispatch + the shared `RestakingVenue`
//! lowering. Mirrors the `liquid_staking` layout. Round-1 actions carry no live
//! inputs, so contexts are `{ meta, venue, …action fields }` only.

use serde_json::{Map, Value};

use simulation_reducer::action::restaking::{RestakingAction, RestakingVenue};

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod complete_withdrawal;
mod delegate_to;
mod deposit;
mod queue_withdrawal;
mod redelegate;
mod register_operator;
mod undelegate;

/// Dispatch a [`RestakingAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &RestakingAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        RestakingAction::DelegateTo(a) => delegate_to::lower(a, ctx),
        RestakingAction::Redelegate(a) => redelegate::lower(a, ctx),
        RestakingAction::Undelegate(a) => undelegate::lower(a, ctx),
        RestakingAction::Deposit(a) => deposit::lower(a, ctx),
        RestakingAction::QueueWithdrawal(a) => queue_withdrawal::lower(a, ctx),
        RestakingAction::CompleteWithdrawal(a) => complete_withdrawal::lower(a, ctx),
        RestakingAction::RegisterOperator(a) => register_operator::lower(a, ctx),
    }
}

/// Lower a [`RestakingVenue`] → `{ name, chain }` (`Restaking::RestakingVenue`).
pub(crate) fn lower_restaking_venue(venue: &RestakingVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        RestakingVenue::EigenLayer { chain } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
    }
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Shared test support: sample builders + the conformance-gate helper (mirrors
// `liquid_staking::test_support`).
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::restaking::RestakingVenue;
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x3333333333333333333333333333333333333333";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn other() -> Address {
        Address::from_str("0x000000000000000000000000000000000000b02d").unwrap()
    }

    fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        }
    }

    /// An `EigenLayer` venue on Ethereum mainnet.
    pub(crate) fn eigenlayer_venue() -> RestakingVenue {
        RestakingVenue::EigenLayer {
            chain: ChainId::ethereum_mainnet(),
        }
    }

    /// stETH `TokenRef` on Ethereum mainnet (a sample LST deposit token).
    pub(crate) fn steth() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xae7ab96520de3a18e5e111b5eaab095312d7fe84").unwrap(),
            },
        }
    }

    /// An on-chain-transaction `ActionMeta` (Ethereum mainnet).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: other(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 3,
                gas_limit: U256::from(150_000u64),
                gas_price: LiveField::new(U256::from(20_000_000_000u64), oracle_src(), now()),
                value: U256::ZERO,
            },
        }
    }

    /// THE GATE: compose the per-policy schema for `tag`, lower `body`/`meta`,
    /// and STRICTLY construct the Cedar context against the schema.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": tag } } }
        }))
        .unwrap();
        let schema_text = crate::schema::compose_per_policy(&manifest).unwrap();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let lowered =
            crate::lowering_v2::lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();
        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
