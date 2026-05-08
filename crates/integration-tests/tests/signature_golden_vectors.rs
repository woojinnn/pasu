use policy_engine::{
    Action, Eip712OtherAction, Permit2PermitKind, SignatureRegistry, SignatureRequest,
};
use policy_engine_adapters_bundle::default_signature_registry;
use std::{fs, path::Path};

fn load_signature(name: &str) -> SignatureRequest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/signatures")
        .join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse fixture {}: {err}", path.display()))
}

#[test]
fn permit2_permit_single_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_single.json");
    let adapter = registry.resolve(&sig).expect("Permit2 adapter resolves");
    let action = adapter.build(&sig).expect("Permit2 fixture decodes");

    let Action::Permit2(action) = action else {
        panic!("expected Action::Permit2");
    };
    assert_eq!(action.permit_kind, Permit2PermitKind::PermitSingle);
    assert_eq!(
        action.spender.as_str(),
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(action.amount, "10000000000000000");
    assert!(!action.is_unlimited);
    assert!(action.nonce_valid);
}

#[test]
fn permit2_permit_batch_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_batch.json");
    let adapter = registry.resolve(&sig).expect("Permit2 adapter resolves");
    let action = adapter.build(&sig).expect("Permit2 batch decodes");

    let Action::Permit2(action) = action else {
        panic!("expected Action::Permit2");
    };
    assert_eq!(action.permit_kind, Permit2PermitKind::PermitBatch);
    assert_eq!(action.approvals.len(), 2);
    assert_eq!(action.token.symbol, "USDC");
    assert_eq!(action.amount, "50000000");
    assert_eq!(action.sig_deadline, 1600);
}

#[test]
fn permit2_permit_transfer_from_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_transfer_from.json");
    let adapter = registry.resolve(&sig).expect("Permit2 adapter resolves");
    let action = adapter.build(&sig).expect("Permit2 transfer decodes");

    let Action::Permit2(action) = action else {
        panic!("expected Action::Permit2");
    };
    assert_eq!(action.permit_kind, Permit2PermitKind::PermitTransferFrom);
    assert_eq!(action.nonce, "3");
    assert_eq!(action.sig_deadline, 1600);
}

#[test]
fn eip2612_permit_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("eip2612_permit.json");
    let adapter = registry.resolve(&sig).expect("EIP-2612 adapter resolves");
    let action = adapter.build(&sig).expect("EIP-2612 fixture decodes");

    let Action::Eip2612(action) = action else {
        panic!("expected Action::Eip2612");
    };
    assert_eq!(
        action.spender.as_str(),
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(
        action.owner.as_str(),
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(action.value, "50000000");
    assert_eq!(action.deadline, 1600);
    assert!(!action.is_unlimited);
    assert!(action.nonce_valid);
}

#[test]
fn eip2612_permit_decodes_owner_from_message_when_signer_differs() {
    let registry = default_signature_registry();
    let mut sig = load_signature("eip2612_permit.json");
    sig.typed_data.message["owner"] =
        serde_json::json!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let adapter = registry.resolve(&sig).expect("EIP-2612 adapter resolves");
    let action = adapter.build(&sig).expect("EIP-2612 fixture decodes");

    let Action::Eip2612(action) = action else {
        panic!("expected Action::Eip2612");
    };
    assert_eq!(
        action.signer.as_str(),
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        action.owner.as_str(),
        "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
}

#[test]
fn unmatched_eip712_builds_catch_all_action() {
    let registry = default_signature_registry();
    let sig = load_signature("eip712_other_mail.json");
    assert!(registry.resolve(&sig).is_none());

    let action = Action::Eip712Other(Eip712OtherAction::from_request(&sig));
    let Action::Eip712Other(action) = action else {
        panic!("expected Action::Eip712Other");
    };
    assert_eq!(action.primary_type, "Mail");
    assert_eq!(
        action.verifying_contract.as_str(),
        "0x9999999999999999999999999999999999999999"
    );
}
