@id("user/min-output-usd-floor")
@severity("deny")
@reason("Minimum output value is below floor")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.outputToken.amount.kind == "min" &&
  context has custom &&
  context.custom has totalMinOutputUsd &&
  context.custom.totalMinOutputUsd.value.lessThan(decimal("50.0000"))
};
