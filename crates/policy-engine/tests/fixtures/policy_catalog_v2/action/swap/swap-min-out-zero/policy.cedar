// blocks: an exact-input swap with minAmountOut == 0, which accepts any output amount and removes all slippage protection
// why:    zero-min-out / unbounded-slippage — a swap with no output floor can be fully drained by a sandwich attack or routed through a malicious pool for ~0 return
// bucket: action/swap  (precedence-won)
// tags: bucket:action cat:swap static action-family:min-out-zero
// Catalog C3 — warn on an exact-input swap with zero minimum output.
// `minAmountOut == 0` accepts any output, removing all slippage protection.
// Pure-static: minAmountOut is present only for exact_input (guard with `has`);
// zero renders as `0x0` (`{:#x}`).
@id("swap-min-out-zero")
@severity("warn")
@reason("This swap accepts any output amount (min-out = 0) — no slippage protection at all")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.direction.kind == "exact_input"
  && context.direction has minAmountOut
  && context.direction.minAmountOut == "0x0"
};
