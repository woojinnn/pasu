// Policy: block signature approvals with unlimited sentinel amounts.

@id("user/signature/permit2/no-unlimited-amount")
@severity("deny")
@reason("Permit2 amount is unlimited")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.isUnlimited
};

@id("user/signature/eip2612/no-unlimited-amount")
@severity("deny")
@reason("EIP-2612 value is unlimited")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.isUnlimited
};
