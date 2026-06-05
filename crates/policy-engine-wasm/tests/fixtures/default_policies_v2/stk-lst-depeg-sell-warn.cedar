// scopeball default v2 policy — explorer id: STK-01 (selling/collateralizing a
// depegged LST/LRT at its discounted market price).
//
// COMPOSITE state+oracle, NOT a pure action-field. The token being sold
// (`tokenIn` of the swap) is a Core::TokenRef record, not a comparable scalar —
// Cedar cannot read "how far has this LST drifted from its redemption peg" from
// the action. The state model classifies LST/LRT via TokenKind::Wrapped (peg_to)
// and carries a secondary-market price_value, and `token.peg_ratio` folds those
// into market/peg — < 1 signals depeg (stETH 0.93 in 2022 ~$180M liquidations;
// ezETH -79% in one hour ~$56M). REUSE `token.peg_ratio` → `lstPegRatio`
// (decimal), projected into context.custom. Selling/collateralizing decodes as
// Amm::Swap (no dedicated staking domain). Defined-not-wired → call is
// `optional` (fail-open). Heads-up only — warn, not deny. Threshold 0.98 is
// user config.
@id("stk-lst-depeg-sell-warn")
@severity("warn")
@reason("This LST/LRT is trading below its redemption peg — selling or collateralizing it now locks in the discount")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has lstPegRatio
  && context.custom.lstPegRatio.lessThan(decimal("0.9800"))
};
