// scopeball default v2 policy — explorer id: AMMLP-3 (stacked open intents whose
// PermitCap sum exceeds the wallet balance → double-spend exposure).
//
// CoW/UniswapX/Fusion signed orders each hold a spend-cap on the sell token
// until filled. Several unfilled orders can have caps summing above the held
// balance, so the "same coin sells in two places" or fills badly all at once on
// a market swing. This is a STATE2 fold: the method `intent.pending_cap_over_balance`
// sums the active `PendingKind::OffchainLimitOrder` PermitCaps (reducer state)
// PLUS the new order's sell amount and compares against the balance, emitting the
// Bool `capSumOverBalance`. `custom` and the call are optional, so the path is
// `has`-guarded.
@id("ammlp-intent-cap-over-balance-warn")
@severity("warn")
@reason("Your open signed orders already commit more than your balance of this token")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has capSumOverBalance
  && context.custom.capSumOverBalance
};
