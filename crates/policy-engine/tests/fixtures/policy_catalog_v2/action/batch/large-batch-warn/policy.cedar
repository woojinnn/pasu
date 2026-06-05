// blocks: silently approving a large multi-call batch (more than 8 bundled calls) without per-leg review
// why:    batch-review-fatigue — a many-leg bundle is hard to inspect leg-by-leg, so a hostile call can ride along unnoticed (calldata-obfuscation / consent-overwhelm threat class)
// bucket: action/batch  (precedence-won)
// tags: bucket:action cat:batch static
//
// Catalog H1 — warn on a batch (multicall) bundling more than 8 calls.
// Hard for a user to review every leg. OUTER scope: fires once on the batch
// itself, not per child. Pure-static (`childCount` is Long).
@id("large-batch-warn")
@severity("warn")
@reason("This batch bundles many calls — review each action before approving")
forbid(principal, action == Core::Action::"Multicall", resource)
when { context.childCount > 8 };
