use policy_engine::{
    ActionAdapterError, Eip712OtherAction, LegacyAction, Permit2PermitKind, SignatureActionAdapter,
    SignatureActionAdapterRegistry, SignatureActionResolverOutcome, SignatureRequest,
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

fn resolved_adapter<'a>(
    registry: &'a impl SignatureActionAdapterRegistry,
    sig: &SignatureRequest,
) -> &'a dyn SignatureActionAdapter {
    match registry.resolve(sig) {
        SignatureActionResolverOutcome::Resolved(adapter) => adapter,
        other => panic!("expected resolved adapter, got {other:?}"),
    }
}

#[test]
fn permit2_permit_single_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_single.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter.build_action(&sig).expect("Permit2 fixture decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
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
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter.build_action(&sig).expect("Permit2 batch decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
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
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("Permit2 transfer decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
    };
    assert_eq!(action.permit_kind, Permit2PermitKind::PermitTransferFrom);
    assert_eq!(action.nonce, "3");
    assert_eq!(action.sig_deadline, 1600);
    assert_eq!(action.expiration, 0);
    assert!(!action.witness_present);
}

#[test]
fn permit2_permit_batch_transfer_from_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_batch_transfer_from.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("Permit2 batch transfer decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
    };
    assert_eq!(
        action.permit_kind,
        Permit2PermitKind::PermitBatchTransferFrom
    );
    assert_eq!(action.approvals.len(), 2);
    assert!(action
        .approvals
        .iter()
        .all(|approval| approval.expiration == 0));
    assert_eq!(action.nonce, "4");
    assert_eq!(action.sig_deadline, 1600);
    assert_eq!(action.token.symbol, "USDC");
    assert_eq!(action.amount, "50000000");
    assert!(!action.witness_present);
}

#[test]
fn permit2_permit_witness_transfer_from_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_witness_transfer_from.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("Permit2 witness transfer decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
    };
    assert_eq!(
        action.permit_kind,
        Permit2PermitKind::PermitWitnessTransferFrom
    );
    assert_eq!(action.approvals.len(), 1);
    assert_eq!(action.nonce, "5");
    assert_eq!(action.sig_deadline, 1600);
    assert_eq!(action.expiration, 0);
    assert!(action.witness_present);
}

#[test]
fn permit2_permit_batch_witness_transfer_from_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("permit2_permit_batch_witness_transfer_from.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("Permit2 batch witness transfer decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
    };
    assert_eq!(
        action.permit_kind,
        Permit2PermitKind::PermitBatchWitnessTransferFrom
    );
    assert_eq!(action.approvals.len(), 2);
    assert_eq!(action.nonce, "6");
    assert_eq!(action.sig_deadline, 1600);
    assert_eq!(action.token.symbol, "USDC");
    assert!(action.witness_present);
}

#[test]
fn eip2612_permit_decodes_to_action() {
    let registry = default_signature_registry();
    let sig = load_signature("eip2612_permit.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("EIP-2612 fixture decodes");

    let LegacyAction::Eip2612(action) = action else {
        panic!("expected LegacyAction::Eip2612");
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
fn eip2612_permit_rejects_owner_when_signer_differs() {
    let registry = default_signature_registry();
    let mut sig = load_signature("eip2612_permit.json");
    sig.typed_data.message["owner"] =
        serde_json::json!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let adapter = resolved_adapter(&registry, &sig);

    let err = adapter
        .build_action(&sig)
        .expect_err("EIP-2612 owner/signer mismatch must fail");

    let ActionAdapterError::BadCalldata(message) = err;
    assert!(message.contains("message.owner"));
    assert!(message.contains("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(message.contains("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
}

#[test]
fn eip2612_permit_allows_owner_when_signer_matches() {
    let registry = default_signature_registry();
    let sig = load_signature("eip2612_permit.json");
    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("EIP-2612 signer/owner match decodes");

    let LegacyAction::Eip2612(action) = action else {
        panic!("expected LegacyAction::Eip2612");
    };
    assert_eq!(
        action.signer.as_str(),
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        action.owner.as_str(),
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn unmatched_eip712_builds_catch_all_action() {
    let registry = default_signature_registry();
    let sig = load_signature("eip712_other_mail.json");
    assert!(matches!(
        registry.resolve(&sig),
        SignatureActionResolverOutcome::NoMatch
    ));

    let action = LegacyAction::Eip712Other(Eip712OtherAction::from_request(&sig));
    let LegacyAction::Eip712Other(action) = action else {
        panic!("expected LegacyAction::Eip712Other");
    };
    assert_eq!(action.primary_type, "Mail");
    assert_eq!(
        action.verifying_contract.as_str(),
        "0x9999999999999999999999999999999999999999"
    );
}

#[test]
fn permit2_primary_type_matches_case_insensitively() {
    let registry = default_signature_registry();
    let mut sig = load_signature("permit2_permit_single.json");
    let permit_single = sig.typed_data.types["PermitSingle"].clone();
    sig.typed_data.primary_type = "permitSingle".into();
    sig.typed_data.types["permitSingle"] = permit_single;

    let adapter = resolved_adapter(&registry, &sig);
    let action = adapter
        .build_action(&sig)
        .expect("lowercase Permit2 fixture decodes");

    let LegacyAction::Permit2(action) = action else {
        panic!("expected LegacyAction::Permit2");
    };
    assert_eq!(action.permit_kind, Permit2PermitKind::PermitSingle);
    assert_eq!(action.primary_type, "permitSingle");
}
