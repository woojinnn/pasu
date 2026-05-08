use policy_engine::PolicyEngine;
use std::{fs, path::Path};

#[test]
fn dex_policy_schema_accepts_aggregate_context() {
    let policy = r#"
        permit(
          principal is Wallet,
          action == Action::"dex",
          resource is Protocol
        )
        when {
          context has totalInputUsd &&
          context.totalInputUsd.value.lessThanOrEqual(decimal("100.00")) &&
          !context.hasZeroMinOutput &&
          context has allowancesCoverInputs &&
          context.allowancesCoverInputs &&
          context has maxFeeBps &&
          context.maxFeeBps <= 100
        };
    "#;

    // builder() pre-loads the bundled schema; no explicit add_schema_text needed.
    PolicyEngine::builder().add_text(policy).build().unwrap();
}

#[test]
fn shipped_dex_policies_validate_against_composed_schema() {
    let policy_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policies/dex");
    let entries = fs::read_dir(&policy_dir).unwrap_or_else(|err| {
        panic!(
            "failed to read shipped dex policy dir {}: {err}",
            policy_dir.display()
        )
    });
    let mut policy_paths = entries
        .map(|entry| {
            entry
                .expect("failed to read shipped policy dir entry")
                .path()
        })
        .filter(|path| path.extension().is_some_and(|ext| ext == "cedar"))
        .collect::<Vec<_>>();
    policy_paths.sort();

    assert!(
        !policy_paths.is_empty(),
        "expected shipped dex policies in {}",
        policy_dir.display()
    );

    for policy_path in policy_paths {
        let policy = fs::read_to_string(&policy_path).unwrap_or_else(|err| {
            panic!(
                "failed to read shipped policy {}: {err}",
                policy_path.display()
            )
        });
        PolicyEngine::builder()
            .add_text(policy)
            .build()
            .unwrap_or_else(|err| {
                panic!(
                    "shipped policy {} failed schema validation: {err}",
                    policy_path.display()
                )
            });
    }
}

#[test]
fn signature_policy_schemas_accept_v1_contexts() {
    let policy = r#"
        permit(
          principal is Wallet,
          action == Action::"signature.permit2",
          resource is Protocol
        )
        when {
          context.signer == "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" &&
          context.requestChainId == context.domainChainId &&
          context.permitKind == "PermitSingle" &&
          context.token.symbol == "USDC" &&
          context.amountHuman.lessThanOrEqual(decimal("50.0000")) &&
          context.sigDeadlineDeltaSec <= 3600 &&
          context.nonceValid &&
          !context.isUnlimited &&
          context.approvalCount >= 1 &&
          context has totalApprovedUsd &&
          context.totalApprovedUsd.value.lessThanOrEqual(decimal("100.00"))
        };

        permit(
          principal is Wallet,
          action == Action::"signature.eip2612",
          resource is Protocol
        )
        when {
          context.owner == context.signer &&
          context.spender == "0x1111111111111111111111111111111111111111" &&
          context.token.symbol == "USDC" &&
          context.valueHuman.lessThanOrEqual(decimal("50.0000")) &&
          context.deadlineDeltaSec <= 3600 &&
          context.nonceValid &&
          !context.isUnlimited &&
          context has totalApprovedUsd
        };

        permit(
          principal is Wallet,
          action == Action::"signature.eip712_other",
          resource is Protocol
        )
        when {
          context.domainName == "Example Mail" &&
          context.domainVersion == "1" &&
          context.domainSalt == "" &&
          context.primaryType == "Mail" &&
          context.typesJson != "" &&
          context.messageJson != ""
        };
    "#;

    PolicyEngine::builder().add_text(policy).build().unwrap();
}

#[test]
fn shipped_signature_policies_validate_against_composed_schema() {
    let policy_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policies/signature");
    let entries = fs::read_dir(&policy_dir).unwrap_or_else(|err| {
        panic!(
            "failed to read shipped signature policy dir {}: {err}",
            policy_dir.display()
        )
    });
    let mut policy_paths = Vec::new();
    for entry in entries {
        let path = entry
            .expect("failed to read shipped policy dir entry")
            .path();
        if path.is_dir() {
            for nested in fs::read_dir(&path).unwrap_or_else(|err| {
                panic!(
                    "failed to read shipped signature policy subdir {}: {err}",
                    path.display()
                )
            }) {
                let nested_path = nested
                    .expect("failed to read shipped policy subdir entry")
                    .path();
                if nested_path.extension().is_some_and(|ext| ext == "cedar") {
                    policy_paths.push(nested_path);
                }
            }
        }
    }
    policy_paths.sort();

    assert!(
        !policy_paths.is_empty(),
        "expected shipped signature policies in {}",
        policy_dir.display()
    );

    for policy_path in policy_paths {
        let policy = fs::read_to_string(&policy_path).unwrap_or_else(|err| {
            panic!(
                "failed to read shipped policy {}: {err}",
                policy_path.display()
            )
        });
        PolicyEngine::builder()
            .add_text(policy)
            .build()
            .unwrap_or_else(|err| {
                panic!(
                    "shipped policy {} failed schema validation: {err}",
                    policy_path.display()
                )
            });
    }
}
