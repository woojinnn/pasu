@id("user/allowance-must-cover-input")
@severity("warn")
@reason("Token allowance does not cover input; approve transaction required")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.tokenIn.isNative == false &&
  context has allowancesCoverInputs &&
  context.allowancesCoverInputs == false
};
