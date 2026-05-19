@id("default/aerodrome/forbid-zero-amount-lp-unstake")
@severity("warn")
@reason("LP unstake with zero amount is a wasted transaction")
forbid (
  principal,
  action == Action::"lp_unstake",
  resource
) when {
  context.amount has value &&
  context.amount.value == "0"
};
