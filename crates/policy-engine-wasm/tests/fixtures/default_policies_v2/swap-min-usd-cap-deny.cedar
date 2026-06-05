// scopeball default v2 policy — deny any swap whose input is worth $0.05 or more.
//
// The swap's input USD value is the host-populated `direction.amountInUsd`
// projection (3-layer amount: raw hex / token-native nano / fiat USD — see
// `Amm::SwapDirection`). It is the fiat sibling of `amountIn`, valued by the
// oracle, and surfaced to Cedar as a `decimal` extension so the threshold uses
// `.greaterThanOrEqual(...)` ("이상" = at or above the cap).
//
// `amountInUsd` is optional (Host-populated, present only on exact-input swaps
// once valuation lands), so the guard `has`-checks the path and the policy
// fail-opens when the projection is absent. The cap is inlined ($0.05) per the
// user request — personalized supply is a later increment.
@id("swap-min-usd-cap-deny")
@severity("deny")
@reason("This swap's value is at or above the $0.05 cap and is not allowed")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.direction has amountInUsd
  && context.direction.amountInUsd.greaterThanOrEqual(decimal("0.0500"))
};
