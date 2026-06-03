// blocks: an ERC-20 approve granting allowance to a spender flagged as a known drainer / scam contract.
// why:    approval-phishing — handing allowance to a known-malicious spender is a direct, irreversible drainer grant; a reputation feed is the wallet-side denylist for spenders (revoke.cash / known-exploit lists).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.reputation could-be-compliance:true action-family:approval
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// `address.reputation` enriches whether the approve spender is a known scam /
// drainer address. ENRICHMENT — dormant until the dispatcher serves the method.
@id("approve-spender-reputation")
@severity("deny")
@reason("Spender is flagged as a known-malicious address — approval blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};
