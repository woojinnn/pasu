// blocks: a borrow that draws against collateral so heavily it leaves the projected health factor below 1.2 — i.e. borrows beyond a safe fraction of the deposited collateral.
// why:    a wallet-level borrowing cap expressed as a collateral-utilization floor (post-action HF) bounds how much of the collateral a single borrow may encumber, keeping a liquidation buffer. (Gnosis Safe roles / borrowing-cap mandate.)
// bucket: wallet/fraction-of-holdings
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:lending.health_factor action-family:fraction-of-holdings
// methods: lending.health_factor -> ../../../_methods/lending.health_factor.md
// dormant: until lending.health_factor is registered in method-catalog.json
@id("borrow-fraction-of-collateral-cap")
@severity("warn")
@reason("This borrow draws too much against your collateral (health factor < 1.2) — confirm the size")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has postActionHf
  && context.custom.postActionHf.lessThan(decimal("1.2000"))
};
