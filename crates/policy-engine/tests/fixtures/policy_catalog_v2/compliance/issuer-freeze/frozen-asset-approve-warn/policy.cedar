// blocks: an ERC-20 approve granting allowance to a spender frozen on the asset's issuer blocklist — soft warn.
// why:    issuer-freeze screening — granting spend authority to a spender the stablecoin issuer has frozen (Tether isBlackListed / Centre blocklist) is pointless and a sign the counterparty is under enforcement: the spender cannot move the asset, and an approval to a flagged contract is a standing risk. Warn (not deny) — an approve is a grant, not yet a movement. (Tether transparency / TetherToken isBlackListed.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:approval
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blocklist for (asset=token, address=spender).
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has frozen` guard is inert until then.
@id("frozen-asset-approve-warn")
@severity("warn")
@reason("Approval spender is frozen on this asset's issuer blocklist — review before granting")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
