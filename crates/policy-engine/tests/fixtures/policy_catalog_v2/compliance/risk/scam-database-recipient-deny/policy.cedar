// blocks: an ERC-20 transfer whose recipient appears in a crowd-sourced scam/abuse database (e.g. Chainabuse / ScamSniffer / Etherscan nametag "Fake_Phishing").
// why:    risk-category screening — a recipient on a community scam denylist is a known phishing/drainer/fraud address; sending value is direct, irreversible loss. Reuses the address.reputation flag as the scam-database signal. (Chainabuse / ScamSniffer reports; reputation flag = scam.)
// bucket: compliance/risk
// tags: bucket:compliance cat:risk enrichment method:address.reputation could-be-compliance:true action-family:transfer
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// `address.reputation` projects a crowd-sourced scam-database flag into
// `context.custom.reputationFlagged`. ENRICHMENT — dormant until wired; the
// `has reputationFlagged` guard is inert until then.
@id("scam-database-recipient-deny")
@severity("deny")
@reason("Recipient appears in a scam/abuse database (known phishing/drainer) — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};
