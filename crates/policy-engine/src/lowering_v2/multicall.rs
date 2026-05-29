//! `ActionBody::Multicall` lowering → `Core::MulticallContext`.
//!
//! Cedar lacks recursive types, so the batch's inner `Vec<ActionBody>` is
//! projected to a FLAT `Set<MulticallChildSummary>` carrying only each direct
//! child's `(domain, action)` discriminators plus a `childCount` summary. The
//! service worker dispatches each child as its own envelope for per-child
//! detail evaluation; this lowering is one level deep (no recursion).

use serde_json::{Map, Value};

use simulation_reducer::action::ActionBody;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an [`ActionBody::Multicall`] into the `Core::MulticallContext` shape.
///
/// Takes the whole [`ActionBody`] (not a domain enum) because `Multicall` is a
/// struct variant on `ActionBody` itself.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] only on the unreachable non-`Multicall`
/// arm (dispatch only routes `Multicall` bodies here); the `Result` matches
/// the shared per-action `lower` contract.
pub(crate) fn lower(
    action: &ActionBody,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let ActionBody::Multicall { actions } = action else {
        return Err(LowerError::Unsupported("multicall".to_owned()));
    };

    let children: Vec<Value> = actions.iter().map(lower_child_summary).collect();

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("childCount".into(), Value::from(actions.len()));
    m.insert("children".into(), Value::Array(children));
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Core::Action::"Multicall""#, Value::Object(m)))
}

/// Project one direct child [`ActionBody`] to a `Core::MulticallChildSummary`
/// `{ domain, action }`. Children that carry no inner action tag
/// (`Multicall` / `Unknown`) fall back to the domain tag so the required
/// `action` String is always populated (mirrors `per_policy`'s
/// `action_tag.unwrap_or(domain)` convention).
fn lower_child_summary(child: &ActionBody) -> Value {
    let view = child.view();
    let mut m = Map::new();
    m.insert("domain".into(), Value::String(view.domain.to_owned()));
    m.insert(
        "action".into(),
        Value::String(view.action_tag.unwrap_or(view.domain).to_owned()),
    );
    Value::Object(m)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::{token, ActionBody, ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::LiveField;

    use crate::lowering_v2::{lower_action, TxMeta};

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// A `Token::Erc20Approve` child — exercises the `Some(action_tag)` branch
    /// of the child summary projection.
    fn approve_child() -> ActionBody {
        let token = TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        ActionBody::Token(token::TokenAction::Erc20Approve(token::Erc20ApproveAction {
            token,
            spender: Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
            amount: U256::from(1_000_000_000u64),
        }))
    }

    /// An `Unknown` child — exercises the `None` action-tag fallback branch
    /// (`action` falls back to the `"unknown"` domain tag).
    fn unknown_child() -> ActionBody {
        ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        }
    }

    /// A two-child multicall (approve + unknown), OnchainTx meta. Covers BOTH
    /// the tagged-child and the untagged-child (None → domain fallback) paths.
    fn sample_multicall() -> (ActionBody, ActionMeta) {
        let body = ActionBody::Multicall {
            actions: vec![approve_child(), unknown_child()],
        };
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 7,
                gas_limit: U256::from(300_000u64),
                gas_price: LiveField::new(
                    U256::from(100_000_000u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "ETH/USD".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        };
        (body, meta)
    }

    /// Synthesize the multicall per-policy schema (core + multicall). NOTE:
    /// `multicall` carries NO inner action tag, so the trigger pins it via
    /// `action.domain` (an `action.tag` `eq` would never match a `None` tag —
    /// see `policy_rpc::trigger`).
    fn multicall_schema_text() -> String {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": "multicall-schema",
            "schema_version": 2,
            "trigger": { "where": { "action.domain": { "eq": "multicall" } } }
        }))
        .unwrap();
        crate::schema::compose_per_policy(&manifest).unwrap()
    }

    /// THE GATE: the lowered `MulticallContext` (meta + childCount + the flat
    /// `{domain, action}` child set) must conform strictly to the schema.
    #[test]
    fn multicall_lowering_conforms_to_schema() {
        let (body, meta) = sample_multicall();
        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();

        assert_eq!(lowered.action_uid, "Core::Action::\"Multicall\"");

        let schema_text = multicall_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("lowered multicall context must conform to Core::MulticallContext");
    }
}
