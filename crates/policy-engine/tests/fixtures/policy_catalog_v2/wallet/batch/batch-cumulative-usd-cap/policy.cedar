// blocks: signing a multi-call batch whose combined USD value exceeds the $50k cap.
// why:    batch-level value ceiling (Fireblocks USD limits) — a per-leg cap misses
//         a batch that spreads a large spend across legs; a summed-USD ceiling on
//         the whole bundle gates the aggregate before one signature commits it.
// bucket: wallet/batch
// tags: bucket:wallet cat:batch enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// OUTER scope. The summed-batch valuation projects into the Multicall action's
// `MulticallCustomContext` (`context.custom.usdValue`). VOCAB §2/§4.4: the outer
// context carries only a `{domain, action}` per-child summary — no per-leg
// asset/amount — so summing the legs into one USD scalar requires a batch-summing
// enrichment the dispatcher does not yet provide.
// ENRICHMENT — dormant: until a batch-summing oracle.usd_value path over Multicall
// children is wired in the /v1/rpc dispatcher (no per-leg amount at outer scope),
// `context.custom.usdValue` is absent and the has-guard keeps the policy inert.
@id("batch-cumulative-usd-cap")
@severity("warn")
@reason("This batch's combined value exceeds $50k — confirm the full bundle before signing")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context has custom
  && context.custom has usdValue
  && context.custom.usdValue.greaterThan(decimal("50000.0000"))
};
