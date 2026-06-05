// blocks: signing an intent order whose sell-side USD notional exceeds the $50k cap
// why:    large-order exposure — an oversized signed intent (CowSwap / UniswapX / 1inch Fusion) is a solver-fillable mandate; a USD ceiling forces a size confirmation before the order becomes redeemable (CoW Protocol order docs)
// bucket: protocol/intent-venue
// tags: bucket:protocol cat:intent-venue enrichment method:oracle.usd_value venue:intent action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
// ENRICHMENT — dormant until the dispatcher serves oracle.usd_value (method is registered; the field is host-populated).
@id("intent-order-sell-usd-cap")
@severity("warn")
@reason("Large intent order (over $50k sell notional) — confirm the size before signing")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has sellUsd
  && context.custom.sellUsd.greaterThan(decimal("50000.0000"))
};
