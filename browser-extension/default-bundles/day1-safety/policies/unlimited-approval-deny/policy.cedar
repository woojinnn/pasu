// scopeball default v2 policy — explorer id: GEN-01 (unlimited ERC20 approval
// to a non-allowlisted spender).
//
// An `approve(spender, amount)` with `amount == U256::MAX` hands `spender` an
// open-ended claim on the token — the canonical token-drain primitive.
//
// STATIC detection: `amount` is a raw lowercase U256 hex String (`{:#x}`, no
// leading zeros) on the BASE context, so the two canonical "unlimited" sentinels
// are matched by string equality — `uint256::MAX` (64 f's, standard ERC20
// max-approve) and `uint160::MAX` (40 f's, the Permit2-style max). No enrichment
// is needed, so this works fully offline (server-independent).
//
// NOTE: this slice was rewritten from the earlier `context.custom.approvalIsUnlimited`
// form, which depended on the `approval.unlimited_over_balance` policy_rpc method.
// That method is NOT implemented, and its output is `required: true`, so the manifest
// tripped `SystemFail` on EVERY approve (deny-all) once the bundle's manifest was
// actually shipped. The static check denies ONLY a genuinely unlimited approve and
// lets bounded approvals pass. (Over-balance scoring — approve-amount vs holdings —
// is dropped here; it needs balance enrichment and returns in a later increment.)
//
// The allowlist (spenders for whom an unlimited approval is expected) is inlined
// as a Cedar set literal; the entry below is the canonical Permit2 contract.
// Addresses are lowercase hex.
@id("unlimited-approval-deny")
@severity("deny")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  (context.amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
   || context.amount == "0xffffffffffffffffffffffffffffffffffffffff")
  && !(["0x000000000022d473030f116ddee9f6b43ac78ba3"].contains(context.spender))
};
