// scopeball default v2 policy — explorer id: STK-04 (LST/LRT over-concentration).
//
// WHOLE-WALLET FOLD, not an action-field. Acquiring more of an LST/LRT can push
// it past a safe share of the whole portfolio; one depeg (stETH/ezETH) then
// drags the entire book. Answering "what % of my portfolio is this LST after the
// trade" needs every holding valued in USD (State₂) — calldata can't. REUSE the
// fold fact `portfolio.group_pct` with group_axis=category: LST/LRT is a holding
// CLASSIFICATION (TokenKind::Wrapped) and the fold buckets holdings by that
// classification, summing the LST group over total portfolio USD after the
// action is applied via the reducer. Projected into context.custom →
// `lstPctOfPortfolio` (decimal). Defined-not-wired → `optional` (fail-open).
// Heads-up — warn, not deny. Cap 35.0000 is user config (opt-in family).
@id("stk-lst-concentration-warn")
@severity("warn")
@reason("After this trade your LST/LRT exposure would exceed your portfolio-concentration cap")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has lstPctOfPortfolio
  && context.custom.lstPctOfPortfolio.greaterThan(decimal("35.0000"))
};
