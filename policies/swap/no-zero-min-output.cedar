@id("user/no-zero-min-output")
@severity("deny")
@reason("swap must specify a non-zero minimum output (slippage protection)")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.outputToken.amount.kind == "min" &&
  context.outputToken.amount has value &&
  context.outputToken.amount.value == "0"
};
