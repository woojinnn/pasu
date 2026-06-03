// blocks: an Aave supply whose aTokens are minted to a different address (onBehalfOf != you) — you fund a position you do not own
// why:    misdirected-funds threat class — Aave V3 supply(...,onBehalfOf) mints the aTokens (the collateral claim) to the named address; a wrong/hostile onBehalfOf gives your deposited collateral to someone else. Source: Aave V3 Pool.supply docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave static venue:aave_v3 action-family:supply
// Pure-static: onBehalfOf is optional → has-guard before comparing to the
// submitter. Fires only when the aToken beneficiary differs from the signer.
@id("aave-supply-on-behalf-of-other")
@severity("warn")
@reason("This supply mints the aToken collateral claim to another address (onBehalfOf is not you) — confirm the beneficiary")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has onBehalfOf
  && context.onBehalfOf != context.meta.submitter
};
