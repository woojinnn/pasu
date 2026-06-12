// dambi default v2 policy — explorer id: GEN-15 (`increaseAllowance` stacking
// onto an existing approval).
//
// `increaseAllowance(spender, addedValue)` ADDS to whatever allowance already
// exists, so a small-looking `addedValue` can push the effective allowance far
// past the wallet's balance once the current on-chain allowance is folded in.
// The resulting allowance is `current_allowance (State₁) + addedValue` — a U256
// sum (Aggregator type 1 — State read + reducer). U256 is NOT Cedar-comparable,
// so the result is scored off-Cedar against the wallet balance: the manifest's
// `approval.resulting_allowance_state` policy_rpc call projects the post-increase
// over-balance ratio into `context.custom.resultingAllowanceOverBalance` and the
// unlimited flag into `context.custom.resultingAllowanceUnlimited`.
//
// `custom` is optional, so each branch `has`-checks its field;
// `resultingAllowanceOverBalance` is a `decimal` (use `.greaterThan(...)`),
// `resultingAllowanceUnlimited` is a `Bool`. The state allowance read lives in
// the sim-server fact host, so the call is `optional` and the policy fail-opens
// until it ships. The over-balance threshold is user configuration; for this
// slice it is inlined (5× balance) and OR'd with the unlimited flag.
@id("increase-allowance-cap-warn")
@severity("warn")
@reason("Adding to your existing allowance pushes it well past your balance")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && (
    (context.custom has resultingAllowanceUnlimited
       && context.custom.resultingAllowanceUnlimited)
    || (context.custom has resultingAllowanceOverBalance
       && context.custom.resultingAllowanceOverBalance.greaterThan(decimal("5.0000")))
  )
};
