// scopeball default v2 policy — explorer id: AAVE-07 (one borrow consumes too
// large a share of remaining borrow capacity).
//
// DERIVED fact (NOT pure action-field). 'Never borrow more than half your
// capacity at once' is the most common safety rule. The denominator
// (UserLendingState.availableBorrowUsd) is carried in the base context only as a
// U256 hex string, and the numerator (amountUsd) only as an optional decimal —
// Cedar cannot form the ratio from those, so the manifest declares
// `position.borrow_fraction_bps` which prices the borrow and divides by the
// precomputed available-borrow USD, projected into `context.custom.borrowFractionBps`
// (Long basis points). Warn above 5000bp (50%). Defined-not-wired → `optional`.
@id("aave-borrow-fraction-warn")
@severity("warn")
@reason("This borrow uses more than half of your remaining borrow capacity at once")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has borrowFractionBps
  && context.custom.borrowFractionBps > 5000
};
