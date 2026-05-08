// Policy: block Permit2 signatures with an unlimited uint160 amount.

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
