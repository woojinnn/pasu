// scopeball default v2 policy — explorer id: BRG-03 (unlimited / over-balance
// ERC20 approval to a bridge contract — only a finite, ~bridge-amount approval
// is acceptable).
//
// In the LI.FI exploit (2024-07, ~$11.6M) only wallets holding an UNLIMITED
// approval to the bridge were drained; wallets with a finite approval were
// unharmed. This is the bridge instance of GEN-01: the approve lands as
// `Token::Erc20Approve` with `amount` a raw U256 hex String, so "unlimited"
// CANNOT be tested in Cedar directly — it cannot compare a hex String
// numerically. The unlimited verdict is supplied as the enriched Bool
// `approvalIsUnlimited`, projected into `context.custom` by the REUSED
// `approval.unlimited_over_balance` policy_rpc call (sim-server wallet-state
// read of approval shape vs available balance). `custom` is optional, so the
// guard `has`-checks the path; the call is `optional` and fail-opens until the
// sim-server fact host ships.
//
// `approvalAmountOverBalance` (decimal) is also projected for over-balance
// scoring; this slice keys the deny on the unlimited flag, leaving the
// amount-far-exceeds-bridge-send threshold to a later increment. Deny applies
// to every spender — an unlimited bridge approval is never the safe default.
@id("bridge-unlimited-approval-deny")
@severity("deny")
@reason("Unlimited token approval to a bridge — a drained bridge could take your full balance")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has approvalIsUnlimited
  && context.custom.approvalIsUnlimited
};
