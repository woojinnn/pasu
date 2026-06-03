// blocks: repaying debt registered to a third party (onBehalfOf != submitter) without the user noticing they are spending their own funds to clear someone else's loan.
// why:    Aave-style `repay(...onBehalfOf)` lets the caller settle another account's debt; a dApp could silently set onBehalfOf to a third party so the user funds an unrelated position (repay-on-behalf misdirection). Source: Aave V3 Pool.repay docs.
// bucket: action/lending
// tags: bucket:action cat:lending static action-family:on-behalf
// Pure-static: onBehalfOf is optional (defaults to submitter) — guard with `has`
// before comparing; only warn when an explicit third-party debtor is named.
@id("repay-on-behalf-confirm")
@severity("warn")
@reason("You are repaying debt registered to another account — confirm you intend to spend your funds on their loan")
forbid(principal, action == Lending::Action::"Repay", resource)
when {
  context has onBehalfOf
  && context.onBehalfOf != context.meta.submitter
};
