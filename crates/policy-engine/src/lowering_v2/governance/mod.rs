//! Governance-domain lowering: delegation, proposal voting, and lifecycle.

use serde_json::{Map, Value};

use policy_transition::action::governance::{GovernanceAction, GovernanceVenue};

use super::common::cedar::{addr, u256_hex};
use super::common::token::lower_token_ref;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod activate_voting;
mod cancel;
mod close_vote;
mod delegate;
mod execute;
mod propose;
mod queue;
mod redeem_cancellation_fee;
mod start_vote;
mod update_representative;
mod vote;

/// Dispatch a [`GovernanceAction`] to its per-action lowering.
pub(crate) fn lower(
    action: &GovernanceAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        GovernanceAction::Delegate(a) => delegate::lower(a, ctx),
        GovernanceAction::Vote(a) => vote::lower(a, ctx),
        GovernanceAction::Propose(a) => propose::lower(a, ctx),
        GovernanceAction::Cancel(a) => cancel::lower(a, ctx),
        GovernanceAction::ActivateVoting(a) => activate_voting::lower(a, ctx),
        GovernanceAction::Queue(a) => queue::lower(a, ctx),
        GovernanceAction::Execute(a) => execute::lower(a, ctx),
        GovernanceAction::StartVote(a) => start_vote::lower(a, ctx),
        GovernanceAction::CloseVote(a) => close_vote::lower(a, ctx),
        GovernanceAction::RedeemCancellationFee(a) => redeem_cancellation_fee::lower(a, ctx),
        GovernanceAction::UpdateRepresentative(a) => update_representative::lower(a, ctx),
    }
}

pub(crate) fn lower_governance_venue(venue: &GovernanceVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        GovernanceVenue::AaveGovernanceV3 { chain, governance } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("governance".into(), Value::String(addr(governance)));
        }
        GovernanceVenue::AaveVotingMachine {
            chain,
            voting_machine,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("votingMachine".into(), Value::String(addr(voting_machine)));
        }
        GovernanceVenue::GovernanceToken { chain, token } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("token".into(), lower_token_ref(token));
        }
    }
    Value::Object(m)
}

pub(crate) fn lower_proposal_id(id: policy_state::primitives::U256) -> Value {
    Value::String(u256_hex(id))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::LiveField;
    use policy_transition::action::governance::GovernanceVenue;
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature};

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x9aee0b04504cef83a65ac3f0e838d0593bcb2bc7";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn other() -> Address {
        Address::from_str("0x000000000000000000000000000000000000b02d").unwrap()
    }

    pub(crate) fn aave_governance_v3() -> GovernanceVenue {
        GovernanceVenue::AaveGovernanceV3 {
            chain: ChainId::ethereum_mainnet(),
            governance: Address::from_str("0x9aee0b04504cef83a65ac3f0e838d0593bcb2bc7").unwrap(),
        }
    }

    pub(crate) fn aave_voting_machine() -> GovernanceVenue {
        GovernanceVenue::AaveVotingMachine {
            chain: ChainId::ethereum_mainnet(),
            voting_machine: Address::from_str("0x06a1795a88b82700896583e123f46be43877bfb6")
                .unwrap(),
        }
    }

    pub(crate) fn aave_token_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9").unwrap(),
            },
        }
    }

    pub(crate) fn governance_token_venue() -> GovernanceVenue {
        GovernanceVenue::GovernanceToken {
            chain: ChainId::ethereum_mainnet(),
            token: aave_token_ref(),
        }
    }

    fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        }
    }

    pub(crate) fn onchain_source() -> DataSource {
        DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9").unwrap(),
            function: "getDelegateeByType(address,uint8)".into(),
            decoder_id: "aave_governance_delegate".into(),
        }
    }

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

    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("governance-{}-schema", tag),
            "schema_version": 2,
            "trigger": { "where": { "action.domain": { "eq": "governance" }, "action.tag": { "eq": tag } } }
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
