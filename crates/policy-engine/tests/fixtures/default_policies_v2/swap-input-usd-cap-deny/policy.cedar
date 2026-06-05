// Default v2 policy — deny a swap whose input is worth $0.05 or more (USD cap).
//
// The input USD value is computed SERVER-SIDE: this bundle's manifest declares an
// `oracle.usd_value` policy-RPC call that the policy-server `/evaluate` handler
// executes from the wallet's synced `price_usd` (no live oracle call at decision
// time — `amount / 10^decimals × price`), projecting the result into
// `context.custom.inputUsd` (a Cedar `decimal`).
//
// The call is required + non-optional, so an unpriced asset fails CLOSED at
// materialization rather than silently fail-opening a deny (the default-policy
// gate enforces this). The threshold uses `.greaterThanOrEqual` ("이상" — $0.05
// included). Unlike the token-amount cap this is decimals-correct for ANY priced
// token, so it is not gated to a specific asset.
@id("swap-input-usd-cap-deny")
@severity("deny")
@reason("This swap's input is worth $0.05 or more — blocked by your policy")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has inputUsd
  && context.custom.inputUsd.greaterThanOrEqual(decimal("0.0500"))
};
