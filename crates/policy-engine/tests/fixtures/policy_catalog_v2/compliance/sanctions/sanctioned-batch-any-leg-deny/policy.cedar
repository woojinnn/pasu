// blocks: signing a multi-call batch so that no leg escapes sanctions screening — the outer gate ensures every child is re-evaluated.
// why:    sanctions screening (batch) — a batch could smuggle a sanctioned-recipient leg among benign ones. The outer Multicall context exposes only per-child {domain,action}, not per-leg addresses, so the actual screen runs per-child in the service worker (the inner sanctioned-* policies); under deny-overrides aggregation, any sanctioned leg fails the whole batch. This outer rule anchors the non-empty batch so each leg is dispatched for inner re-eval. (OFAC SLS / Treasury SDN list.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions static could-be-compliance:true action-family:batch
//
// OUTER scope, pure-static. Per-leg sanctions detail is NOT in the outer
// context (VOCAB §2) — it is screened per-child (inner) and aggregated
// deny-overrides. This gate simply requires a non-empty batch.
@id("sanctioned-batch-any-leg-deny")
@severity("deny")
@reason("A leg of this batch screens as sanctioned (OFAC/EU/UN) — the whole batch is blocked")
forbid(principal, action == Core::Action::"Multicall", resource)
when { context.childCount > 0 };
