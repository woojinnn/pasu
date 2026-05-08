// Policy: block EIP-2612 permits with an unlimited uint256 value.

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
