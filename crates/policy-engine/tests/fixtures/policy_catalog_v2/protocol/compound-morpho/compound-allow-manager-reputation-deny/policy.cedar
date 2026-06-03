// blocks: granting a Compound III account-manager role to an address flagged as a known drainer/scam
// why:    a manager allowed on Comet can move collateral and borrow against you; granting it to a flagged address is a direct account-takeover vector (delegated-authorization + reputation-feed threat class). Source: Compound III docs — Comet `allow`/`allowBySig`.
// bucket: protocol/compound-morpho
// tags: bucket:protocol cat:compound-morpho enrichment method:address.reputation venue:compound_v3 could-be-compliance:reputation-feed action-family:permission
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// ENRICHMENT — `context.custom.managerFlagged` is host-populated by
// address.reputation on the grantee (`$.action.authorized`); always `has`-guarded
// so the policy is inert until the method is wired.
@id("compound-allow-manager-reputation-deny")
@severity("deny")
@reason("The account manager you are granting is flagged as a known-malicious address — blocked")
forbid(principal, action == Permission::Action::"ProtocolAuthorization", resource)
when {
  context.isAuthorized == true
  && context has custom
  && context.custom has managerFlagged
  && context.custom.managerFlagged == true
};
