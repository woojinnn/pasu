@id("default/aerodrome/forbid-zero-amount-lp-stake")
@severity("warn")
@reason("LP stake with zero amount is a wasted transaction")
forbid (
  principal,
  action == Action::"lp_stake",
  resource
) when {
  context.lpToken.amount has value &&
  context.lpToken.amount.value == "0"
};
