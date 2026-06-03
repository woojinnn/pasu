// blocks: signing an EIP-2612 permit for an unlimited (U256::MAX) allowance
// why:    approval phishing via off-chain signature — an unlimited permit grants the spender the right to move the wallet's entire token balance, the same drainer risk as an unlimited approve but with no on-chain tx prompt
// bucket: action/approval  (precedence-won)
// tags: bucket:action cat:approval static
// Catalog B3 — block an EIP-2612 permit for an unlimited (U256::MAX) amount.
// Same drainer risk as an unlimited on-chain approve, but via an off-chain
// signature. Pure-static (amount rendered lower-hex `{:#x}`).
@id("permit-unlimited-amount")
@severity("deny")
@reason("This permit signs away an unlimited allowance — sign only a bounded amount")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context.amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
};
