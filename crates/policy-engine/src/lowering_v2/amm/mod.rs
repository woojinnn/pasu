//! AMM-domain lowering: per-action dispatch + the shared `AmmVenue` lowering.

use serde_json::{Map, Value};

use policy_state::primitives::U256;
use policy_state::token::TokenRef;
use policy_transition::action::amm::{AmmAction, AmmVenue, BalancerPoolType, IntentVenue};

use super::common::cedar::{addr, u256_hex};
use super::common::token::lower_token_ref;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod add_liquidity;
mod cancel_intent_order;
mod collect_fees;
mod gsm_swap;
mod remove_liquidity;
mod settle_intent_order;
mod sign_intent_order;
mod swap;

/// Dispatch an [`AmmAction`] to its per-action lowering.
///
/// # Errors
///
/// Propagates the per-action `lower` result (infallible today).
pub(crate) fn lower(action: &AmmAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    match action {
        AmmAction::Swap(a) => swap::lower(a, ctx),
        AmmAction::GsmSwap(a) => gsm_swap::lower(a, ctx),
        AmmAction::AddLiquidity(a) => add_liquidity::lower(a, ctx),
        AmmAction::RemoveLiquidity(a) => remove_liquidity::lower(a, ctx),
        AmmAction::CollectFees(a) => collect_fees::lower(a, ctx),
        AmmAction::SignIntentOrder(a) => sign_intent_order::lower(a, ctx),
        AmmAction::SettleIntentOrder(a) => settle_intent_order::lower(a, ctx),
        AmmAction::CancelIntentOrder(a) => cancel_intent_order::lower(a, ctx),
    }
}

/// Lower an [`AmmVenue`] → `{ name, chain, <per-variant optional fields> }`
/// (`Amm::AmmVenue`). Only the fields a variant carries are emitted.
// Flat exhaustive dispatch over 11 venue variants: splitting it would hide the
// 1:1 variant→field-set mapping that is the whole point of this function.
#[allow(clippy::too_many_lines)]
pub(crate) fn lower_amm_venue(venue: &AmmVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        AmmVenue::UniswapV2 {
            chain,
            pool,
            factory,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("factory".into(), Value::String(addr(factory)));
        }
        AmmVenue::UniswapV3 {
            chain,
            pool,
            fee_tier_bp,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("feeTierBp".into(), Value::from(i64::from(*fee_tier_bp)));
        }
        AmmVenue::UniswapV4 {
            chain,
            pool_id,
            pool_manager,
            hooks,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert("poolManager".into(), Value::String(addr(pool_manager)));
            m.insert("hooks".into(), Value::String(addr(hooks)));
        }
        // SushiV2 / CurveV2 / MaverickV2 all expose only `{ chain, pool }`; the
        // discriminating `name` is already set above, so they share one arm.
        AmmVenue::SushiV2 { chain, pool }
        | AmmVenue::CurveV2 { chain, pool }
        | AmmVenue::MaverickV2 { chain, pool } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
        }
        AmmVenue::CurveV1 {
            chain,
            pool,
            n_coins,
            is_meta,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pool".into(), Value::String(addr(pool)));
            m.insert("nCoins".into(), Value::from(i64::from(*n_coins)));
            m.insert("isMeta".into(), Value::Bool(*is_meta));
        }
        AmmVenue::BalancerV2 {
            chain,
            vault,
            pool_id,
            pool_type,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("vault".into(), Value::String(addr(vault)));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert(
                "poolType".into(),
                Value::String(balancer_pool_type(pool_type).into()),
            );
        }
        AmmVenue::BalancerV3 {
            chain,
            pool_id,
            pool_type,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("poolId".into(), Value::String(pool_id.clone()));
            m.insert(
                "poolType".into(),
                Value::String(balancer_pool_type(pool_type).into()),
            );
        }
        AmmVenue::TraderJoeLB {
            chain,
            pair,
            bin_step,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("pair".into(), Value::String(addr(pair)));
            m.insert("binStep".into(), Value::from(i64::from(*bin_step)));
        }
        AmmVenue::AaveGsm { chain, gsm } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("gsm".into(), Value::String(addr(gsm)));
        }
        AmmVenue::AggregatorRoute {
            chain,
            router,
            route_hash,
            executor,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("router".into(), Value::String(addr(router)));
            m.insert("routeHash".into(), Value::String(route_hash.clone()));
            if let Some(executor) = executor {
                m.insert("executor".into(), Value::String(addr(executor)));
            }
        }
    }
    Value::Object(m)
}

