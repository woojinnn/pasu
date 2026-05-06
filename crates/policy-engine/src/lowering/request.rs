use std::collections::HashSet;

use crate::core::{Action, AmountSpec, TransactionRequest};
use crate::policy::PolicyRequest;
use serde_json::{json, Value};

use super::decimal::add_decimal_strings;

/// Build a `PolicyRequest` from a fully-enriched `Action`. This is the public
/// "Action → Cedar request" conversion used by `Pipeline` lowering.
pub fn request_from_action(action: &Action) -> PolicyRequest {
    let principal = format!(r#"Wallet::"{}""#, action.actor().as_str());
    let action_uid = format!(r#"Action::"{}""#, action.kind());
    let resource = match action {
        Action::Swap(s) => format!(r#"Protocol::"{}""#, s.protocol_id),
        Action::Multi(_) => String::from(r#"Protocol::"multi""#),
        Action::Other { .. } => String::from(r#"Protocol::"unknown""#),
    };
    let entities = action_entities(action);
    let context = action_context(action);
    PolicyRequest::new(principal, action_uid, resource, entities, context)
}

pub fn request_for_tx(
    tx: &TransactionRequest,
    leaves: &[Action],
    leaf_requests: &[PolicyRequest],
) -> PolicyRequest {
    let principal = format!(r#"Wallet::"{}""#, tx.from.as_str());
    let action = r#"Action::"send_tx""#.to_string();
    let resource = format!(r#"Address_::"{}""#, tx.to.as_str());

    let kinds: Vec<String> = leaves.iter().map(|a| a.kind().to_string()).collect();
    let mut protocols_used: Vec<String> = leaves
        .iter()
        .filter_map(|action| {
            if let Action::Swap(s) = action {
                Some(s.protocol_id.clone())
            } else {
                None
            }
        })
        .collect();
    protocols_used.sort_unstable();
    protocols_used.dedup();

    let distinct_recipients = leaves
        .iter()
        .filter_map(|action| {
            if let Action::Swap(s) = action {
                Some(s.recipient.as_str())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>()
        .len() as i64;

    let has_approve = kinds.iter().any(|kind| kind == "approve");
    let has_unknown = kinds.iter().any(|kind| kind == "other");

    let allow_revert_count = leaf_requests
        .iter()
        .filter_map(|req| req.context.get("allowRevert").and_then(Value::as_bool))
        .filter(|v| *v)
        .count() as i64;

    let mut total_input_sum: Option<String> = None;
    for req in leaf_requests {
        let maybe_usd = req
            .context
            .get("inputAmount")
            .and_then(|input| input.get("usd"))
            .and_then(|usd| usd.get("value"))
            .and_then(|value| value.get("__extn"))
            .and_then(|extn| extn.get("arg"))
            .and_then(Value::as_str);
        if let Some(value) = maybe_usd {
            total_input_sum = Some(match total_input_sum {
                Some(prev) => add_decimal_strings(&prev, value),
                None => value.to_string(),
            });
        }
    }

    let mut context = serde_json::Map::new();
    context.insert("chainId".into(), Value::from(tx.chain_id as i64));
    context.insert("from".into(), Value::from(tx.from.as_str()));
    context.insert("to".into(), Value::from(tx.to.as_str()));
    context.insert("valueWei".into(), Value::from(tx.value_wei.clone()));
    context.insert(
        "selector".into(),
        Value::from(tx.selector_hex().unwrap_or_else(|| "0x".into())),
    );
    context.insert("childCount".into(), Value::from(leaves.len() as i64));
    context.insert(
        "kinds".into(),
        Value::Array(kinds.iter().map(|kind| Value::from(kind.clone())).collect()),
    );
    context.insert(
        "protocolsUsed".into(),
        Value::Array(
            protocols_used
                .iter()
                .map(|protocol| Value::from(protocol.as_str()))
                .collect(),
        ),
    );
    context.insert("hasApprove".into(), Value::from(has_approve));
    context.insert("hasUnknown".into(), Value::from(has_unknown));
    context.insert(
        "distinctRecipients".into(),
        Value::from(distinct_recipients),
    );
    context.insert("allowRevertCount".into(), Value::from(allow_revert_count));

    if let Some(total_input_usd) = total_input_sum {
        context.insert(
            "totalInputUsd".into(),
            json!({ "__extn": { "fn": "decimal", "arg": total_input_usd } }),
        );
    }

    let entities = json!([
        { "uid": { "type": "Wallet", "id": tx.from.as_str() }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Address_", "id": tx.to.as_str() }, "attrs": {}, "parents": [] },
    ]);
    PolicyRequest::new(
        principal,
        action,
        resource,
        entities,
        Value::Object(context),
    )
}

/// Build one or more leaf `PolicyRequest`s from an action tree. `Multi`
/// actions are structural: their children are evaluated individually so
/// existing leaf policies such as `action == Action::"swap"` keep working
/// without policy edits.
pub fn requests_from_action(action: &Action) -> Vec<PolicyRequest> {
    match action {
        Action::Multi(m) => requests_from_actions(&m.children),
        Action::Swap(_) | Action::Other { .. } => vec![request_from_action(action)],
    }
}

pub fn requests_from_actions(actions: &[Action]) -> Vec<PolicyRequest> {
    actions.iter().flat_map(requests_from_action).collect()
}

pub(super) fn action_entities(action: &Action) -> Value {
    let resource_id = match action {
        Action::Swap(s) => s.protocol_id.clone(),
        Action::Multi(_) => "multi".into(),
        Action::Other { .. } => "unknown".into(),
    };
    let actor_id = action.actor().as_str();
    json!([
        { "uid": { "type": "Wallet",   "id": actor_id },     "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": resource_id },   "attrs": {}, "parents": [] },
    ])
}

pub(super) fn action_context(action: &Action) -> Value {
    // Optional fields are *omitted* (not set to null) — Cedar has no null
    // type, and policies guard with `context has "field"`.
    match action {
        Action::Swap(s) => {
            let mut m = serde_json::Map::new();
            m.insert("inputAmount".into(), amount_json(&s.input_amount));
            if let Some(min) = &s.min_output_amount {
                m.insert("minOutputAmount".into(), amount_json(min));
            }
            if let Some(fee) = s.fee_bips {
                m.insert("feeBips".into(), Value::from(fee as i64));
            }
            m.insert("target".into(), Value::from(s.target.0.clone()));
            m.insert("recipient".into(), Value::from(s.recipient.0.clone()));
            m.insert("protocolId".into(), Value::from(s.protocol_id.clone()));
            Value::Object(m)
        }
        Action::Multi(m) => json!({
            "target": m.target.0,
            "childCount": m.children.len() as i64,
            "childKinds": m.children.iter().map(|a| Value::from(a.kind())).collect::<Vec<_>>(),
        }),
        Action::Other {
            selector, target, ..
        } => json!({
            "selector": selector,
            "target":   target.0,
        }),
    }
}

pub(super) fn amount_json(a: &AmountSpec) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("tokenSymbol".into(), Value::from(a.token.symbol.clone()));
    m.insert("raw".into(), Value::from(a.raw.clone()));
    if let Some(h) = &a.human {
        m.insert("human".into(), Value::from(h.clone()));
    }
    if let Some(u) = &a.usd {
        m.insert(
            "usd".into(),
            json!({
                "value": { "__extn": { "fn": "decimal", "arg": u.value } },
                "staleSec": u.stale_sec as i64,
            }),
        );
    }
    Value::Object(m)
}
