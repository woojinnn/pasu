// blocks: an Aave repay credited to someone else's debt position (onBehalfOf != you) — your funds clear a third party's loan
// why:    misdirected-funds threat class — Aave V3 repay(...,onBehalfOf) settles the named address's debt, not yours; a wrong/hostile onBehalfOf burns your repayment with no benefit to your own position. Source: Aave V3 Pool.repay docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave static venue:aave_v3 action-family:repay
// Pure-static: onBehalfOf is optional → has-guard before comparing to the
// submitter. Fires only when the repay is explicitly credited to a different
// address than the signer.
@id("aave-repay-on-behalf-of-other")
@severity("warn")
@reason("This repay clears someone else's Aave debt (onBehalfOf is not you) — confirm the beneficiary")
forbid(principal, action == Lending::Action::"Repay", resource)
when {
  context has onBehalfOf
  && context.onBehalfOf != context.meta.submitter
};