/// Map a [`BalancerPoolType`] to its `snake_case` cedarschema `poolType` spelling.
const fn balancer_pool_type(pool_type: &BalancerPoolType) -> &'static str {
    match pool_type {
        BalancerPoolType::Weighted => "weighted",
        BalancerPoolType::Stable => "stable",
        BalancerPoolType::ComposableStable => "composable_stable",
        BalancerPoolType::MetaStable => "meta_stable",
        BalancerPoolType::LiquidityBootstrapping => "liquidity_bootstrapping",
        BalancerPoolType::Linear => "linear",
    }
}

/// Lower an [`IntentVenue`] → `{ name, chain, reactor?, settlement? }`
/// (`Amm::IntentVenue`). Shared by `SignIntentOrder` / `CancelIntentOrder`.
/// Only `UniswapX` carries `reactor`; only `CowSwap` carries `settlement`;
/// `OneInchLimitOrder` carries `verifyingContract`; `OneInchFusion` / `Bebop`
/// expose only `{ name, chain }`.
pub(crate) fn lower_intent_venue(venue: &IntentVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    match venue {
        IntentVenue::UniswapX { chain, reactor } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("reactor".into(), Value::String(addr(reactor)));
        }
        IntentVenue::CowSwap { chain, settlement } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("settlement".into(), Value::String(addr(settlement)));
        }
        IntentVenue::OneInchFusion { chain } | IntentVenue::Bebop { chain } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
        IntentVenue::OneInchLimitOrder {
            chain,
            verifying_contract,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert(
                "verifyingContract".into(),
                Value::String(addr(verifying_contract)),
            );
        }
    }
    Value::Object(m)
}

/// Lower a `Vec<(TokenRef, U256)>` → `Set<{ token, amount }>` (order lost).
/// Shared by `AddLiquidity` (pooled `tokens`), `RemoveLiquidity`
/// (`pooled_burn.minOut` + `feesOwed`), and `CollectFees` (`feesOwed`).
pub(crate) fn lower_token_amount_set(items: &[(TokenRef, U256)]) -> Value {
    let arr = items
        .iter()
        .map(|(token, amount)| {
            let mut e = Map::new();
            e.insert("token".into(), lower_token_ref(token));
            e.insert("amount".into(), Value::String(u256_hex(*amount)));
            Value::Object(e)
        })
        .collect();
    Value::Array(arr)
}

/// Lower a `(U256, U256)` amount pair → `{ a, b }` (both U256 hex). Shared by
/// `AddLiquidity` (`amountDesired` / `amountMin`) and `RemoveLiquidity`
/// (`concentrated_decrease.amountMin`).
pub(crate) fn lower_amount_pair(pair: &(U256, U256)) -> Value {
    let mut m = Map::new();
    m.insert("a".into(), Value::String(u256_hex(pair.0)));
    m.insert("b".into(), Value::String(u256_hex(pair.1)));
    Value::Object(m)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId};

    /// The merged `{chain, pool}` venue group emits only chain + pool (no extra
    /// venue fields leak in), and `CurveV1` carries `nCoins` (Long) + `isMeta`
    /// (Bool).
    #[test]
    fn amm_venue_merged_group_and_curve_v1_map_correctly() {
        let chain = ChainId::ethereum_mainnet();
        let contract = Address::from_str("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599").unwrap();

        // Merged `{chain, pool}` venue group: SushiV2 emits name "sushi_v2"
        // with only chain + pool.
        let sushi = lower_amm_venue(&AmmVenue::SushiV2 {
            chain: chain.clone(),
            pool: contract,
        });
        assert_eq!(sushi["name"], serde_json::json!("sushi_v2"));
        assert_eq!(sushi["pool"], serde_json::json!(format!("{contract:#x}")));
        assert!(sushi.get("factory").is_none());
        assert!(sushi.get("feeTierBp").is_none());

        // CurveV1 carries nCoins (Long) + isMeta (Bool).
        let curve = lower_amm_venue(&AmmVenue::CurveV1 {
            chain,
            pool: contract,
            n_coins: 3,
            is_meta: true,
        });
        assert_eq!(curve["name"], serde_json::json!("curve_v1"));
        assert_eq!(curve["nCoins"], serde_json::json!(3));
        assert_eq!(curve["isMeta"], serde_json::json!(true));
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

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
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

    /// The recipient address used across AMM leaf samples.
    pub(crate) fn user() -> Address {
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
