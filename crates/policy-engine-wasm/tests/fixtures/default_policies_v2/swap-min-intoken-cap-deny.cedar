// scopeball default v2 policy — deny any swap selling 0.05 or more of the input
// token (token-amount basis, NOT USD).
//
// The raw `direction.amountIn` is a U256 hex String (not Cedar-comparable), so
// the threshold reads the host-populated `direction.amountInNano` sibling — a
// `Long` in token-native nano units (`raw × 10^(9 − decimals)`), i.e.
// `token_amount × 10^9` regardless of the token's decimals. So 0.05 tokens =
// 0.05 × 10^9 = 50_000_000 nano. The comparison is a plain Long `>=` ("이상").
//
// `amountInNano` is optional (Host-populated, present on exact-input swaps once
// the nano projection lands), so the guard `has`-checks the path and the policy
// fail-opens when it is absent. The cap is inlined (0.05 token) per the user
// request — personalized supply is a later increment.
@id("swap-min-intoken-cap-deny")
@severity("deny")
@reason("This swap sells 0.05 or more of the input token and is not allowed")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.direction has amountInNano
  && context.direction.amountInNano >= 50000000
};
