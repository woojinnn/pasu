//! Liquid-staking-domain lowering: per-action dispatch + the shared
//! `StakingVenue` lowering. Mirrors the `lending` layout. Actions carry no live
//! inputs, so contexts are `{ meta, venue, …action fields }` only.

use serde_json::{Map, Value};

use simulation_reducer::action::liquid_staking::{LiquidStakingAction, StakingVenue};

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod claim_withdrawal;
mod request_withdrawal;
mod stake;
mod transfer_shares;
mod unwrap;
mod wrap;

/// Dispatch a [`LiquidStakingAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &LiquidStakingAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        LiquidStakingAction::Stake(a) => stake::lower(a, ctx),
        LiquidStakingAction::Wrap(a) => wrap::lower(a, ctx),
        LiquidStakingAction::Unwrap(a) => unwrap::lower(a, ctx),
        LiquidStakingAction::RequestWithdrawal(a) => request_withdrawal::lower(a, ctx),
        LiquidStakingAction::ClaimWithdrawal(a) => claim_withdrawal::lower(a, ctx),
        LiquidStakingAction::TransferShares(a) => transfer_shares::lower(a, ctx),
    }
}

/// Lower a [`StakingVenue`] → `{ name, chain }` (`LiquidStaking::StakingVenue`).
pub(crate) fn lower_staking_venue(venue: &StakingVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        StakingVenue::Lido { chain } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
    }
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Shared test support: sample builders + the conformance-gate helper (mirrors
// `lending::test_support`). Leaf tests build a representative `(body, meta)` and
// pass it to `assert_conforms`, which composes the per-policy schema and
// STRICTLY checks the lowered context against it.
// ---------------------------------------------------------------------------
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use simulation_reducer::action::liquid_staking::StakingVenue;
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

    /// A `Lido` venue on Ethereum mainnet.
    pub(crate) fn lido_venue() -> StakingVenue {
        StakingVenue::Lido {
            chain: ChainId::ethereum_mainnet(),
        }
    }

    /// Skeleton `LiveField<U256>` for conform tests — host fills the value in
    /// production; here `U256::ZERO` exercises the host-populated live-field path.
    pub(crate) fn live_u256() -> LiveField<U256> {
        LiveField::new(U256::ZERO, oracle_src(), now())
    }

    /// stETH `TokenRef` on Ethereum mainnet.
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
        cedar_policy::Context::from_json_value(lowered.context, Some((&schema, &uid)))
            .unwrap_or_else(|e| panic!("{tag} context must conform: {e:?}"));
    }
}
