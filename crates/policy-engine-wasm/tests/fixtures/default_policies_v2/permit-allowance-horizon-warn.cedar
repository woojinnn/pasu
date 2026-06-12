// dambi default v2 policy — explorer id: GEN-03 (Permit / Permit2 signature
// granting a large allowance or a far-future expiry).
//
// An off-chain `permit` is a gasless allowance: a signature anyone can later
// submit to set a spender's allowance until `deadline`. A large USD allowance or
// a deadline months out is the same drain exposure as an unlimited on-chain
// `approve`, but easier to miss because it never hits a confirmation screen.
//
// Two enriched facts drive this (Aggregator type 2 — Valuation, plus a
// clock-relative horizon):
//   - `permitAllowanceUsd`  — `amount × price_value`. `amount` is a raw U256 hex
//                             String (NOT Cedar-comparable), so the USD product
//                             is computed off-Cedar.
//   - `permitExpiryHorizonSec` — `deadline − now`. The action's `deadline` is a
//                             Long, but the horizon needs the host clock (Cedar
//                             has no `now`), so it is computed off-Cedar.
// The manifest's `permit.allowance_horizon` policy_rpc call emits both fields
// from one call (record output; precedent `approval.unlimited_over_balance`).
//
// `custom` is optional, so each branch `has`-checks its field. `permitAllowanceUsd`
// is a `decimal` (use `.greaterThan(...)`); `permitExpiryHorizonSec` is a `Long`
// (use `>`). The price read lives in the sim-server fact host, so the call is
// `optional` and the policy fail-opens until it ships. Thresholds are user
// configuration; for this slice they are inlined ($10,000 allowance OR a horizon
// over 30 days = 2,592,000 s).
@id("permit-allowance-horizon-warn")
@severity("warn")
@reason("This off-chain permit grants a large allowance or stays valid for a long time")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && (
    (context.custom has permitAllowanceUsd
       && context.custom.permitAllowanceUsd.greaterThan(decimal("10000.0000")))
    || (context.custom has permitExpiryHorizonSec
       && context.custom.permitExpiryHorizonSec > 2592000)
  )
};
