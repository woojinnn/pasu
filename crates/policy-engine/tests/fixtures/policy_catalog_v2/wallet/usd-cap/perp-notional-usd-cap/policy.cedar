// blocks: opening a perp position whose collateral-backed notional exceeds the $25k cap.
// why:    notional-exposure risk — leveraged size, not just collateral, drives
//         liquidation and funding loss; a USD ceiling on the priced collateral leg
//         gates outsized positions before signing. source: Fireblocks
//         transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap action-family:perp-risk
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Perp::OpenPosition prices the `collateralToken` + `collateralAmount`
// pair via oracle.usd_value (the only U256-hex amount oracle.usd_value can take —
// `size`/`leverage`/`notional` are decimal STRINGS per VOCAB §7, not priceable here).
// This caps the collateral notional; true leveraged notional = collateral × leverage
// and is not summable in one scalar projection. Distinct from the existing
// `perp-position-usd-cap` ($100k collateral cap) by binding a tighter $25k notional
// gate under a separate leaf. The policy reads only the projected USD leaf, so it
// compiles regardless of planner resolution.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("perp-notional-usd-cap")
@severity("warn")
@reason("Large perp notional (collateral over $25k) — confirm the position size before signing")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has perpNotionalUsd
  && context.custom.perpNotionalUsd.greaterThan(decimal("25000.0000"))
};
