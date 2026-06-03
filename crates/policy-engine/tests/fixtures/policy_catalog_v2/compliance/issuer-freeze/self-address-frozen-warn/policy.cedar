// blocks: an ERC-20 transfer where the signer's OWN address is frozen on the issuer's on-chain blocklist — soft warn.
// why:    issuer-freeze self-screen — if the user's own address has been frozen by the stablecoin issuer (Tether isBlackListed / Centre blocklist), an outbound transfer of that asset will revert and may signal the account is under enforcement. Warn (not deny) so the user learns their own address is flagged before wasting a signature. (Tether transparency / TetherToken isBlackListed.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:transfer
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blocklist for (asset=token,
// address=the signer's own from-address `$.root.from`). ENRICHMENT — dormant
// until the dispatcher serves the method; the `context.custom has frozen` guard is inert.
@id("self-address-frozen-warn")
@severity("warn")
@reason("Your own address is frozen on this asset's issuer blocklist — the transfer will likely fail")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
