// Policy: block Permit2 approvals above a human token amount threshold.

@id("user/signature/permit2/max-human-amount-50")
@severity("deny")
@reason("Permit2 amount exceeds configured maximum")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.amountHumanClampedAtCeiling
  || context.amountHuman.greaterThan(decimal("50.0000"))
};
