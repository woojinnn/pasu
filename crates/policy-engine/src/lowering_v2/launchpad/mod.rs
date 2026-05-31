//! Launchpad-domain lowering: per-action dispatch + the shared `ProtocolRef` /
//! `SaleState` / `VestSchedule` lowerings reused across launchpad leaves.

use serde_json::{Map, Value};

use simulation_reducer::action::launchpad::{LaunchpadAction, SaleState};
use simulation_state::position::{VestCurve, VestSchedule};
use simulation_state::primitives::ProtocolRef;

use super::common::cedar::u256_hex;
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod claim_allocation;
mod claim_vested;
mod commit;
mod refund;
mod withdraw_commit;

/// Dispatch a [`LaunchpadAction`] to its per-action lowering.
///
/// # Errors
///
/// Infallible today — every variant has a leaf lowering — but the `Result`
/// matches the shared per-domain contract so callers stay uniform.
pub(crate) fn lower(
    action: &LaunchpadAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        LaunchpadAction::Commit(a) => commit::lower(a, ctx),
        LaunchpadAction::ClaimAllocation(a) => claim_allocation::lower(a, ctx),
        LaunchpadAction::ClaimVested(a) => claim_vested::lower(a, ctx),
        LaunchpadAction::Refund(a) => refund::lower(a, ctx),
        LaunchpadAction::WithdrawCommit(a) => withdraw_commit::lower(a, ctx),
    }
}

/// Lower a [`ProtocolRef`] → `{ name, version?, chain?, market? }`
/// (`Core::ProtocolRef`). Absent optionals are omitted.
///
/// Shared by `Commit` / `ClaimAllocation` / `Refund` / `WithdrawCommit`, all of
/// which carry a `platform: ProtocolRef`.
pub(crate) fn lower_protocol_ref(protocol: &ProtocolRef) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(protocol.name.clone()));
    if let Some(version) = &protocol.version {
        m.insert("version".into(), Value::String(version.clone()));
    }
    if let Some(chain) = &protocol.chain {
        m.insert("chain".into(), Value::String(chain.to_string()));
    }
    if let Some(market) = &protocol.market {
        m.insert("market".into(), Value::String(market.clone()));
    }
    Value::Object(m)
}

/// Lower a [`SaleState`] → `Launchpad::SaleState`. The `(start, end)`
/// `sale_window` tuple is flattened to `saleWindowStart` / `saleWindowEnd`;
/// absent optionals (`hardCap` / `softCap` / `vestSchedule`) are omitted.
///
/// Shared by `Commit` and `WithdrawCommit`.
pub(crate) fn lower_sale_state(sale: &SaleState) -> Value {
    let mut m = Map::new();
    m.insert("isActive".into(), Value::Bool(sale.is_active));
    m.insert(
        "totalCommitted".into(),
        Value::String(u256_hex(sale.total_committed)),
    );
    if let Some(hard_cap) = sale.hard_cap {
        m.insert("hardCap".into(), Value::String(u256_hex(hard_cap)));
    }
    if let Some(soft_cap) = sale.soft_cap {
        m.insert("softCap".into(), Value::String(u256_hex(soft_cap)));
    }
    m.insert(
        "saleWindowStart".into(),
        Value::from(sale.sale_window.0.as_unix()),
    );
    m.insert(
        "saleWindowEnd".into(),
        Value::from(sale.sale_window.1.as_unix()),
    );
    if let Some(vest) = &sale.vest_schedule {
        m.insert("vestSchedule".into(), lower_vest_schedule(vest));
    }
    Value::Object(m)
}

/// Lower a [`VestSchedule`] → `Launchpad::VestSchedule`. `cliff` / `end` are
/// omitted when absent; `total` is U256 hex.
pub(crate) fn lower_vest_schedule(vest: &VestSchedule) -> Value {
    let mut m = Map::new();
    m.insert("start".into(), Value::from(vest.start.as_unix()));
    if let Some(cliff) = vest.cliff {
        m.insert("cliff".into(), Value::from(cliff.as_unix()));
    }
    if let Some(end) = vest.end {
        m.insert("end".into(), Value::from(end.as_unix()));
    }
    m.insert("curve".into(), lower_vest_curve(&vest.curve));
    m.insert("total".into(), Value::String(u256_hex(vest.total)));
    Value::Object(m)
}

