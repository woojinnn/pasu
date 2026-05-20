@id("user/transfer-large-amount-exact")
@severity("warn")
@reason("Transfer 금액 이 정확히 1000 ETH (PoC 의 hardcoded threshold — String 비교 한계)")
forbid (
  principal,
  action == Action::"transfer",
  resource
) when {
  context.token.amount has value &&
  context.token.amount.value == "1000000000000000000000"
};
