@id("default/aerodrome/forbid-zero-amount-lp-stake")
@severity("warn")
@reason("LP stake with zero amount is a wasted transaction")
forbid (
  principal,
  action == Action::"lp_stake",
  resource
) when {
  context.amount has value &&
  context.amount.value == "0"
};
