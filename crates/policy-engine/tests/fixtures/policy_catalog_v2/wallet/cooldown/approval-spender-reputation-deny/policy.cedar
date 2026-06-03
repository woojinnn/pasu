// blocks: an ERC-20 approval granting spend rights to a spender flagged as a known-malicious drainer.
// why:    Coinbase-style malicious-spender block — an approval to a flagged drainer hands over a standing pull on the token; once granted the attacker can drain at will, so a known-bad spender must be denied outright, not merely warned (malicious-spender threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.reputation action-family:cooldown could-be-compliance:scam-feed
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// Cooldown — DENY an approval whose spender is reputation-flagged.
// `address.reputation` enriches whether the spender is a known scam / drainer.
// ENRICHMENT — dormant.
@id("approval-spender-reputation-deny")
@severity("deny")
@reason("Spender is flagged as a known-malicious address — approval blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderFlagged
  && context.custom.spenderFlagged == true
};
