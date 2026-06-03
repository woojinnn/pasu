// blocks: a Permit2 signed allowance delegating spend to a spender not on the user's allowlist
// why:    a Permit2 (Uniswap permit2) signature is a gasless off-chain allowance grant — equally drainer-prone as on-chain approve; an allowlist on `spender` blocks signing a standing pull for an unknown contract (Coinbase wallet policy)
// bucket: wallet/spender-allowlist
// tags: bucket:wallet cat:spender-allowlist static venue:permit2 action-family:spender-allowlist
// Off-chain signature (meta.nature.kind == "offchain_sig"); the on-chain Erc20Approve twin is covered by spender-allowlist.
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("permit2-spender-allowlist")
@severity("deny")
@reason("Permit2 spender is not on your allowlist — signature blocked")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.spender)
};
