// Policy: v1 structural nonce sanity only; block type(uint256).max sentinel nonces.

@id("user/signature/eip2612/nonce-sanity")
@severity("deny")
@reason("EIP-2612 nonce is structurally invalid")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  !context.nonceValid
};
