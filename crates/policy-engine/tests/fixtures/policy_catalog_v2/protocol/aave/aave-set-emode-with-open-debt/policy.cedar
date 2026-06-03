// blocks: switching Aave e-mode category while you carry open debt, where the recomputed health factor lands below 1.1 (liquidation range)
// why:    self-inflicted-liquidation threat class — changing e-mode re-derives your LTV and liquidation thresholds against current debt; if the projected post-action health factor falls below the liquidation threshold the position becomes liquidatable. Source: Aave V3 setUserEMode / e-mode risk docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave enrichment method:lending.health_factor venue:aave_v3
// methods: lending.health_factor -> ../../../_methods/lending.health_factor.md
// ENRICHMENT — dormant until lending.health_factor is registered in method-catalog.json.
// dormant: until lending.health_factor is registered in method-catalog.json
@id("aave-set-emode-with-open-debt")
@severity("warn")
@reason("Changing e-mode with open debt drops your projected health factor below 1.1 (liquidation range)")
forbid(principal, action == Lending::Action::"SetEMode", resource)
when {
  context has custom
  && context.custom has postActionHf
  && context.custom.postActionHf.lessThan(decimal("1.1000"))
};
