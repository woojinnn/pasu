//! `ActionBody::Unknown` lowering → `Core::UnknownContext`.
//!
//! Catch-all for calldata no manifest matched. The lowering surfaces the raw
//! `(target, chain, calldata, value)` so policies can still gate the call
//! (typically forbid outright, or whitelist specific `(chain, target)` pairs).

use serde_json::{Map, Value};

use policy_transition::action::ActionBody;

use super::common::cedar::{addr, u256_hex};
use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an [`ActionBody::Unknown`] into the `Core::UnknownContext` shape.
///
/// Takes the whole [`ActionBody`] (not a domain enum) because `Unknown` is a
/// struct variant on `ActionBody` itself.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] only on the unreachable non-`Unknown`
/// arm (dispatch only routes `Unknown` bodies here); the `Result` matches the
/// shared per-action `lower` contract.
pub(crate) fn lower(action: &ActionBody, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let ActionBody::Unknown {
        target,
        chain,
        calldata,
        value,
    } = action
    else {
        return Err(LowerError::Unsupported("unknown".to_owned()));
    };

    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("target".into(), Value::String(addr(target)));
    m.insert("chain".into(), Value::String(chain.to_string()));
    // `calldata` is already a 0x-hex `String` (raw bytes) — emit verbatim.
    m.insert("calldata".into(), Value::String(calldata.clone()));
    m.insert("value".into(), Value::String(u256_hex(*value)));
    // `custom` is OMITTED here — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Core::Action::"Unknown""#, Value::Object(m)))
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

    use serde_json::Value;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::{LiveField, NonceKey};
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};

    use crate::lowering_v2::{lower_action, TxMeta};

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// The raw `Unknown` body reused by every nature variant below — only the
    /// `meta.nature` differs across the conformance tests.
    fn unknown_body() -> ActionBody {
        ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeefcafebabe".into(),
            value: U256::from(1_000_000_000_000_000_000u64),
        }
    }

    /// An unidentified call with raw calldata, OffchainSig meta (exercises the
    /// offchain `meta.nature` branch alongside the raw-call fields).
    fn sample_unknown() -> (ActionBody, ActionMeta) {
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Unknown".into(),
                    version: None,
                    chain_id: Some(1),
                    verifying_contract: None,
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                        .into(),
                }),
            },
        };
        (unknown_body(), meta)
    }

    /// Synthesize the unknown per-policy schema (core + unknown). NOTE: like
    /// multicall, `unknown` carries NO inner action tag, so the trigger pins it
    /// via `action.domain` (an `action.tag` `eq` would never match a `None`
    /// tag — see `policy_rpc::trigger`).
    fn unknown_schema_text() -> String {
        let manifest: crate::policy_rpc::ManifestV2 = serde_json::from_value(serde_json::json!({
            "id": "unknown-schema",
            "schema_version": 2,
            "trigger": { "where": { "action.domain": { "eq": "unknown" } } }
        }))
        .unwrap();
        crate::schema::compose_per_policy(&manifest).unwrap()
    }

    /// THE GATE, factored: lower an `Unknown` body and strictly validate the
    /// resulting `Core::UnknownContext` JSON against the synthesized schema.
    /// Returns the lowered context so per-branch assertions can inspect the
    /// `meta.nature` (and its kind-specific fields) the branch produced.
    fn assert_unknown_conforms(body: &ActionBody, meta: &ActionMeta) -> Value {
        let lowered = lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        assert_eq!(lowered.action_uid, "Core::Action::\"Unknown\"");

        let schema_text = unknown_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("lowered unknown context must conform to Core::UnknownContext");
        lowered.context
    }

    /// THE GATE: the lowered `UnknownContext` (meta + target/chain/calldata/
    /// value) must conform strictly to the schema. Uses the `OffchainSig` nature
    /// with `nonceKey` PRESENT — pins the off-chain branch + the populated
    /// `nonceKey` slot.
    #[test]
    fn unknown_lowering_conforms_to_schema() {
        let (body, meta) = sample_unknown();
        let ctx = assert_unknown_conforms(&body, &meta);
        assert_eq!(ctx["meta"]["nature"]["kind"], Value::from("offchain_sig"));
        // nonce_key = Some → `nonceKey` slot populated.
        assert!(ctx["meta"]["nature"].get("nonceKey").is_some());
    }

    /// `OnchainTx` `meta.nature`: the on-chain branch of `lower_nature`. The
    /// existing sample only used `OffchainSig`, so the lowered context never
    /// carried the `onchain_tx` discriminator + its `gasLimit`/`gasPrice`/
    /// `nonce`/`chain` slots through strict schema validation until now.
    #[test]
    fn unknown_onchain_tx_nature_conforms() {
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 0,
                gas_limit: U256::from(21_000u64),
                gas_price: LiveField::new(
                    U256::from(1u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "gas".into(),
                    },
                    now(),
                ),
                value: U256::ZERO,
            },
        };

        let ctx = assert_unknown_conforms(&unknown_body(), &meta);
        let nature = &ctx["meta"]["nature"];
        assert_eq!(nature["kind"], Value::from("onchain_tx"));
        // The kind-specific slots must be present (omitting any would still
        // conform structurally, so assert they are emitted by the branch).
        assert!(nature.get("gasLimit").is_some());
        assert!(nature.get("gasPrice").is_some());
        assert_eq!(nature["nonce"], Value::from(0));
    }

    /// `OffchainSig` with `nonce_key: None` AND every `Eip712Domain` optional
    /// PRESENT (`version` / `verifyingContract` / `salt`), `chainId` ABSENT.
    /// Complements `sample_unknown` (which had `nonce_key: Some`, `chainId:
    /// Some`, and the other optionals absent), so between the two every
    /// `lower_eip712` Option arm — and the `nonceKey` omitted branch — is hit.
    #[test]
    fn unknown_offchain_sig_no_nonce_key_full_domain_conforms() {
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Unknown".into(),
                    version: Some("2".into()),
                    chain_id: None,
                    verifying_contract: Some(
                        Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
                    ),
                    salt: Some(
                        "0xfeed0000000000000000000000000000000000000000000000000000000000ff".into(),
                    ),
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: None,
            },
        };

        let ctx = assert_unknown_conforms(&unknown_body(), &meta);
        let nature = &ctx["meta"]["nature"];
        assert_eq!(nature["kind"], Value::from("offchain_sig"));
        // nonce_key = None → `nonceKey` slot omitted entirely (never null).
        assert!(nature.get("nonceKey").is_none());

        let domain = &nature["domain"];
        // version / verifyingContract / salt present; chainId omitted.
        assert_eq!(domain["version"], Value::from("2"));
        assert!(domain.get("verifyingContract").is_some());
        assert!(domain.get("salt").is_some());
        assert!(domain.get("chainId").is_none());
    }
}
