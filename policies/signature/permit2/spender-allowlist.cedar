// Policy: only allow Permit2 signatures for known spender contracts.

@id("user/signature/permit2/spender-allowlist")
@severity("deny")
@reason("Permit2 spender is not on the allowlist")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
unless {
  ["0x1111111111111111111111111111111111111111"].contains(context.spender)
};
