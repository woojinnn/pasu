// blocks: transacting an ERC-20 token whose contract is unverified / unrecognized by the metadata feed.
// why:    counterfeit-token bait — an unverified token contract is the carrier for honeypots, spoofed-symbol lookalikes, and malicious transfer hooks; surfacing "unknown token" before the user acts is the standard wallet defense (MetaMask / Blockaid token-alert class).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:token.metadata action-family:approval
// methods: token.metadata -> ../../../_methods/token.metadata.md
// dormant: until token.metadata is registered in method-catalog.json
//
// `token.metadata` enriches whether the token contract is verified. Warn when
// the token is unverified (isVerified == false). ENRICHMENT — dormant until the
// dispatcher serves the method.
@id("interact-unverified-token")
@severity("warn")
@reason("This token's contract is unverified / unrecognized — confirm it is the asset you intend before transacting")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has isVerified
  && context.custom.isVerified == false
};
