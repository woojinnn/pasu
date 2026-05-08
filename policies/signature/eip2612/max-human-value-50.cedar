// Policy: block EIP-2612 values above a human token amount threshold.

@id("user/signature/eip2612/max-human-value-50")
@severity("deny")
@reason("EIP-2612 value exceeds configured maximum")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.valueHumanClampedAtCeiling
  || context.valueHuman.greaterThan(decimal("50.0000"))
};
