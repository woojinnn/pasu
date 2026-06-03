// blocks: signing a swap whose input-side USD notional exceeds the $50k cap.
// why:    large-trade exposure — an oversized swap amplifies slippage/MEV loss and
//         fat-finger risk; a USD ceiling forces a size confirmation before signing.
// bucket: wallet/usd-cap  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// Catalog C6 — warn on a swap whose input USD value exceeds $50k.
// `oracle.usd_value` enriches the input notional into `context.custom.swapUsd`.
// ENRICHMENT — dormant until the dispatcher serves `oracle.usd_value`.
@id("swap-usd-cap")
@severity("warn")
@reason("Large swap (over $50k notional) — confirm the size before signing")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has swapUsd
  && context.custom.swapUsd.greaterThan(decimal("50000.0000"))
};
