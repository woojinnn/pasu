//! `#[wasm_bindgen]` trigger pre-filter export.
//!
//! The host calls [`evaluate_triggers_json`] AFTER decoding a transaction into
//! a `policy_transition::action::ActionBody` but BEFORE any policy-rpc call,
//! to learn which installed policies' triggers match — so only those policies'
//! enrichment runs and only they are evaluated.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

use policy_engine::policy_rpc::{evaluate_trigger, ManifestV2, TriggerScope, TxView};
use policy_transition::action::ActionBody;

/// Input: the installed manifests (v2), the decoded action, and tx metadata.
#[derive(Deserialize)]
struct EvaluateTriggersInput {
    manifests: Vec<ManifestV2>,
    action: ActionBody,
    tx: TxInput,
}

impl EvaluateTriggersInput {
    /// 트리거의 tx.from/tx.to eq/in 비교도 엔진 표기(소문자) 기준 — checksum
    /// 케이스 입력을 정규화한다 (action_eval_exports::TxInput::normalize와 동일).
    fn normalize(&mut self) {
        self.tx.from = self.tx.from.to_lowercase();
        self.tx.to = self.tx.to.to_lowercase();
    }
}

/// Transaction-level trigger fields.
#[derive(Deserialize)]
struct TxInput {
    chain_id: String,
    from: String,
    to: String,
}

/// Output: ids of the manifests whose trigger matched.
#[derive(Serialize)]
struct EvaluateTriggersOutput {
    matched_ids: Vec<String>,
}

/// Return the ids of the manifests whose [`Trigger`](policy_engine::policy_rpc::Trigger)
/// matches the decoded action.
///
/// Scope handling: `outer` matches against the outer action's view; `inner`
/// (default) matches a `Multicall` if ANY inner child matches, otherwise the
/// action's own view. Returns `{"matched_ids":[...]}` or `{"error":"..."}`.
#[wasm_bindgen]
#[must_use]
pub fn evaluate_triggers_json(input_json: String) -> String {
    match run(&input_json) {
        Ok(out) => serde_json::to_string(&out)
            .unwrap_or_else(|e| error_json(&format!("serialize output: {e}"))),
        Err(e) => error_json(&e),
    }
}

fn run(input_json: &str) -> Result<EvaluateTriggersOutput, String> {
    let mut input: EvaluateTriggersInput =
        serde_json::from_str(input_json).map_err(|e| format!("invalid input json: {e}"))?;
    input.normalize();
    let tx = TxView {
        chain_id: &input.tx.chain_id,
        from: &input.tx.from,
        to: &input.tx.to,
    };
    let matched_ids = input
        .manifests
        .iter()
        .filter(|m| manifest_matches(m, &input.action, &tx))
        .map(|m| m.id.clone())
        .collect();
    Ok(EvaluateTriggersOutput { matched_ids })
}

fn manifest_matches(manifest: &ManifestV2, action: &ActionBody, tx: &TxView<'_>) -> bool {
    match manifest.trigger.scope {
        TriggerScope::Outer => evaluate_trigger(&manifest.trigger, &action.view(), tx),
        TriggerScope::Inner => match action {
            // Per-inner-child: a Multicall matches if ANY child matches.
            ActionBody::Multicall { actions } => actions
                .iter()
                .any(|child| evaluate_trigger(&manifest.trigger, &child.view(), tx)),
            other => evaluate_trigger(&manifest.trigger, &other.view(), tx),
        },
    }
}

fn error_json(message: &str) -> String {
    // `message` is plain text; serialize it as a JSON string for safe quoting.
    let quoted = serde_json::to_string(message).unwrap_or_else(|_| "\"error\"".to_owned());
    format!("{{\"error\":{quoted}}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::primitives::{Address, ChainId, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_transition::action::token::{Erc20ApproveAction, TokenAction};
    use serde_json::{json, Value};
    use std::str::FromStr;

    fn approve() -> ActionBody {
        let token = TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        };
        ActionBody::Token(TokenAction::Erc20Approve(Erc20ApproveAction {
            token,
            spender: Address::from_str("0x00000000000000000000000000000000deadbeef").unwrap(),
            amount: U256::from(1u64),
        }))
    }

    fn matched(input: Value) -> Vec<String> {
        let out = evaluate_triggers_json(input.to_string());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("error").is_none(), "unexpected error: {out}");
        v["matched_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect()
    }

    fn tx() -> Value {
        json!({ "chain_id": "eip155:1", "from": "0x01", "to": "0x02" })
    }

    #[test]
    fn matches_by_action_tag_and_filters_others() {
        let ids = matched(json!({
            "manifests": [
                { "id": "approve-policy", "schema_version": 2,
                  "trigger": { "where": { "action.tag": { "eq": "erc20_approve" } } } },
                { "id": "swap-policy", "schema_version": 2,
                  "trigger": { "where": { "action.tag": { "eq": "swap" } } } }
            ],
            "action": approve(),
            "tx": tx(),
        }));
        assert_eq!(ids, vec!["approve-policy".to_owned()]);
    }

    #[test]
    fn empty_trigger_always_matches() {
        let ids = matched(json!({
            "manifests": [ { "id": "always", "schema_version": 2 } ],
            "action": approve(),
            "tx": tx(),
        }));
        assert_eq!(ids, vec!["always".to_owned()]);
    }

    #[test]
    fn inner_scope_multicall_matches_if_any_child_matches() {
        let action = ActionBody::Multicall {
            actions: vec![approve()],
        };
        let ids = matched(json!({
            "manifests": [
                { "id": "approve-policy", "schema_version": 2,
                  "trigger": { "where": { "action.tag": { "eq": "erc20_approve" } } } },
                { "id": "swap-policy", "schema_version": 2,
                  "trigger": { "where": { "action.tag": { "eq": "swap" } } } }
            ],
            "action": action,
            "tx": tx(),
        }));
        assert_eq!(ids, vec!["approve-policy".to_owned()]);
    }

    #[test]
    fn outer_scope_matches_multicall_domain() {
        let action = ActionBody::Multicall {
            actions: vec![approve()],
        };
        let ids = matched(json!({
            "manifests": [
                { "id": "bundle-watch", "schema_version": 2,
                  "trigger": { "scope": "outer",
                               "where": { "action.domain": { "eq": "multicall" } } } },
                // inner-scope domain==multicall never matches (children aren't multicall)
                { "id": "inner-multicall", "schema_version": 2,
                  "trigger": { "where": { "action.domain": { "eq": "multicall" } } } }
            ],
            "action": action,
            "tx": tx(),
        }));
        assert_eq!(ids, vec!["bundle-watch".to_owned()]);
    }

    #[test]
    fn invalid_input_returns_error_json() {
        let out = evaluate_triggers_json("not json".to_owned());
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["error"].as_str().unwrap().contains("invalid input json"));
    }
}
