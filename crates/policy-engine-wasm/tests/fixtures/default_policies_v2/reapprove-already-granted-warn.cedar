// scopeball default v2 policy — explorer id: GEN-20 (re-approving a spender you
// already granted).
//
// Approving a spender you have ALREADY approved is usually redundant, and a
// surprise re-approval prompt (especially right after a fresh grant) is a common
// phishing tell — the dapp is quietly bumping or refreshing an allowance. The
// check is "does a non-zero allowance for `(token, spender)` already exist in
// wallet state?" (Aggregator type 1 — State read: `approvals_erc20.amount > 0`).
// The manifest's `approval.already_granted` policy_rpc call projects the answer
// into `context.custom.alreadyGranted`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a `Bool`.
// The allowance read lives in the sim-server fact host, so the call is `optional`
// and the policy fail-opens until it ships. This is a low-severity informational
// nudge (warn, not deny).
@id("reapprove-already-granted-warn")
@severity("warn")
@reason("You already have an allowance for this spender — this approval may be redundant")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has alreadyGranted
  && context.custom.alreadyGranted
};
