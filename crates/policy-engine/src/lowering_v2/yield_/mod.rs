//! Yield-domain lowering: per-action dispatch + the shared `YieldVenue`
//! lowering. Mirrors the `liquid_staking` layout. P1a actions carry no live
//! inputs, so contexts are `{ meta, venue, …action fields }` only (the
//! market→PT/YT/maturity enrichment is added in P1c).

use serde_json::{Map, Value};

use policy_transition::action::yield_::{YieldAction, YieldVenue};

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod add_market_liquidity;
mod cancel_limit_order;
mod claim_yield;
mod mint_py;
mod mint_sy;
mod pt_swap;
mod redeem_py;
mod redeem_sy;
mod remove_market_liquidity;
mod sign_limit_order;
mod yt_swap;

/// Dispatch a [`YieldAction`] to its per-action lowering.
///
/// # Errors
///
/// Per-action lowerings are infallible today, but the `Result` matches the
/// shared per-action `lower` contract so the dispatch stays uniform.
pub(crate) fn lower(action: &YieldAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    match action {
        YieldAction::PtSwap(a) => pt_swap::lower(a, ctx),
        YieldAction::YtSwap(a) => yt_swap::lower(a, ctx),
        YieldAction::AddMarketLiquidity(a) => add_market_liquidity::lower(a, ctx),
        YieldAction::RemoveMarketLiquidity(a) => remove_market_liquidity::lower(a, ctx),
        YieldAction::MintPy(a) => mint_py::lower(a, ctx),
        YieldAction::RedeemPy(a) => redeem_py::lower(a, ctx),
        YieldAction::MintSy(a) => mint_sy::lower(a, ctx),
        YieldAction::RedeemSy(a) => redeem_sy::lower(a, ctx),
        YieldAction::ClaimYield(a) => claim_yield::lower(a, ctx),
        YieldAction::SignLimitOrder(a) => sign_limit_order::lower(a, ctx),
        YieldAction::CancelLimitOrder(a) => cancel_limit_order::lower(a, ctx),
    }
}

/// Lower a [`YieldVenue`] → `{ name, chain }` (`Yield::YieldVenue`).
pub(crate) fn lower_yield_venue(venue: &YieldVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        YieldVenue::PendleV2 { chain } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
    }
    Value::Object(m)
}

/// Lower a unit enum (direction / kind discriminant) to its serde snake_case
/// tag as a JSON string. serde is the source of truth, so this never drifts
/// from the `#[serde(rename_all = "snake_case")]` mapping.
pub(crate) fn enum_tag<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
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

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::LiveField;
    use policy_transition::action::yield_::YieldVenue;
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature};

    use crate::lowering_v2::TxMeta;

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x888888888889758f76e7103c6cbf23abbf58f946";

    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    pub(crate) fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn oracle_src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        }
    }

    /// A `Pendle V2` venue on Ethereum mainnet.
    pub(crate) fn pendle_venue() -> YieldVenue {
        YieldVenue::PendleV2 {
            chain: ChainId::ethereum_mainnet(),
        }
    }

    /// A sample Pendle market / YT / SY address.
    pub(crate) fn pendle_market() -> Address {
        Address::from_str("0xcfd848b9f6fef552204014ac67901223ad6bf679").unwrap()
    }

    /// A sample external `TokenRef` (USDC) on Ethereum mainnet.
    pub(crate) fn usdc() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    /// Skeleton `LiveField<Address>` for conform tests — the host fills the real
    /// value (market SY/PT/YT) at sync time, so this is a zero-address stand-in.
    pub(crate) fn live_addr() -> LiveField<Address> {
        LiveField::new(
            Address::from_str("0x0000000000000000000000000000000000000000").unwrap(),
            oracle_src(),
            now(),
        )
    }

    /// Skeleton `LiveField<U256>` for conform tests — host fills the maturity.
    pub(crate) fn live_u256() -> LiveField<U256> {
        LiveField::new(U256::ZERO, oracle_src(), now())
    }

    /// An on-chain-transaction `ActionMeta` (Ethereum mainnet).
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 7,
                gas_limit: U256::from(300_000u64),
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
