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
