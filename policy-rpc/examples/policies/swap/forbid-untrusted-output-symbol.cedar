@id("user/forbid-untrusted-output-symbol")
@severity("warn")
@reason("Swap output is not in the registered token list (symbol could not be resolved)")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.outputToken.asset.kind == "erc20" &&
  context.outputToken.asset.symbol == ""
};
