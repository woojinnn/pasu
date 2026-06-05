// blocks: signing an EIP-2612 permit whose spender is flagged as a known drainer / phishing contract
// why:    permit phishing via reputation feed — off-chain EIP-2612 signatures bypass weak wallet warnings, so a spender flagged by a malicious-address feed must be hard-blocked
// bucket: action/approval  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:approval enrichment method:address.reputation could-be-compliance:sanctions-feed
// methods: address.reputation -> ../../../_methods/address.reputation.md
// Catalog B1 — block an EIP-2612 permit to a flagged spender.
//
// Off-chain `permit` signatures are a top phishing surface (no on-chain tx, weak
// wallet warnings). `address.reputation` enriches whether the spender is a known
// drainer / phishing contract. ENRICHMENT — dormant until the dispatcher serves
// `address.reputation`. Threat refs: SlowMist permit phishing; ScamSniffer; Revoke.cash EIP-2612.
@id("permit-unknown-spender")
@severity("deny")
@reason("This EIP-2612 permit authorizes a spender flagged as malicious — do not sign")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has spenderFlagged
  && context.custom.spenderFlagged == true
};
