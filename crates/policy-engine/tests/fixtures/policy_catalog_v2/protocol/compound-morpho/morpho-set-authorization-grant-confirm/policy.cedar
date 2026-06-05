// blocks: silently granting a Morpho Blue authorization that lets another address manage your position
// why:    `setAuthorization`/`setAuthorizationWithSig` on Morpho Blue lets the authorized address withdraw/borrow against your account (delegated-authorization threat class). Confirm on GRANT (`isAuthorized == true`); revokes pass. Source: Morpho Blue docs — `setAuthorization`.
// bucket: protocol/compound-morpho
// tags: bucket:protocol cat:compound-morpho static action-family:permission
//
// Catalog — confirm a Morpho Blue authorization grant. SetAuthorization carries NO
// venue (bespoke chain/protocol locator), so this binds on the action UID + the
// `isAuthorized` flag only. Pure-static.
@id("morpho-set-authorization-grant-confirm")
@severity("warn")
@reason("Authorizing an address on Morpho lets it manage your position — confirm the grantee")
forbid(principal, action == Lending::Action::"SetAuthorization", resource)
when { context.isAuthorized == true };
