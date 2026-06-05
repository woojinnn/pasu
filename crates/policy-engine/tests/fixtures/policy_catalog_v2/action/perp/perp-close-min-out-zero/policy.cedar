// blocks: a perp close whose explicit size amount is zero (size.amount == 0x0) — a degenerate close that fills no base yet may still be signed/relayed, or accepts any fill outcome.
// why:    a close order with a zero/unconstrained size leg has no execution floor and can be filled at an adversarial price (any-fill close). ClosePosition carries no minOut field, so the zero-size leg is the only static signal of an unbounded close. Source: perp venue EIP-712 order docs.
// bucket: action/perp
// tags: bucket:action cat:perp static action-family:min-out-zero
// Pure-static: `size` is optional (None ≡ full close) and `size.amount` is itself
// optional on SizeSpec — both `has`-guarded. amount is U256 hex; only the exact
// zero sentinel `0x0` is a valid equality compare.
@id("perp-close-min-out-zero")
@severity("warn")
@reason("This close order specifies a zero size — confirm; it may fill at any price or do nothing")
forbid(principal, action == Perp::Action::"ClosePosition", resource)
when {
  context has size
  && context.size has amount
  && context.size.amount == "0x0"
};
