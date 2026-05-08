// Policy: only allow EIP-2612 permits for known spender contracts.

@id("user/signature/eip2612/spender-allowlist")
@severity("deny")
@reason("EIP-2612 spender is not on the allowlist")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
unless {
  ["0x1111111111111111111111111111111111111111"].contains(context.spender)
};
