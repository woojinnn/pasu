//! `ActionBody::Multicall` lowering → `Core::MulticallContext`.
//!
//! Cedar lacks recursive types, so the batch's inner `Vec<ActionBody>` is
//! projected to a FLAT `Set<MulticallChildSummary>` carrying only each direct
//! child's `(domain, action)` discriminators plus a `childCount` summary. The
//! service worker dispatches each child as its own envelope for per-child
//! detail evaluation; this lowering is one level deep (no recursion).

use serde_json::{Map, Value};

use policy_transition::action::ActionBody;

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
pub(crate) fn lower(action: &ActionBody, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
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

    use serde_json::Value;

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::LiveField;
    use policy_transition::action::{airdrop, token, ActionBody, ActionMeta, ActionNature};

    use crate::lowering_v2::{lower_action, TxMeta};

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    /// OnchainTx meta — the on-chain `meta.nature` branch (the `sample_multicall`
    /// path uses the same; this is shared so every multicall test reuses one
    /// conforming meta regardless of how many / which children it carries.)
    fn onchain_meta() -> ActionMeta {
        ActionMeta {
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
        }
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
        ActionBody::Token(token::TokenAction::Erc20Approve(
            token::Erc20ApproveAction {
                token,
                spender: Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
                amount: U256::from(1_000_000_000u64),
            },
        ))
    }

    /// An `Unknown` child — exercises ONE of the two `None` action-tag fallback
    /// branches (`action` falls back to the `"unknown"` domain tag).
    fn unknown_child() -> ActionBody {
        ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        }
    }

    /// An `Airdrop::Delegate` child — a TAGGED child from a NON-token domain.
    /// Confirms the `domain` discriminator flows through verbatim for a domain
    /// other than `token`, and the `Some(action_tag)` projection (`"delegate"`)
    /// is taken from a fresh enum (not just the one token leaf the existing
    /// `approve_child` exercised).
    fn airdrop_child() -> ActionBody {
        let token = TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984").unwrap(),
            },
        };
        let src = DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984").unwrap(),
            function: "delegates(address)".into(),
            decoder_id: "erc20votes_delegates".into(),
        };
        ActionBody::Airdrop(airdrop::AirdropAction::Delegate(
            airdrop::DelegateGovernanceAction {
                token,
                delegatee: Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
                live_inputs: airdrop::DelegateLiveInputs {
                    current_delegate: LiveField::new(None::<Address>, src.clone(), now()),
                    voting_power: LiveField::new(U256::from(1_000u64), src, now()),
                },
            },
        ))
    }

    /// A nested `Multicall` child — the OTHER `None` action-tag fallback branch
    /// (besides `Unknown`). `action` falls back to the `"multicall"` domain tag.
    /// The flat projection is one level deep: a nested multicall is summarized,
    /// not recursed.
    fn multicall_child() -> ActionBody {
        ActionBody::Multicall {
            actions: vec![approve_child()],
        }
    }

    /// A two-child multicall (approve + unknown), OnchainTx meta. Covers BOTH
    /// the tagged-child and the untagged-child (None → domain fallback) paths.
    fn sample_multicall() -> (ActionBody, ActionMeta) {
        let body = ActionBody::Multicall {
            actions: vec![approve_child(), unknown_child()],
        };
        (body, onchain_meta())
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

    /// THE GATE, factored: lower a `Multicall` body and strictly validate the
    /// resulting `Core::MulticallContext` JSON against the synthesized schema.
    /// Returns the lowered context so per-branch assertions can inspect
    /// `childCount` / `children` directly (the venue/enum/None-tag field lives
    /// in this JSON, so conformance validates that branch end-to-end).
    fn assert_multicall_conforms(body: &ActionBody, meta: &ActionMeta) -> Value {
        let lowered = lower_action(body, meta, &TxMeta { from: FROM, to: TO }).unwrap();
        assert_eq!(lowered.action_uid, "Core::Action::\"Multicall\"");

        let schema_text = multicall_schema_text();
        let (schema, _w) = cedar_policy::Schema::from_cedarschema_str(&schema_text).unwrap();
        let uid: cedar_policy::EntityUid = lowered.action_uid.parse().unwrap();

        cedar_policy::Context::from_json_value(lowered.context.clone(), Some((&schema, &uid)))
            .expect("lowered multicall context must conform to Core::MulticallContext");
        lowered.context
    }

    /// THE GATE: the lowered `MulticallContext` (meta + childCount + the flat
    /// `{domain, action}` child set) must conform strictly to the schema.
    #[test]
    fn multicall_lowering_conforms_to_schema() {
        let (body, meta) = sample_multicall();
        let ctx = assert_multicall_conforms(&body, &meta);
        // Two children: a tagged token child + an untagged Unknown child.
        assert_eq!(ctx["childCount"], Value::from(2));
    }

    /// EMPTY multicall: `childCount` = 0 and `children` is the empty Set. The
    /// `actions.iter().map(...)` produces no entries — the zero-child boundary
    /// the two-child sample never exercised.
    #[test]
    fn multicall_empty_conforms_and_has_zero_children() {
        let body = ActionBody::Multicall { actions: vec![] };
        let ctx = assert_multicall_conforms(&body, &onchain_meta());

        assert_eq!(ctx["childCount"], Value::from(0));
        assert_eq!(
            ctx["children"]
                .as_array()
                .expect("children is an array")
                .len(),
            0,
        );
    }

    /// NESTED-Multicall child: exercises the OTHER `None` action-tag fallback
    /// branch (besides `Unknown`). A `Multicall` child must summarize to
    /// `{ domain: "multicall", action: "multicall" }` — the `unwrap_or(domain)`
    /// taken when `action_tag` is `None` for a non-`unknown` domain.
    #[test]
    fn multicall_nested_child_falls_back_to_multicall_tag() {
        let body = ActionBody::Multicall {
            actions: vec![multicall_child()],
        };
        let ctx = assert_multicall_conforms(&body, &onchain_meta());

        assert_eq!(ctx["childCount"], Value::from(1));
        let children = ctx["children"].as_array().expect("children is an array");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["domain"], Value::from("multicall"));
        // action falls back to the domain tag because action_tag is None.
        assert_eq!(children[0]["action"], Value::from("multicall"));
    }

    /// MANY children spanning VARIED domains + BOTH untagged fallbacks. Exercises:
    ///   - a tagged token child   → { token, erc20_approve }
    ///   - a tagged airdrop child → { airdrop, delegate }   (non-token domain)
    ///   - an untagged Unknown    → { unknown, unknown }
    ///   - a nested Multicall     → { multicall, multicall }
    /// Confirms the per-child projection picks the right `(domain, action)` for
    /// every branch in one bundle, and the larger `childCount` (4) conforms.
    #[test]
    fn multicall_many_varied_children_conform() {
        let body = ActionBody::Multicall {
            actions: vec![
                approve_child(),
                airdrop_child(),
                unknown_child(),
                multicall_child(),
            ],
        };
        let ctx = assert_multicall_conforms(&body, &onchain_meta());

        assert_eq!(ctx["childCount"], Value::from(4));
        let children = ctx["children"].as_array().expect("children is an array");
        // `children` is a Cedar Set: 4 DISTINCT `(domain, action)` pairs survive
        // (no two children share a summary), so the set carries all 4.
        assert_eq!(children.len(), 4);

        let mut pairs: Vec<(String, String)> = children
            .iter()
            .map(|c| {
                (
                    c["domain"].as_str().unwrap().to_owned(),
                    c["action"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("airdrop".to_owned(), "delegate".to_owned()),
                ("multicall".to_owned(), "multicall".to_owned()),
                ("token".to_owned(), "erc20_approve".to_owned()),
                ("unknown".to_owned(), "unknown".to_owned()),
            ],
        );
    }

    /// OffchainSig `meta.nature`: a multicall can be submitted as an off-chain
    /// signature (e.g. a batched intent). The two-child sample only used the
    /// `OnchainTx` nature; this pins the `offchain_sig` meta branch through the
    /// multicall context too (the `meta` slot is shared but venue/none-tag and
    /// nature interact in one JSON object).
    #[test]
    fn multicall_offchain_sig_meta_conforms() {
        use policy_state::NonceKey;
        use policy_transition::action::Eip712Domain;

        let body = ActionBody::Multicall {
            actions: vec![approve_child(), unknown_child()],
        };
        let meta = ActionMeta {
            submitted_at: now(),
            submitter: user(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "Multicall".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: Some(
                        Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
                    ),
                    salt: None,
                },
                deadline: Time::from_unix(1_738_001_800),
                nonce_key: Some(NonceKey::OrderHash {
                    hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                        .into(),
                }),
            },
        };

        let ctx = assert_multicall_conforms(&body, &meta);
        assert_eq!(ctx["meta"]["nature"]["kind"], Value::from("offchain_sig"));
    }
}
