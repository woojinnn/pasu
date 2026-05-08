//! Focused request-lowering tests for action-to-Cedar request conversion.

use policy_engine::{
    request_from_action, Action, Address, DexAction, DexFacts, DexTrace, OtherAction,
};
use serde_json::json;

#[test]
fn dex_action_lowers_to_one_dex_policy_request() {
    let actor = Address::new("0x0000000000000000000000000000000000000001").unwrap();
    let target = Address::new("0x0000000000000000000000000000000000000002").unwrap();
    let action = Action::Dex(DexAction {
        actor: actor.clone(),
        target: target.clone(),
        value_wei: "0".into(),
        facts: DexFacts {
            protocol_ids: vec!["uniswap".into()],
            has_zero_min_output: true,
            ..Default::default()
        },
        oracle_requirements: Vec::new(),
        trace: DexTrace::default(),
    });

    let requests = policy_engine::lowering::requests_from_action(&action)
        .expect("Dex action should lower without host");

    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.action, r#"Action::"dex""#);
    assert_eq!(
        request.principal,
        format!(r#"Wallet::"{}""#, actor.as_str())
    );
    assert_eq!(request.resource, r#"Protocol::"dex""#);
    assert_eq!(request.context["target"], target.as_str());
    assert_eq!(request.context["protocolIds"], json!(["uniswap"]));
    assert_eq!(request.context["hasZeroMinOutput"], true);
    assert!(request.context.get("trace").is_none());
}

#[test]
fn other_action_lowers_to_one_other_policy_request() {
    let actor = Address::new("0x0000000000000000000000000000000000000001").unwrap();
    let target = Address::new("0x0000000000000000000000000000000000000002").unwrap();
    let action = Action::Other(OtherAction {
        actor: actor.clone(),
        target: target.clone(),
        selector: "0xaabbccdd".into(),
        value_wei: "7".into(),
        raw_calldata: "0xaabbccdd00".into(),
    });

    let request = request_from_action(&action).expect("Other action should lower without host");

    assert_eq!(request.action, r#"Action::"other""#);
    assert_eq!(
        request.principal,
        format!(r#"Wallet::"{}""#, actor.as_str())
    );
    assert_eq!(request.resource, r#"Protocol::"unknown""#);
    assert_eq!(request.context["target"], target.as_str());
    assert_eq!(request.context["selector"], "0xaabbccdd");
    assert_eq!(request.context["valueWei"], "7");
    assert_eq!(request.context["rawCalldata"], "0xaabbccdd00");
}
