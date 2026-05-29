//! AMM-domain lowering: per-action dispatch + the shared `AmmVenue` lowering.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::{AmmAction, AmmVenue, BalancerPoolType};

use super::common::cedar::addr;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod swap;

/// Dispatch an [`AmmAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for AMM actions not yet implemented.
pub(crate) fn lower(action: &AmmAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    match action {
        AmmAction::Swap(a) => swap::lower(a, ctx),
        other => Err(LowerError::Unsupported(format!("amm/{}", other.action_tag()))),
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
        AmmVenue::AggregatorRoute {
            chain,
            router,
            route_hash,
        } => {
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("router".into(), Value::String(addr(router)));
            m.insert("routeHash".into(), Value::String(route_hash.clone()));
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_state::primitives::{Address, ChainId};

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
