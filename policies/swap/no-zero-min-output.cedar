@id("user/no-zero-min-output")
@severity("deny")
@reason("swap must specify a non-zero minimum output (slippage protection)")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.amountOut.kind == "min" &&
  context.amountOut has value &&
  context.amountOut.value == "0"
};
