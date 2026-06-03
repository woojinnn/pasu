// blocks: disabling an asset as collateral while you hold open debt, dropping the projected health factor below 1.1 and into liquidation range
// why:    self-inflicted-liquidation threat class — unmarking collateral on Aave removes borrow backing; if open debt remains the post-action health factor can fall below the liquidation threshold. Source: Aave V3 setUserUseReserveAsCollateral docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave enrichment method:lending.health_factor venue:aave_v3
// methods: lending.health_factor -> ../../../_methods/lending.health_factor.md
// ENRICHMENT — dormant until lending.health_factor is registered in method-catalog.json.
// dormant: until lending.health_factor is registered in method-catalog.json
@id("aave-disable-collateral-while-borrowing")
@severity("warn")
@reason("Disabling this collateral leaves your borrow position near liquidation (health factor < 1.1)")
forbid(principal, action == Lending::Action::"DisableCollateral", resource)
when {
  context has custom
  && context.custom has postActionHf
  && context.custom.postActionHf.lessThan(decimal("1.1000"))
};
