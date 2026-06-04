// scopeball default v2 policy — Lido stETH safety. Demand signal (Telegram / X +
// Lido help docs): supplying *rebasing* stETH as liquidity on an AMM that is NOT
// rebase-aware (Uniswap v2/v3/v4, Balancer, Maverick, Trader Joe …) silently
// stops your daily staking rewards — the pool holds a fixed balance, so the
// rebase accrues to the pool rather than to you. Lido recommends wrapping to the
// non-rebasing wstETH first, or using a rebase-aware venue (Curve / Yearn).
// Source: "providing stETH as liquidity on Uniswap/1inch/Sushi may result in
// losing out on daily staking rewards" (Bitget academy; Lido bridging guide).
//
// The trigger already restricts to the rebase-UNAWARE venues, so Curve adds are
// never flagged (no false positive on the safe venue). The single enrichment Bool
// comes from a derived fold fact `lp.input_rebasing` (does ANY pooled token of
// this add carry TokenKind::Wrapped{peg_kind: Rebasing}?). `optional` → fail-open
// until the fact + Lido token classification ship. Warn, not deny.
@id("lido-rebasing-steth-as-lp-warn")
@severity("warn")
@reason("You are supplying rebasing stETH as liquidity on an AMM that does not track rebases — your daily staking rewards stop accruing to you. Wrap to wstETH first, or use a rebase-aware venue like Curve.")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has lpInputRebasing && context.custom.lpInputRebasing
};
