// blocks: signing a multi-call batch so no leg escapes risk-category screening — the outer gate anchors the batch so every child is re-evaluated.
// why:    risk-category screening (batch) — a batch could smuggle a risk-categorised recipient leg (mixer/darknet/scam) among benign ones. The outer Multicall context exposes only per-child {domain,action}, not per-leg addresses, so the actual recipient screen runs per-child in the service worker (the inner risk/* policies); this outer rule anchors a non-empty batch so each leg is dispatched for inner re-eval. WARN tier surfaces the batch for review without hard-blocking. (analytics risk categories.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk static could-be-compliance:true action-family:batch
//
// OUTER scope, pure-static. Per-leg risk-category detail is NOT in the outer
// context (VOCAB §2) — it is screened per-child (inner) and aggregated. This
// gate simply anchors a non-empty batch for review.
@id("sanctioned-batch-recipient-warn")
@severity("warn")
@reason("A leg of this batch may carry a risk-categorised recipient — review the batch before proceeding")
forbid(principal, action == Core::Action::"Multicall", resource)
when { context.childCount > 0 };
