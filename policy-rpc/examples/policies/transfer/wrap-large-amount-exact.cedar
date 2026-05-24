@id("user/wrap-large-amount-exact")
@severity("warn")
@reason("WRAP_ETH 금액 이 정확히 100 ETH (PoC 의 hardcoded threshold)")
forbid (
  principal,
  action == Action::"wrap",
  resource
) when {
  context.nativeAsset.amount has value &&
  context.nativeAsset.amount.value == "100000000000000000000"
};
