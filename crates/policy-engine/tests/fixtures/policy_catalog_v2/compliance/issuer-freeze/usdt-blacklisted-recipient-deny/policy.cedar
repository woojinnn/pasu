// blocks: an ERC-20 (USDT) transfer whose recipient the issuer has blacklisted on-chain (Tether isBlackListed).
// why:    issuer-freeze screening — Tether's USDT contract exposes an on-chain `isBlackListed` mapping; Tether routinely freezes addresses on law-enforcement request, after which the recipient can neither receive nor move USDT. Screening the recipient against the issuer's on-chain blacklist blocks a frozen destination before signing. (Tether transparency / TetherToken isBlackListed.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:transfer
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blacklist for (asset=token, address=recipient).
// Asset-scoped to the transferred token (USDT). ENRICHMENT — dormant until the
// dispatcher serves the method; the `context.custom has frozen` guard is inert until then.
@id("usdt-blacklisted-recipient-deny")
@severity("deny")
@reason("Recipient is on the USDT issuer blacklist (frozen) — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
