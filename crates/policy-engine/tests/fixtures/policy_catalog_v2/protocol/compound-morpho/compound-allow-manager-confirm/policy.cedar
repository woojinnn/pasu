// blocks: silently granting a Compound III (Comet) account manager who can move/borrow on your behalf
// why:    `allow(manager, true)` on Comet gives the manager full authority to transfer collateral and borrow against your account (delegated-authorization / collateral-drain threat class). Source: Compound III docs — Comet `allow`/`allowBySig`.
// bucket: protocol/compound-morpho
// tags: bucket:protocol cat:compound-morpho static venue:compound_v3 action-family:permission
//
// Catalog — confirm a Compound III account-manager grant. The grant surfaces as a
// `protocol_authorization` ActionBody; warn only when GRANTING to Compound III
// (`protocolName == "compound_v3"` && `isAuthorized == true`). Revokes pass.
// Pure-static (`protocolName`/`isAuthorized` are fixed fields).
@id("compound-allow-manager-confirm")
@severity("deny")
@reason("Granting a Compound III account manager lets it move and borrow against your account — confirm the manager")
forbid(principal, action == Permission::Action::"ProtocolAuthorization", resource)
when {
  context.protocolName == "compound_v3"
  && context.isAuthorized == true
};
