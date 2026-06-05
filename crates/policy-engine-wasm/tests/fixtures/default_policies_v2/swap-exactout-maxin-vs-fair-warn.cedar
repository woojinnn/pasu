// scopeball default v2 policy — explorer id: UNI-10 (exact-out max-in far above
// fair value).
//
// An exact-out swap ("receive exactly N") carries a `maxAmountIn` ceiling; if
// that ceiling is far above fair value, a thin pool can quietly charge you up to
// the limit while you only watch the output. `direction.maxAmountIn` and
// `direction.amountOut` are raw U256 HEX Strings, so Cedar cannot compute the
// fair-value gap; the method `external.swap_maxin_vs_fair_bps` converts amountOut
// at the oracle fair price and reports the overpay as the enriched `Long`
// `maxinVsFairBp`. `custom` and the call are optional, so the path is
// `has`-guarded. 300 bp tolerance here.
@id("swap-exactout-maxin-vs-fair-warn")
@severity("warn")
@reason("This exact-output swap's maximum payment is well above the fair price for the amount received")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has maxinVsFairBp
  && context.custom.maxinVsFairBp > 300
};
