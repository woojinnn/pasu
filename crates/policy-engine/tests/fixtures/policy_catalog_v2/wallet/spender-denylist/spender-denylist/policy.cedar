// blocks: an ERC-20 approve granting allowance to a spender on the known-bad denylist
// why:    a denylist (ThreatLocker block-policy) hard-blocks approvals to addresses flagged as drainers/scam contracts; can also be fed by a shared malicious-spender feed
// bucket: wallet/spender-denylist
// tags: bucket:wallet cat:spender-denylist static action-family:spender-denylist could-be-compliance:true
// Denied-spender set is user/operator-configured (lowercase 0x; placeholders below). Pure-static.
@id("spender-denylist")
@severity("deny")
@reason("Approval spender is on your denylist of known-malicious addresses — approve blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  ["0x000000000000000000000000000000000000dead", "0x2222222222222222222222222222222222222222"].contains(context.spender)
};
