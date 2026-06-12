// dambi default v2 policy — explorer id: AMMLP-2 (remove_liquidity locks in
// in-the-loss IL).
//
// Burning a concentrated (V3/V4) position while price has drifted hard to one
// side realizes IL: you exit with mostly one token ("withdrawing at a loss").
// Unlike UNI-14 (one-sided ENTRY), this is exit-timing regret. Judging it needs
// the position's entry range (durable state) plus the current price on the
// remove path — sv = PART (the remove path does not yet carry a current_price
// LiveField). The method `lp.exit_asymmetry` folds the held position's entry
// range against the live price and emits the Bool `exitAsymmetric`. `custom`
// and the call are optional, so the path is `has`-guarded.
@id("ammlp-remove-exit-asymmetry-warn")
@severity("warn")
@reason("Removing this liquidity now locks in impermanent loss — the position has drifted heavily to one side")
forbid(principal, action == Amm::Action::"RemoveLiquidity", resource)
when {
  context has custom
  && context.custom has exitAsymmetric
  && context.custom.exitAsymmetric
};
