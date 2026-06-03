// blocks: granting a Morpho Blue authorization to an address flagged as a known drainer/scam
// why:    the authorized address can withdraw/borrow against your Morpho position; granting it to a flagged address is a direct position-takeover vector (delegated-authorization + reputation-feed threat class). Source: Morpho Blue docs — `setAuthorization`/`setAuthorizationWithSig`.
// bucket: protocol/compound-morpho
// tags: bucket:protocol cat:compound-morpho enrichment method:address.reputation could-be-compliance:reputation-feed action-family:permission
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// ENRICHMENT — `context.custom.authorizedFlagged` is host-populated by
// address.reputation on the grantee (`$.action.authorized`); always `has`-guarded
// so the policy is inert until the method is wired. SetAuthorization has NO venue.
@id("morpho-set-authorization-reputation-deny")
@severity("deny")
@reason("The address you are authorizing on Morpho is flagged as known-malicious — blocked")
forbid(principal, action == Lending::Action::"SetAuthorization", resource)
when {
  context.isAuthorized == true
  && context has custom
  && context.custom has authorizedFlagged
  && context.custom.authorizedFlagged == true
};
