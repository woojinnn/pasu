// blocks: withdrawing collateral from an open perp position (signed delta < 0) without confirmation, which raises effective leverage and moves the liquidation price closer.
// why:    AdjustMargin with a negative delta pulls margin out of a live position; a spoofed/relayed margin-withdraw reduces the safety buffer and can push the position toward liquidation (margin-pull risk). Source: perp venue EIP-712 adjust-margin docs.
// bucket: action/perp
// tags: bucket:action cat:perp static action-family:margin-withdraw
// Pure-static: `delta` is a SignedI256 exposed as a String (positive = deposit,
// negative = withdraw). HL/perp numerics are decimal strings — match sign with a
// `like "-*"` pattern, never a magnitude compare.
@id("perp-margin-withdraw-confirm")
@severity("warn")
@reason("This removes collateral from an open position — confirm; your leverage rises and liquidation price moves closer")
forbid(principal, action == Perp::Action::"AdjustMargin", resource)
when {
  context.delta like "-*"
};
