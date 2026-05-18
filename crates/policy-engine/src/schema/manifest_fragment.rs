//! Translate one action's manifest requirements into a Cedar `<Action>CustomContext` fragment.

use super::action_name::{snake_to_pascal, REGISTERED_ACTIONS};
use super::fragment::{CedarTypeFragment, CustomFieldSource};
use crate::policy_rpc::{validate_manifests, PolicyManifest, PolicyRpcError};
use serde_json::Value;
use std::collections::HashSet;

const CEDAR_RESERVED: &[&str] = &[
    "principal",
    "action",
    "resource",
    "context",
    "if",
    "then",
    "else",
    "true",
    "false",
    "permit",
    "forbid",
    "when",
    "unless",
    "in",
    "has",
    "like",
    "is",
];

// Spec §"Manifest validation rules" lists `$.root`, `$.action`, `$.context`.
// The runtime selector resolver in `policy_rpc/selector.rs` also accepts
// `$.params`, and the existing planning tests rely on that. Keeping the
// validator aligned with the runtime so future manifests stay installable.
const PARAM_SELECTOR_ROOTS: &[&str] = &["$.root", "$.action", "$.context", "$.params"];

/// Render the `<Action>CustomContext` Cedar type for `action` from `manifest`.
///
/// Only requirements whose `when.action` matches `action` contribute fields.
/// Every produced field is declared optional (`?:`) in Cedar per the v0
/// fail-open enrichment model.
///
/// # Errors
///
/// Returns [`PolicyRpcError::InvalidManifest`] when the manifest violates any
/// of the 10 manifest validation rules documented in the design spec.
pub fn manifest_to_cedarschema(
    action: &str,
    manifest: &PolicyManifest,
) -> Result<CedarTypeFragment, PolicyRpcError> {
    // Rule 1: requirement.id uniqueness (delegated to existing validator).
    validate_manifests(std::slice::from_ref(manifest))?;

    // Rule 2: action must be registered.
    if !REGISTERED_ACTIONS.contains(&action) {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "unregistered action `{action}`"
        )));
    }

    // Walk requirements once, validating params (Rule 9) for the whole
    // manifest and collecting outputs that target `action`.
    let mut fields: Vec<CustomFieldSource> = Vec::new();
    let mut seen_fields: HashSet<String> = HashSet::new();
    let base_fields = base_field_names(action);

    for req in &manifest.requires {
        // Rule 9: every selector-valued param must use a permitted root.
        for (name, raw) in &req.params {
            validate_param_selector(&req.id, name, raw)?;
        }

        if req.when.action != action {
            continue;
        }

        for out in &req.outputs {
            // Rule 6: identifier shape.
            validate_identifier(&out.field)?;
            // Rule 7: reserved words.
            if CEDAR_RESERVED.contains(&out.field.as_str()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "field `{}` is a Cedar reserved word",
                    out.field
                )));
            }
            // Rule 4: collision with base context fields.
            if base_fields.contains(&out.field.as_str()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "field `{}` collides with base context field of action `{}`",
                    out.field, action
                )));
            }
            // Rule 5: duplicate output field within the same action.
            if !seen_fields.insert(out.field.clone()) {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "duplicate output field `{}` for action `{}`",
                    out.field, action
                )));
            }
            // Rule 8: selector roots.
            if !out.from.starts_with("$.result") {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "output `{}` selector must start with `$.result`",
                    out.field
                )));
            }

            let cedar_type = out.type_name.cedar_type().to_owned();
            fields.push(CustomFieldSource {
                field: out.field.clone(),
                cedar_type,
                source_requirement_id: req.id.clone(),
                source_method: req.method.clone(),
                source_from: out.from.clone(),
                requirement_optional: req.optional,
            });
        }
    }

    // Rule 10: declared context_extensions must match the derived set.
    if !manifest.context_extensions.is_empty() {
        validate_context_extensions(action, manifest, &fields)?;
    }

    let type_text = render_type_text(action, &fields);
    Ok(CedarTypeFragment { type_text, fields })
}

fn render_type_text(action: &str, fields: &[CustomFieldSource]) -> String {
    let pascal = snake_to_pascal(action);
    if fields.is_empty() {
        return format!("type {pascal}CustomContext = {{}};\n");
    }
    let body = fields
        .iter()
        .map(|f| format!("  {field}?: {ty}", field = f.field, ty = f.cedar_type))
        .collect::<Vec<_>>()
        .join(",\n")
        + ",\n";
    format!("type {pascal}CustomContext = {{\n{body}}};\n")
}

