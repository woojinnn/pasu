//! Phase 2 Task 2.1 — swap.cedarschema must declare an empty
//! `SwapCustomContext` placeholder and host none of the enrichment fields
//! that the matching `swap.policy-rpc.json` manifest contributes.

#[test]
fn swap_base_schema_has_custom_placeholder() {
    let text = std::fs::read_to_string("../../schema/policy-schema/actions/DEX/swap.cedarschema")
        .expect("swap.cedarschema must be readable from the workspace root");
    assert!(
        text.contains("custom?: SwapCustomContext"),
        "swap.cedarschema must declare `custom?: SwapCustomContext` on SwapContext"
    );
    assert!(
        text.contains("type SwapCustomContext = {};"),
        "swap.cedarschema must declare an empty `SwapCustomContext` stub"
    );
    // Enrichment-derived fields must be gone from the base schema; the
    // manifest is now the single source of truth for them.
    assert!(
        !text.contains("totalInputUsd?: UsdValuation"),
        "totalInputUsd is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("totalMinOutputUsd?: UsdValuation"),
        "totalMinOutputUsd is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("effectiveRateVsOracleBps?:"),
        "effectiveRateVsOracleBps is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("totalInputFractionOfPortfolioBps?:"),
        "totalInputFractionOfPortfolioBps is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("validityDeltaSec?:"),
        "validityDeltaSec is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("recipientIsContract?:"),
        "recipientIsContract is enrichment and must not appear in base swap.cedarschema"
    );
    assert!(
        !text.contains("windowStats?:"),
        "windowStats is enrichment and must not appear in base swap.cedarschema"
    );
}
