// scopeball default v2 policy — explorer id: AMMLP-4 (signing a near-duplicate of
// an already-open intent order).
//
// Re-signing a stuck/"did it go through?" order is common; if both stay live,
// double the intended size sells (or a malicious dApp double-fills). This is a
// STATE2 membership check: the method `intent.near_duplicate_pending` tests the
// new order against the active `PendingKind::OffchainLimitOrder` set (same venue,
// sell, buy, similar cap) and emits the Bool `duplicateIntent`. Distinct from the
// quantitative cap-sum (AMMLP-3). `custom` and the call are optional, so the path
// is `has`-guarded.
@id("ammlp-intent-duplicate-warn")
@severity("warn")
@reason("This order is nearly identical to one you already have open — you may be signing a duplicate")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has duplicateIntent
  && context.custom.duplicateIntent
};
