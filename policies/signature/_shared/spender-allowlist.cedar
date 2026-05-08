// Policy: only allow signature approvals for known spender contracts.

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
