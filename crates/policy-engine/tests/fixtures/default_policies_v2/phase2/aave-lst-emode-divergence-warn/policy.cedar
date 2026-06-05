// scopeball default v2 policy — explorer id: AAVE-09 (looping a depeg-prone
// LST/LRT into a high-LTV eMode category).
//
// Composite: STATE LST classification + ORACLE divergence. Repeatedly borrowing
// against an unbacked LST/LRT in a high-LTV eMode collapses on a single depeg
// (KelpDAO rsETH, '26-04, $123M bad debt, frozen withdrawals). Two facts compose:
//   - REUSE `token.peg_ratio` → `lstPegRatio` (decimal); < 1 signals depeg of the
//     eMode collateral (TokenKind.peg_to vs market price_value).
//   - NEW `external.lst_price_divergence_bps` → `lstDivergenceBps` (Long); gap
//     between the Aave oracle price and the true market/backing price.
// Both are projected into `context.custom`. The eMode entry itself is the action.
// Defined-not-wired → both calls `optional` (fail-open). Thresholds are user config.
@id("aave-lst-emode-divergence-warn")
@severity("warn")
@reason("Looping a depeg-prone LST into high-LTV eMode while its oracle price diverges from market")
forbid(principal, action == Lending::Action::"SetEMode", resource)
when {
  context has custom
  && context.custom has lstPegRatio
  && context.custom has lstDivergenceBps
  && context.custom.lstPegRatio.lessThan(decimal("1.0000"))
  && context.custom.lstDivergenceBps > 50
};
