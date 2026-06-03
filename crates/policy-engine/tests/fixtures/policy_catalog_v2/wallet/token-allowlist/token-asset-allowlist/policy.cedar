// blocks: a transfer of an ERC-20 whose contract is not on the user's permitted-asset list
// why:    asset-allowlisting (Fireblocks Transaction Authorization Policy) confines moves to vetted tokens; a transfer of an unlisted token (airdropped-scam, counterfeit, or honeypot asset) trips a warn before signing
// bucket: wallet/token-allowlist
// tags: bucket:wallet cat:token-allowlist static action-family:asset-allowlist
// VOCAB §4: erc20_transfer binds the token as `token` (TokenRef), so the contract
// address is `context.token.key.address` — NOT `context.asset` (lending-only).
// `address` is optional on TokenKey (absent for native) — has-guarded.
// Permitted-asset set is user-configured (lowercase 0x; placeholders below). Pure-static.
@id("token-asset-allowlist")
@severity("warn")
@reason("This token is not on your permitted-asset list — confirm before transferring")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context.token.key has address &&
  !(["0x1111111111111111111111111111111111111111", "0x2222222222222222222222222222222222222222"].contains(context.token.key.address))
};
