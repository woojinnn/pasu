// blocks: opening a perp position whose collateral notional exceeds the $100k cap.
// why:    perp position-sizing risk — outsized collateral concentrates liquidation
//         and funding exposure on one leveraged position; a USD ceiling gates it.
// bucket: wallet/usd-cap  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap action-family:perp-risk
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// Catalog F4 — warn when a perp position's collateral exceeds $100k.
// `oracle.usd_value` enriches the collateral notional. ENRICHMENT — dormant
// until the dispatcher serves `oracle.usd_value`.
@id("perp-position-usd-cap")
@severity("warn")
@reason("Large perp collateral (over $100k) — confirm the position size")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has collateralUsd
  && context.custom.collateralUsd.greaterThan(decimal("100000.0000"))
};
