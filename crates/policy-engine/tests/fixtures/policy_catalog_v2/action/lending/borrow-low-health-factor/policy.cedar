// blocks: a borrow that would push the account's projected health factor below 1.5, leaving it close to liquidation.
// why:    thin-collateral lending positions get liquidated on minor price moves — warning on a low post-action health factor guards against self-inflicted liquidation risk (Aave/Compound health-factor model).
// bucket: action/lending  (precedence-won)
// tags: bucket:action cat:lending enrichment method:lending.health_factor
// methods: lending.health_factor -> ../../../_methods/lending.health_factor.md
// Catalog E2 — warn when a borrow would leave the health factor below 1.5.
// `lending.health_factor` enriches the projected post-borrow HF.
// ENRICHMENT — dormant until the dispatcher serves `lending.health_factor`.
@id("borrow-low-health-factor")
@severity("warn")
@reason("This borrow leaves your position close to liquidation (health factor < 1.5)")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has postActionHf
  && context.custom.postActionHf.lessThan(decimal("1.5000"))
};
