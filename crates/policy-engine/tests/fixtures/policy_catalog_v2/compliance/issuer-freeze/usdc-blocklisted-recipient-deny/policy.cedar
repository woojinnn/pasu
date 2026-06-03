// blocks: an ERC-20 (USDC) transfer whose recipient the issuer has blocklisted on-chain (Centre Blacklistable).
// why:    issuer-freeze screening — Circle/Centre's USDC FiatToken exposes an on-chain blocklist; a Blacklister-frozen recipient cannot receive or move USDC, so the transfer would revert or strand value. Screening the recipient against the issuer's on-chain blocklist blocks a known-bad/frozen destination before signing. (Circle / Centre FiatTokenV2 isBlacklisted.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:transfer
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blocklist for (asset=token, address=recipient).
// Asset-scoped to the transferred token (USDC). ENRICHMENT — dormant until the
// dispatcher serves the method; the `context.custom has frozen` guard is inert until then.
@id("usdc-blocklisted-recipient-deny")
@severity("deny")
@reason("Recipient is on the USDC issuer blocklist (frozen) — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
