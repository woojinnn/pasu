//! Staking-domain lowering: per-action dispatch + the shared `StakeVenue`
//! lowering. Mirrors the `liquid_staking` layout. Actions carry no live inputs,
//! so contexts are `{ meta, venue, …action fields }` only.

use serde_json::{Map, Value};

use simulation_reducer::action::staking::{StakeVenue, StakingAction};

use super::common::cedar::addr;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod claim_rewards;
mod gauge_deposit;
mod gauge_withdraw;
mod increase_lock_amount;
mod increase_lock_time;
mod lock;
mod unlock;
mod vote_for_gauge;

/// Dispatch a [`StakingAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &StakingAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        StakingAction::Lock(a) => lock::lower(a, ctx),
        StakingAction::IncreaseLockAmount(a) => increase_lock_amount::lower(a, ctx),
        StakingAction::IncreaseLockTime(a) => increase_lock_time::lower(a, ctx),
        StakingAction::Unlock(a) => unlock::lower(a, ctx),
        StakingAction::ClaimRewards(a) => claim_rewards::lower(a, ctx),
        StakingAction::VoteForGauge(a) => vote_for_gauge::lower(a, ctx),
        StakingAction::GaugeDeposit(a) => gauge_deposit::lower(a, ctx),
        StakingAction::GaugeWithdraw(a) => gauge_withdraw::lower(a, ctx),
    }
}

/// Lower a [`StakeVenue`] → `{ name, chain, <addr field> }` (`Staking::StakeVenue`).
/// Each variant carries its single contract address under a variant-named key.
pub(crate) fn lower_stake_venue(venue: &StakeVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        StakeVenue::CurveVotingEscrow { chain, escrow } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("escrow".into(), Value::String(addr(escrow)));
        }
        StakeVenue::CurveMinter { chain, minter } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("minter".into(), Value::String(addr(minter)));
        }
        StakeVenue::CurveGaugeController { chain, controller } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("controller".into(), Value::String(addr(controller)));
        }
        StakeVenue::CurveGauge { chain, gauge } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("gauge".into(), Value::String(addr(gauge)));
        }
        StakeVenue::CurveFeeDistributor { chain, distributor } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("distributor".into(), Value::String(addr(distributor)));
        }
    }
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Shared test support: sample builders + the conformance-gate helper (mirrors
// `liquid_staking::test_support`). Leaf tests build a representative
// `(body, meta)` and pass it to `assert_conforms`, which composes the per-policy
// schema and STRICTLY checks the lowered context against it.
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::staking::StakeVenue;
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

    /// Curve `VotingEscrow` (veCRV) venue on Ethereum mainnet.
    pub(crate) fn vecrv_venue() -> StakeVenue {
        StakeVenue::CurveVotingEscrow {
            chain: ChainId::ethereum_mainnet(),
            escrow: Address::from_str("0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2").unwrap(),
        }
    }

    /// Curve `Minter` venue on Ethereum mainnet.
    pub(crate) fn minter_venue() -> StakeVenue {
        StakeVenue::CurveMinter {
            chain: ChainId::ethereum_mainnet(),
            minter: Address::from_str("0xd061d61a4d941c39e5453435b6345dc261c2fce0").unwrap(),
        }
    }

    /// Curve `GaugeController` venue on Ethereum mainnet.
    pub(crate) fn gauge_controller_venue() -> StakeVenue {
        StakeVenue::CurveGaugeController {
            chain: ChainId::ethereum_mainnet(),
            controller: Address::from_str("0x2f50d538606fa9edd2b11e2446beb18c9d5846bb").unwrap(),
        }
    }

    /// A Curve liquidity gauge venue (2btc pool gauge) on Ethereum mainnet.
    pub(crate) fn gauge_venue() -> StakeVenue {
        StakeVenue::CurveGauge {
            chain: ChainId::ethereum_mainnet(),
            gauge: Address::from_str("0x5010263ac1978297f56048c7d2b02316a3435404").unwrap(),
        }
    }

    /// CRV `TokenRef` on Ethereum mainnet.
    pub(crate) fn crv() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xd533a949740bb3306d119cc777fa900ba034cd52").unwrap(),
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
    /// and STRICTLY construct the Cedar context against the schema. A wrong
    /// rename / missing required field / wrong type ERRORS here.
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