/// Lower a [`VestCurve`] → discriminated `{ kind, … }` (`Launchpad::VestCurve`).
/// The `Stepped.points` `(time, amount)` pairs are flattened to two parallel
/// `Set`s (`steppedPointTimes` / `steppedPointAmounts`) so each index aligns.
fn lower_vest_curve(curve: &VestCurve) -> Value {
    let mut m = Map::new();
    match curve {
        VestCurve::Linear => {
            m.insert("kind".into(), Value::String("linear".into()));
        }
        VestCurve::Stepped { points } => {
            m.insert("kind".into(), Value::String("stepped".into()));
            let times: Vec<Value> = points
                .iter()
                .map(|(t, _)| Value::from(t.as_unix()))
                .collect();
            let amounts: Vec<Value> = points
                .iter()
                .map(|(_, a)| Value::String(u256_hex(*a)))
                .collect();
            m.insert("steppedPointTimes".into(), Value::Array(times));
            m.insert("steppedPointAmounts".into(), Value::Array(amounts));
        }
        VestCurve::Custom { description } => {
            m.insert("kind".into(), Value::String("custom".into()));
            m.insert("description".into(), Value::String(description.clone()));
        }
    }
    Value::Object(m)
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

    use simulation_reducer::action::launchpad::SaleState;
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::position::{VestCurve, VestSchedule};
    use simulation_state::primitives::{Address, ChainId, ProtocolRef, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    use crate::lowering_v2::{lower_action, TxMeta};

    pub(crate) const FROM: &str = "0x1111111111111111111111111111111111111111";
    pub(crate) const TO: &str = "0x2222222222222222222222222222222222222222";

    /// A fixed submission timestamp shared across launchpad samples.
    pub(crate) fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    /// The user/submitter address shared across launchpad samples.
    pub(crate) fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A sample launchpad platform (CoinList on Ethereum mainnet, versioned).
    pub(crate) fn platform() -> ProtocolRef {
        ProtocolRef {
            name: "coinlist".into(),
            version: Some("v1".into()),
            chain: Some(ChainId::ethereum_mainnet()),
            market: None,
        }
    }

    /// A sample ERC20 pay/allocation token (USDC on Ethereum mainnet).
    pub(crate) fn usdc() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    /// A reusable `DataSource` for the live-field samples.
    pub(crate) fn src() -> DataSource {
        DataSource::OracleFeed {
            provider: OracleProvider::Pyth,
            feed_id: "launchpad/sale".into(),
        }
    }

    /// A representative `SaleState`: active, with caps, a sale window, and a
    /// vesting schedule (exercises every optional + the flattened window/curve).
    ///
    /// The vest schedule uses the `Stepped` curve with both `cliff` and `end`
    /// present, so this single value covers: `hardCap`/`softCap`/`vestSchedule`
    /// all PRESENT, `cliff`/`end` PRESENT, and `VestCurve::Stepped`.
    pub(crate) fn sale_state() -> SaleState {
        SaleState {
            is_active: true,
            total_committed: U256::from(1_000_000_000u64),
            hard_cap: Some(U256::from(5_000_000_000u64)),
            soft_cap: Some(U256::from(500_000_000u64)),
            sale_window: (
                Time::from_unix(1_738_000_000),
                Time::from_unix(1_738_600_000),
            ),
            vest_schedule: Some(VestSchedule {
                start: Time::from_unix(1_739_000_000),
                cliff: Some(Time::from_unix(1_739_500_000)),
                end: Some(Time::from_unix(1_745_000_000)),
                curve: VestCurve::Stepped {
                    points: vec![
                        (Time::from_unix(1_740_000_000), U256::from(100u64)),
                        (Time::from_unix(1_742_000_000), U256::from(250u64)),
                    ],
                },
                total: U256::from(1_000u64),
            }),
        }
    }

    /// A MINIMAL `SaleState`: every optional ABSENT (`hard_cap` / `soft_cap` /
    /// `vest_schedule` all `None`). Exercises the omit-branch of every optional
    /// in `lower_sale_state` (the counterpart to [`sale_state`]).
    pub(crate) fn sale_state_minimal() -> SaleState {
        SaleState {
            is_active: false,
            total_committed: U256::ZERO,
            hard_cap: None,
            soft_cap: None,
            sale_window: (
                Time::from_unix(1_738_000_000),
                Time::from_unix(1_738_600_000),
            ),
            vest_schedule: None,
        }
    }

    /// A `SaleState` whose `vest_schedule` is PRESENT but uses the `Linear`
    /// curve with BOTH `cliff` and `end` ABSENT. Exercises `VestCurve::Linear`
    /// plus the omit-branch of `cliff`/`end` in `lower_vest_schedule`.
    pub(crate) fn sale_state_linear_bare_vest() -> SaleState {
        SaleState {
            is_active: true,
            total_committed: U256::from(1_000_000_000u64),
            hard_cap: Some(U256::from(5_000_000_000u64)),
            soft_cap: None,
            sale_window: (
                Time::from_unix(1_738_000_000),
                Time::from_unix(1_738_600_000),
            ),
            vest_schedule: Some(VestSchedule {
                start: Time::from_unix(1_739_000_000),
                cliff: None,
                end: None,
                curve: VestCurve::Linear,
                total: U256::from(2_000u64),
            }),
        }
    }

    /// A `SaleState` whose `vest_schedule` is PRESENT and uses the `Custom`
    /// curve (with `cliff` present, `end` absent — a mixed optional case).
    /// Exercises `VestCurve::Custom`.
    pub(crate) fn sale_state_custom_vest() -> SaleState {
        SaleState {
            is_active: true,
            total_committed: U256::from(1_000_000_000u64),
            hard_cap: None,
            soft_cap: Some(U256::from(500_000_000u64)),
            sale_window: (
                Time::from_unix(1_738_000_000),
                Time::from_unix(1_738_600_000),
            ),
            vest_schedule: Some(VestSchedule {
                start: Time::from_unix(1_739_000_000),
                cliff: Some(Time::from_unix(1_739_500_000)),
                end: None,
                curve: VestCurve::Custom {
                    description: "bespoke milestone unlock".into(),
                },
                total: U256::from(3_000u64),
            }),
        }
    }

    /// An on-chain `ActionMeta` shared across launchpad samples.
    pub(crate) fn onchain_meta() -> ActionMeta {
        ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 7,
                gas_limit: U256::from(150_000u64),
                gas_price: LiveField::new(U256::from(20_000_000_000u64), src(), now()),
                value: U256::ZERO,
            },
        }
    }

    /// THE GATE: synthesize the per-policy schema for `tag`, lower `(body, meta)`
    /// via the public entrypoint, and assert the lowered context conforms
    /// strictly to the action's `*Context` type.
    pub(crate) fn assert_conforms(tag: &str, body: &ActionBody, meta: &ActionMeta) {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": format!("{tag}-schema"),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use simulation_state::position::VestCurve;
    use simulation_state::primitives::{ChainId, ProtocolRef, Time, U256};

    /// A `ProtocolRef` with every optional present emits all four fields; the
    /// off-chain (`chain == None`) case omits `chain`.
    #[test]
    fn protocol_ref_optionals_map_correctly() {
        let full = lower_protocol_ref(&ProtocolRef {
            name: "fjord".into(),
            version: Some("v2".into()),
            chain: Some(ChainId::ethereum_mainnet()),
            market: Some("pool-1".into()),
        });
        assert_eq!(full["name"], serde_json::json!("fjord"));
        assert_eq!(full["version"], serde_json::json!("v2"));
        assert!(full.get("chain").is_some());
        assert_eq!(full["market"], serde_json::json!("pool-1"));

        let offchain = lower_protocol_ref(&ProtocolRef::new("echo"));
        assert_eq!(offchain["name"], serde_json::json!("echo"));
        assert!(offchain.get("version").is_none());
        assert!(offchain.get("chain").is_none());
        assert!(offchain.get("market").is_none());
    }

    /// `VestCurve::Stepped` flattens its `(time, amount)` pairs to two parallel
    /// `Set`s of equal length, index-aligned.
    #[test]
    fn vest_curve_stepped_flattens_to_parallel_sets() {
        let curve = lower_vest_curve(&VestCurve::Stepped {
            points: vec![
                (Time::from_unix(100), U256::from(1u64)),
                (Time::from_unix(200), U256::from(2u64)),
            ],
        });
        assert_eq!(curve["kind"], serde_json::json!("stepped"));
        assert_eq!(curve["steppedPointTimes"], serde_json::json!([100, 200]));
        assert_eq!(
            curve["steppedPointAmounts"],
            serde_json::json!(["0x1", "0x2"])
        );
    }
}
