@id("user/permit-max-amount")
@severity("deny")
@reason("Permit2 max uint160 amount 정확 매칭 — drainer signature 의 흔한 패턴 (2024 $494M loss)")
forbid (
  principal,
  action == Action::"permit",
  resource
) when {
  context has amount &&
  context.amount has value &&
  context.amount.value == "1461501637330902918203684832716283019655932542975"
};
