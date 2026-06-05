// BLOCKED-BY-ACTION: SwapContext has no `deadline` field — the AMM mapper does not
// decode the router `deadline` argument out of calldata today, so there is no base
// field to compare a time axis against. Skipped by all consumers until this banner
// is removed.
//
// scopeball candidate — swap deadline missing or stale (MEV sandwich via mempool
// dwell). slippageBp bounds the PRICE axis only; this bounds the TIME axis. The
// deadline lives in nearly every DEX router calldata (Uniswap V2
// `swapExactTokensForTokens(...,deadline)`, V3 `ExactInputParams.deadline`, …) but
// `Amm::SwapContext` lowers only price/amount/slippage fields. TARGET intent: a
// mapper-decoded `deadline` (unix s) + host-derived `secondsUntilDeadline`, so both
// prongs check: (a) missing/infinite deadline (== 0 / == uint max), and (b) a
// deadline so far out the tx sits stale in the mempool and is sandwiched. Wired as
// optional/fail-open until the field lands; every read `has`-guarded. Demotes to
// cedar-only once `SwapContext.deadline: Long` exists. Evidence: Uniswap support
// (pending → MEV sandwich), Sherlock audit (deadline check ineffective).
@id("swap-deadline-missing-or-stale-warn")
@severity("warn")
@reason("This swap has no deadline (or one set far in the future) — it can sit pending in the mempool and be sandwiched at a stale price")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (
      context.custom has deadlineIsUnbounded
      && context.custom.deadlineIsUnbounded == true
    )
    || (
      context.custom has secondsUntilDeadline
      && context.custom.secondsUntilDeadline > 1800
    )
  )
};
