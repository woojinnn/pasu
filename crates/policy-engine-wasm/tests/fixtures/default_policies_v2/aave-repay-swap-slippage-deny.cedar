// dambi default v2 policy — explorer id: AAVE-13 (repay-with-collateral /
// collateral-swap has excessive embedded-swap slippage).
//
// EXTERNAL fact (ORACLE vs calldata). Repay-with-collateral and collateral swaps
// run an internal swap; a loose minAmountOut/slippage floor bleeds value ('26-03
// a $50M swap filled for $36K, ~$34M extracted by MEV). The Repay action context
// carries NO swap-floor field, so the floor (from decoded calldata) and the fair
// price (from the oracle) are both read externally: the manifest declares
// `external.repay_swap_slippage_bps`, projected into
// `context.custom.repaySwapSlippageBps` (Long, negative = unfavourable). Deny when
// realized slippage is worse than -300bp (3%). Defined-not-wired → `optional`.
@id("aave-repay-swap-slippage-deny")
@severity("deny")
@reason("The embedded swap in this repay-with-collateral has excessive slippage")
forbid(principal, action == Lending::Action::"Repay", resource)
when {
  context has custom
  && context.custom has repaySwapSlippageBps
  && context.custom.repaySwapSlippageBps < -300
};
