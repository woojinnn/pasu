// blocks: signing an EIP-2612 permit that grants allowance to a spender flagged as a known drainer / scam.
// why:    gasless approval-phishing — a permit signature hands the spender pull rights over the token with no on-chain tx prompt; granting it to a known-malicious spender is direct, irreversible loss (revoke.cash / off-chain-signature drainer class).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.reputation could-be-compliance:true action-family:approval
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// `address.reputation` enriches whether the permit spender is a known scam /
// drainer address. ENRICHMENT — dormant until the dispatcher serves the method.
@id("permit-spender-reputation")
@severity("deny")
@reason("This permit's spender is flagged as a known-malicious address — signature blocked")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has reputationFlagged
  && context.custom.reputationFlagged == true
};
