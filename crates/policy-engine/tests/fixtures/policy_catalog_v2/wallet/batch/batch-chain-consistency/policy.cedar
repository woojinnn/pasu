// blocks: signing a multi-call batch without reviewing that every leg targets the
//         intended chain (a cross-chain leg smuggled into a same-chain bundle).
// why:    batch chain-consistency review (Ledger clear-signing guidance) — a
//         bundle that mixes chains/legs hides an off-chain or wrong-chain action
//         among benign ones. VOCAB §2: the OUTER Multicall context exposes only a
//         flat `{domain, action}` summary per child — NOT each child's chain — so
//         the per-leg chain screen runs in the service-worker per-child re-eval.
//         At outer scope this is a count gate that flags any >1-leg batch for the
//         user to confirm chain consistency before signing.
// bucket: wallet/batch
// tags: bucket:wallet cat:batch static action-family:batch-review
//
// OUTER scope. Pure-static — `childCount` is the only batch-wide field that bears
// on chain consistency at this scope; finer per-leg chain checks are deferred to
// the SW (VOCAB §2 / §4.4).
@id("batch-chain-consistency")
@severity("warn")
@reason("This batch bundles multiple calls — confirm every leg targets the intended chain")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.childCount > 1
};
