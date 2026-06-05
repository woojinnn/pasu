// blocks: an ERC-20 approve granting allowance to a spender not on the user's allowlist
// why:    approvals are the #1 drainer vector; confining approved spenders to a vetted allowlist (Coinbase wallet policy) blocks allowance grants to unknown/malicious contracts before signing
// bucket: wallet/spender-allowlist
// tags: bucket:wallet cat:spender-allowlist static action-family:spender-allowlist
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("spender-allowlist")
@severity("deny")
@reason("Approval spender is not on your allowlist — approve blocked")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.spender)
};
