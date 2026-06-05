// scopeball default v2 policy — cumulative approval exposure.
//
// Token approvals never expire and are almost never revoked, so they PILE UP. A
// user can hold a $2k allowance to router A, $5k to router B, $4k to dapp C —
// each individually under any single cap — yet the SUM of live allowances
// already exceeds the wallet's balance. The next approve adds to that pile.
// Approval-phishing has drained $1B+ cumulatively precisely because the surface
// is the running TOTAL, not any one grant. Single-grant gates miss this:
//   * unlimited-approval-deny     — keys on THIS grant being unlimited.
//   * increase-allowance-cap-warn — keys on (current allowance for THIS spender
//                                   + this delta) vs balance.
// Neither folds in allowances already held to OTHER spenders.
//
// `approval.cumulative_allowance_usd` = USD sum of every currently-active
// allowance this owner holds on this token across ALL spenders, PLUS this
// pending approve. This is a STATE read + reducer (Aggregator type 1): the
// sim-server must track a per-(owner,token) set of active spender→allowance
// entries and reduce to a USD total. That per-spender allowance SET does NOT
// exist in WalletState today (only a single `myAllowance` scalar), so the call
// is `optional` (fail-open) and 3-guarded — until the cumulative-allowance
// state tracker ships, the field is absent ⇒ never fires. Threshold = user
// config; inlined here at $10,000. EXPRESSIBILITY: needs-new-state.
@id("approve-cumulative-allowance-exposure-warn")
@severity("warn")
@reason("Your total live token approvals across all spenders already exceed a safe cap — unrevoked approvals pile up and are the dominant approval-phishing loss path")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has cumulativeAllowanceUsd
  && context.custom.cumulativeAllowanceUsd.greaterThan(decimal("10000.0000"))
};
