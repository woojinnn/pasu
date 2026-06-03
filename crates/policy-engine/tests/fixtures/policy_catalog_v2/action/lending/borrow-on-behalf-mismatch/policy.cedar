// blocks: borrowing on behalf of a third party (onBehalfOf != submitter), which draws against a credit line the user delegated and can be abused to exhaust a delegated allowance.
// why:    Aave credit-delegation lets `borrow(...onBehalfOf)` draw debt onto another account that delegated borrow power; a malicious dApp can borrow against the user's delegated credit without their intent (credit-delegation abuse). Source: Aave V3 credit-delegation docs.
// bucket: action/lending
// tags: bucket:action cat:lending static action-family:on-behalf
// Pure-static: onBehalfOf is optional (defaults to submitter) — `has`-guard,
// then warn only on an explicit third-party borrower of record.
@id("borrow-on-behalf-mismatch")
@severity("warn")
@reason("This borrow is registered to a different account than yours — confirm the credit-delegation is intended")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has onBehalfOf
  && context.onBehalfOf != context.meta.submitter
};
