@id("user/min-output-usd-floor")
@severity("deny")
@reason("Minimum output value is below  floor")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has totalMinOutputUsd &&
  context.outputToken.amount.kind == "min" &&
  context.totalMinOutputUsd.value.lessThan(decimal("50.0000"))
};
