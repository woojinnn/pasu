// blocks: a collateral withdrawal that would drop the account's projected health factor below 1.5, pushing the position toward liquidation.
// why:    removing collateral while debt is outstanding reduces the liquidation buffer — warning on a low post-action health factor guards against self-inflicted liquidation risk (Aave/Compound health-factor model).
// bucket: action/lending  (precedence-won)
// tags: bucket:action cat:lending enrichment method:lending.health_factor
// methods: lending.health_factor -> ../../../_methods/lending.health_factor.md
// Catalog E3 — warn when withdrawing collateral would drop the health factor below 1.5.
// `lending.health_factor` enriches the projected post-withdraw HF.
// ENRICHMENT — dormant until the dispatcher serves `lending.health_factor`.
@id("withdraw-collateral-health-factor")
@severity("warn")
@reason("Withdrawing this much collateral pushes your position toward liquidation (health factor < 1.5)")
forbid(principal, action == Lending::Action::"Withdraw", resource)
when {
  context has custom
  && context.custom has postActionHf
  && context.custom.postActionHf.lessThan(decimal("1.5000"))
};
