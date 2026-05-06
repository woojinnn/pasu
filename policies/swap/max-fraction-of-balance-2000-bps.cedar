@id("user/max-fraction-of-balance-2000-bps")
@severity("deny")
@reason("Swap input exceeds 20% of the actor's balance in this token")
forbid (principal, action == Action::"swap", resource)
when {
  context has "inputFractionOfBalanceBps"
  && context.inputFractionOfBalanceBps > 2000
};
