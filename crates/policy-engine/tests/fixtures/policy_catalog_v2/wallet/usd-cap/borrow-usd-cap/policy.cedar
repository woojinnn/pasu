// blocks: signing a debt-position borrow whose principal exceeds the $25k USD cap.
// why:    oversized leverage / liquidation-buffer risk — large borrows raise the
//         account's liquidation exposure; a USD ceiling forces a size confirmation.
// bucket: wallet/usd-cap  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// Catalog E6 — warn on a borrow whose USD value exceeds $25k.
// `oracle.usd_value` enriches the borrowed notional. ENRICHMENT — dormant until
// the dispatcher serves `oracle.usd_value`.
@id("borrow-usd-cap")
@severity("warn")
@reason("Large borrow (over $25k) — confirm the size and your liquidation buffer")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has borrowUsd
  && context.custom.borrowUsd.greaterThan(decimal("25000.0000"))
};
