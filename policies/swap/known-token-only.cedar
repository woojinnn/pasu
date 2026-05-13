@id("user/known-token-only")
@severity("deny")
@reason("Swap involves unknown token with empty symbol")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.tokenIn.symbol == "" ||
  context.tokenOut.symbol == ""
};
