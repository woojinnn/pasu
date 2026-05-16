@id("user/min-output-usd-floor")
@severity("deny")
@reason("Minimum output value is below floor")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.outputToken.amount.kind == "min" &&
  context has totalMinOutputUsd &&
  context.totalMinOutputUsd.value.lessThan(decimal("50.0000"))
};
