// dambi default v2 policy — explorer id: UNI-04 (contract floor much worse
// than the on-screen quote).
//
// A malicious dApp shows a good quote but embeds a far harsher `minAmountOut`
// floor in the calldata, pocketing the gap. The contract floor lives in
// `direction.minAmountOut` — a raw U256 HEX String — and the fair rate comes
// from an oracle, so Cedar CANNOT compute the divergence: the bp gap arrives
// as the enriched `Long` `floorVsFairBp`, projected into `context.custom` by
// the manifest's `external.swap_floor_vs_fair_bps` call. `custom` is optional
// and the call is `optional` (fail-open until the provider ships), so the guard
// `has`-checks the path before reading it. 300 bp is the warn tolerance.
@id("swap-floor-vs-fair-warn")
@severity("warn")
@reason("The contract's worst-case received amount is far below the oracle fair value for this swap")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has floorVsFairBp
  && context.custom.floorVsFairBp > 300
};