fn validate_identifier(field: &str) -> Result<(), PolicyRpcError> {
    let mut chars = field.chars();
    let Some(first) = chars.next() else {
        return Err(PolicyRpcError::InvalidManifest(
            "field name must not be empty".to_owned(),
        ));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "field `{field}` is not a valid Cedar identifier"
        )));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(PolicyRpcError::InvalidManifest(format!(
            "field `{field}` is not a valid Cedar identifier"
        )));
    }
    Ok(())
}

fn validate_param_selector(
    requirement_id: &str,
    param_name: &str,
    raw: &Value,
) -> Result<(), PolicyRpcError> {
    let Some(s) = raw.as_str() else {
        return Ok(());
    };
    if !s.starts_with("$.") {
        return Ok(());
    }
    if PARAM_SELECTOR_ROOTS
        .iter()
        .any(|root| s == *root || s.starts_with(&format!("{root}.")))
    {
        return Ok(());
    }
    Err(PolicyRpcError::InvalidManifest(format!(
        "requirement `{requirement_id}` param `{param_name}` selector `{s}` must start with one of $.root, $.action, $.context, $.params"
    )))
}

fn validate_context_extensions(
    action: &str,
    manifest: &PolicyManifest,
    fields: &[CustomFieldSource],
) -> Result<(), PolicyRpcError> {
    let Some(declared) = manifest.context_extensions.get(action) else {
        return Ok(());
    };
    for f in fields {
        let field_name = &f.field;
        let derived_type = &f.cedar_type;
        match declared.get(field_name) {
            Some(t) if t == derived_type => {}
            Some(t) => {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "context_extensions field `{action}.{field_name}` declared as `{t}` but derived from outputs as `{derived_type}`"
                )));
            }
            None => {
                return Err(PolicyRpcError::InvalidManifest(format!(
                    "context_extensions for `{action}` does not declare derived field `{field_name}`"
                )));
            }
        }
    }
    let derived: HashSet<&str> = fields.iter().map(|f| f.field.as_str()).collect();
    for declared_field in declared.keys() {
        if !derived.contains(declared_field.as_str()) {
            return Err(PolicyRpcError::InvalidManifest(format!(
                "context_extensions for `{action}` declares `{declared_field}` but no output produces it"
            )));
        }
    }
    Ok(())
}

