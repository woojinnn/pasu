// blocks: a transfer of an ERC-20 whose contract is on the user/operator denied-asset list
// why:    asset-denylisting (ThreatLocker block-policy) hard-blocks interaction with tokens flagged as scam/honeypot/malicious; interacting (even transferring) a denied token can trigger malicious transfer hooks — deny-tier
// bucket: wallet/token-denylist
// tags: bucket:wallet cat:token-denylist static action-family:asset-denylist could-be-compliance:true
// VOCAB §4: erc20_transfer binds the token as `token` (TokenRef) → contract address
// is `context.token.key.address` (NOT `context.asset`). `address` optional — has-guarded.
// Denied-asset set is user/operator-configured (lowercase 0x; placeholders below). Pure-static.
@id("token-asset-denylist")
@severity("deny")
@reason("This token is on your denied-asset list — transfer blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.token.key has address &&
  ["0x000000000000000000000000000000000000dead", "0x2222222222222222222222222222222222222222"].contains(context.token.key.address)
};
