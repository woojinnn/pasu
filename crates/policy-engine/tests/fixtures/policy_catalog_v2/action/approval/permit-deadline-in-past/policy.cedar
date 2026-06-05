// blocks: signing an EIP-2612 permit whose deadline is already in the past relative to wall-clock now.
// why:    malformed / replay-bait permit — an already-expired deadline means the signature is useless to the user but is a tell-tale of a crafted/forged payload (or a UI lying about validity); surface it before signing (EIP-2612 deadline semantics).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:clock.now action-family:stale-ttl
// methods: clock.now -> ../../../_methods/clock.now.md
//
// `clock.now` enriches wall-clock unix seconds. Warn when the permit deadline is
// already in the past (deadline < now). ENRICHMENT (clock.now) — dormant until
// the dispatcher serves it.
@id("permit-deadline-in-past")
@severity("warn")
@reason("This permit's deadline is already in the past — the signature is invalid or the payload is malformed")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.deadline < context.custom.nowTs
};
