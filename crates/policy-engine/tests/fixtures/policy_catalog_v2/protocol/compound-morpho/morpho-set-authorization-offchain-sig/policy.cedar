// blocks: silently signing a gasless Morpho `setAuthorizationWithSig` that grants position-management authority
// why:    the off-chain EIP-712 twin (`setAuthorizationWithSig`) grants the same authority as the on-chain call but with no gas/UX friction — a relayer can submit it later, so the signer must consciously confirm the gasless grant (delegated-authorization + offchain-sig threat class). Source: Morpho Blue docs — `setAuthorizationWithSig`.
// bucket: protocol/compound-morpho
// tags: bucket:protocol cat:compound-morpho static action-family:permission
//
// Catalog — confirm an OFF-CHAIN Morpho authorization grant. Discriminate the
// signature twin from the on-chain call via `meta.nature.kind == "offchain_sig"`
// so this does not double-fire with the on-chain grant-confirm. SetAuthorization
// has NO venue. Pure-static.
@id("morpho-set-authorization-offchain-sig")
@severity("warn")
@reason("Signing a gasless Morpho authorization grants position-management authority a relayer can submit later — confirm")
forbid(principal, action == Lending::Action::"SetAuthorization", resource)
when {
  context.isAuthorized == true
  && context.meta.nature.kind == "offchain_sig"
};
