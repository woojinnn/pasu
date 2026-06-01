// Default v2 policy — confirm before signing a Permit2 token allowance.
//
// EIP-712 `Permit2` allowance signatures (`permit2_sign_allowance`) grant a
// `spender` an off-chain-authorized allowance on the underlying token — a major
// phishing surface. Like the HyperLiquid agent-approval confirm, we warn on
// every such signature so the user reviews the decoded spender / amount /
// deadline before approving. Unconditional (no `when`): `spender`/`amount` are
// statically decoded, so this manifest declares no policy_rpc calls.
@id("permit2-sign-allowance-confirm")
@severity("warn")
@reason("Signing a Permit2 token allowance — confirm the spender and amount before approving")
forbid(
  principal,
  action == Token::Action::"Permit2SignAllowance",
  resource
);
