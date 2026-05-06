//! `Action::Other` to `PolicyRequest` conversion.

use crate::core::OtherAction;
use crate::policy::PolicyRequest;
use serde_json::{json, Value};

pub(super) fn request(action: &OtherAction) -> PolicyRequest {
    let principal = format!(r#"Wallet::"{}""#, action.actor.as_str());
    let action_uid = r#"Action::"other""#.to_string();
    let resource = r#"Protocol::"unknown""#.to_string();
    let entities = json!([
        { "uid": { "type": "Wallet",   "id": action.actor.as_str() },   "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": "unknown" },   "attrs": {}, "parents": [] },
    ]);
    PolicyRequest::new(principal, action_uid, resource, entities, context(action))
}

fn context(action: &OtherAction) -> Value {
    json!({
        "selector": &action.selector,
        "target": action.target.as_str(),
        "valueWei": &action.value_wei,
        "rawCalldata": &action.raw_calldata,
    })
}