/// Names that the action's base `Context` type already declares.
///
/// Post-Phase-2 every shipped cedarschema is trimmed to its calldata-derived
/// fields; enrichment lives in `<Action>CustomContext` and is contributed by
/// manifests. The sets below mirror those base schemas exactly so Rule 4 can
/// reject a manifest output that would collide with a base field.
///
/// The `custom` field itself is intentionally absent from every list — it is
/// the placeholder the composer overwrites with the manifest fragment and
/// must not be reserved against manifest authors.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
fn base_field_names(action: &str) -> &'static [&'static str] {
    match action {
        // DEX
        "swap" => &[
            "swapMode",
            "inputToken",
            "outputToken",
            "recipient",
            "validity",
            "feeBps",
        ],
        "add_liquidity" => &["pool", "inputTokens", "outputLp", "recipient", "validity"],
        "burn_liquidity_nft" => &["nft", "burnKind", "outputTokens", "recipient", "validity"],
        "decrease_liquidity" => &[
            "nft",
            "liquidityDelta",
            "outputTokens",
            "recipient",
            "validity",
        ],
        "donate" => &[
            "pool",
            "inputTokens",
            "from",
            "validity",
            "hooks",
            "hookPermissions",
            "isDynamicFee",
            "hookDataLen",
            "hookDataSelector",
        ],
        "increase_liquidity" => &["nft", "inputTokens", "validity"],
        "initialize_pool" => &[
            "pool",
            "token0",
            "token1",
            "feeBps",
            "tickSpacing",
            "sqrtPriceX96",
            "hooks",
            "isDynamicFee",
            "hookPermissions",
        ],
        "mint_liquidity_nft" => &[
            "pool",
            "feeBps",
            "tickRange",
            "inputTokens",
            "recipient",
            "validity",
        ],
        "remove_liquidity" => &[
            "exitMode",
            "pool",
            "inputLp",
            "outputTokens",
            "recipient",
            "validity",
        ],
        // lending
        "borrow" => &[
            "market",
            "asset",
            "amount",
            "amountMode",
            "recipient",
            "onBehalf",
            "validity",
        ],
        "flash_loan" => &[
            "pool",
            "assets",
            "receiver",
            "onBehalf",
            "flashLoanKind",
            "fee",
        ],
        "liquidate" => &[
            "market",
            "borrower",
            "collateralAsset",
            "debtAsset",
            "debtToCover",
            "seizedCollateralAmount",
            "liquidationKind",
            "liquidateMode",
            "recipient",
            "receiveAToken",
        ],
        "repay" => &[
            "market",
            "asset",
            "amount",
            "amountMode",
            "onBehalf",
            "repayKind",
            "validity",
        ],
        "revoke" => &["target", "caller", "subject", "revokeKind"],
        "set_authorization" => &[
            "market",
            "authorizer",
            "authorized",
            "isAuthorized",
            "authorizationScope",
            "amount",
        ],
        "sign_authorization" => &[
            "market",
            "authorizer",
            "authorized",
            "isAuthorized",
            "authorizationScope",
            "amount",
            "nonce",
            "validity",
        ],
        "supply" => &[
            "market",
            "asset",
            "amount",
            "amountMode",
            "recipient",
            "from",
            "validity",
        ],
        "withdraw" => &[
            "market",
            "asset",
            "amount",
            "amountMode",
            "recipient",
            "onBehalf",
        ],
        // misc
        "approve" => &["approvalKind", "token", "spender", "amount", "validity"],
        "claim_rewards" => &["sourceAddress", "nft", "from", "recipient", "rewards"],
        "delegate" => &["token", "delegatee", "validity"],
        "permit" => &[
            "permitKind",
            "token",
            "owner",
            "spender",
            "recipient",
            "amount",
            "requestedAmount",
            "operator",
            "approved",
            "validity",
            "signatureValidity",
        ],
        "set_approval_for_all" => &["collection", "operator", "approved"],
        "sign_message" => &[
            "signer",
            "requestChainId",
            "domainChainId",
            "verifyingContract",
            "primaryType",
            "nowTs",
            "messageDigest",
        ],
        "transfer" => &["token", "from", "recipient"],
        "unwrap" => &["wrappedAsset", "nativeAsset", "recipient"],
        "vote" => &["governance", "proposalId", "support", "reason", "validity"],
        "wrap" => &["nativeAsset", "wrappedAsset", "recipient"],
        // restaking
        "claim_restake_withdrawal" => &["tokenOut", "amountOut", "recipient"],
        "request_restake_withdrawal" => &[
            "tokenOut",
            "receiptToken",
            "amountIn",
            "amountOut",
            "strategy",
            "recipient",
        ],
        "restake" => &[
            "tokenIn",
            "receiptToken",
            "amountIn",
            "amountOut",
            "strategy",
            "recipient",
        ],
        // staking
        "claim_unstake" => &["tokenOut", "amountOut", "recipient"],
        "request_unstake" => &[
            "receiptToken",
            "tokenOut",
            "amountIn",
            "amountOut",
            "recipient",
        ],
        "stake" => &[
            "tokenIn",
            "receiptToken",
            "amountIn",
            "amountOut",
            "recipient",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_rpc::{
        ContextProjection, PolicyManifest, ProjectionType, Requirement, RequirementWhen,
    };
    use std::collections::BTreeMap;

    fn manifest_with_one_output() -> PolicyManifest {
        PolicyManifest {
            id: "test::swap".to_owned(),
            schema_version: 1,
            requires: vec![Requirement {
                id: "req1".into(),
                when: RequirementWhen {
                    action: "swap".into(),
                },
                method: "oracle.usd_value".into(),
                params: BTreeMap::default(),
                outputs: vec![ContextProjection {
                    kind: "context".into(),
                    field: "totalInputUsd".into(),
                    type_name: ProjectionType::UsdValuation,
                    from: "$.result".into(),
                    required: false,
                }],
                optional: true,
            }],
            context_extensions: BTreeMap::default(),
        }
    }

    #[test]
    fn single_output_produces_one_optional_field() {
        let f = manifest_to_cedarschema("swap", &manifest_with_one_output()).expect("ok");
        assert_eq!(f.fields.len(), 1);
        let only = &f.fields[0];
        assert_eq!(only.field, "totalInputUsd");
        assert_eq!(only.cedar_type, "UsdValuation");
        assert!(only.requirement_optional);
        assert!(f.type_text.contains("totalInputUsd?: UsdValuation"));
        assert!(f.type_text.contains("type SwapCustomContext = {"));
    }

    #[test]
    fn empty_manifest_produces_empty_type() {
        let m = PolicyManifest {
            id: "test::swap".into(),
            schema_version: 1,
            requires: vec![],
            context_extensions: BTreeMap::default(),
        };
        let f = manifest_to_cedarschema("swap", &m).unwrap();
        assert!(f.fields.is_empty());
        assert!(f.type_text.contains("type SwapCustomContext = {};"));
    }

    #[test]
    fn rule1_duplicate_requirement_id() {
        let mut m = manifest_with_one_output();
        let mut dup = m.requires[0].clone();
        dup.outputs[0].field = "other".into();
        m.requires.push(dup);
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule2_unregistered_action_errors() {
        let m = manifest_with_one_output();
        let err = manifest_to_cedarschema("unknown_action", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule3_unknown_type_is_rejected_by_serde() {
        // ProjectionType is a closed enum, so serde rejects unknown values at
        // parse time. This test pins that behavior so Rule 3 stays covered.
        let json = r#"{
          "id":"x","schema_version":1,
          "requires":[{"id":"r","when":{"action":"swap"},"method":"m","params":{},
            "outputs":[{"kind":"context","field":"f","type":"RiskScore","from":"$.result","required":false}],
            "optional":true}],
          "context_extensions":{}}"#;
        let parsed: Result<PolicyManifest, _> = serde_json::from_str(json);
        assert!(parsed.is_err(), "serde must reject unknown ProjectionType");
    }

    #[test]
    fn rule4_field_collides_with_base() {
        let mut m = manifest_with_one_output();
        m.requires[0].outputs[0].field = "swapMode".into();
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule5_duplicate_output_field() {
        let mut m = manifest_with_one_output();
        let mut second = m.requires[0].clone();
        second.id = "req2".into();
        m.requires.push(second);
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule6_invalid_identifier() {
        let mut m = manifest_with_one_output();
        m.requires[0].outputs[0].field = "1totalUsd".into();
        assert!(manifest_to_cedarschema("swap", &m).is_err());
        m.requires[0].outputs[0].field = "total-usd".into();
        assert!(manifest_to_cedarschema("swap", &m).is_err());
        m.requires[0].outputs[0].field = String::new();
        assert!(manifest_to_cedarschema("swap", &m).is_err());
    }

    #[test]
    fn rule7_reserved_word_field() {
        let mut m = manifest_with_one_output();
        m.requires[0].outputs[0].field = "context".into();
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule8_from_must_start_with_result() {
        let mut m = manifest_with_one_output();
        m.requires[0].outputs[0].from = "$.action.inputToken".into();
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule9_param_selector_root_invalid() {
        let mut m = manifest_with_one_output();
        m.requires[0]
            .params
            .insert("x".into(), Value::String("$.bogus.foo".into()));
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule9_param_selector_known_roots_allowed() {
        for root in [
            "$.root.chain_id",
            "$.action.foo",
            "$.context.bar",
            "$.params.origin",
        ] {
            let mut m = manifest_with_one_output();
            m.requires[0]
                .params
                .insert("k".into(), Value::String(root.into()));
            assert!(manifest_to_cedarschema("swap", &m).is_ok(), "{root}");
        }
    }

    #[test]
    fn rule9_non_selector_string_params_allowed() {
        let mut m = manifest_with_one_output();
        m.requires[0]
            .params
            .insert("k".into(), Value::String("not-a-selector".into()));
        m.requires[0].params.insert("n".into(), Value::from(42));
        assert!(manifest_to_cedarschema("swap", &m).is_ok());
    }

    #[test]
    fn rule10_context_extensions_mismatch() {
        let mut m = manifest_with_one_output();
        let mut ext: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut fields = BTreeMap::new();
        fields.insert("totalInputUsd".into(), "Long".into());
        ext.insert("swap".into(), fields);
        m.context_extensions = ext;
        let err = manifest_to_cedarschema("swap", &m).unwrap_err();
        assert!(matches!(err, PolicyRpcError::InvalidManifest(_)));
    }

    #[test]
    fn rule10_context_extensions_match_passes() {
        let mut m = manifest_with_one_output();
        let mut ext: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut fields = BTreeMap::new();
        fields.insert("totalInputUsd".into(), "UsdValuation".into());
        ext.insert("swap".into(), fields);
        m.context_extensions = ext;
        assert!(manifest_to_cedarschema("swap", &m).is_ok());
    }
}
