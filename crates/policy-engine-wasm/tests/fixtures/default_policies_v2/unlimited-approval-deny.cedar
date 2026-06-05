// scopeball default v2 policy — explorer id: GEN-01 (unlimited / over-balance
// ERC20 approval to a non-allowlisted spender).
//
// An `approve(spender, amount)` with `amount == U256::MAX` (or an amount far
// exceeding the wallet's balance) hands `spender` an open-ended claim on the
// token — the canonical token-drain primitive. `amount` is a raw U256 hex
// String on the base context, so "unlimited" CANNOT be tested in Cedar
// directly; it arrives as the enriched Bool `approvalIsUnlimited`, projected
// into `context.custom` by the manifest's `approval.unlimited_over_balance`
// policy_rpc call (see manifest.json). `custom` is optional, so the guard
// `has`-checks the path before reading it.
//
// `approvalAmountOverBalance` (decimal, approve-amount / available-balance) is
// also projected for over-balance scoring; this slice keys the deny on the
// unlimited flag, leaving the over-balance threshold to a later increment.
//
// The allowlist (spenders for whom an unlimited approval is expected, e.g. the
// canonical Permit2 contract) is user configuration; for this slice it is
// inlined as a Cedar set literal (design proposal F#2 — personalized supply via
// enrichment/Set<String> custom field is a later increment). Addresses are
// lowercase hex; the sample entry below is the canonical Permit2 contract.
@id("unlimited-approval-deny")
@severity("deny")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has approvalIsUnlimited
  && context.custom.approvalIsUnlimited
  && !(["0x000000000022d473030f116ddee9f6b43ac78ba3"].contains(context.spender))
};
