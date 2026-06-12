// dambi default v2 policy — explorer id: UNI-14 (V3/V4 LP added one-sided,
// outside the current price → forced single-asset range order).
//
// A concentrated LP whose [tickLower, tickUpper] does not contain the current
// tick becomes a one-sided range order (100% of one asset), with outsized IL
// exposure versus intent. The range lives in `params.range` (`tickLower`/
// `tickUpper` as `Long`) and `currentPrice` is a decimal String — converting a
// tick to a comparable price is beyond Cedar, so the method
// `lp.range_out_of_position` does the tick↔price check natively and emits the
// Bool `lpOutOfRange`. `custom` and the call are optional, so the path is
// `has`-guarded.
@id("ammlp-uni-v3v4-out-of-range-warn")
@severity("warn")
@reason("This liquidity range does not include the current price — you would deposit one-sided")
forbid(principal, action == Amm::Action::"AddLiquidity", resource)
when {
  context has custom
  && context.custom has lpOutOfRange
  && context.custom.lpOutOfRange
};
