// blocks: an exact-output swap with an unbounded (uint256-max) maximum input, which removes the spend cap and lets the route consume an arbitrary amount of the input token.
// why:    unbounded-input drain — an exact-output swap with maxAmountIn set to uint256-max has no upper bound on what it pays, so a manipulated pool / sandwich can extract far more input than expected (source: Uniswap docs).
// bucket: action/swap
// tags: bucket:action cat:swap static action-family:swap
// Pure-static: maxAmountIn is present only for exact_output (guard with `has`).
// The uint256-max sentinel (0xff..×64) means no upper bound on input spent.
@id("swap-exact-output-unbounded-input")
@severity("warn")
@reason("This swap places no upper limit on the input spent (unbounded max-in) — confirm the spend")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.direction.kind == "exact_output"
  && context.direction has maxAmountIn
  && context.direction.maxAmountIn == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
};
