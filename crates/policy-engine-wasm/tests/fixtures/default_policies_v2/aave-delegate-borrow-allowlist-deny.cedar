// scopeball default v2 policy — explorer id: AAVE-05 (credit-delegation to a
// non-allowlisted delegatee).
//
// LITERAL SET-MEMBERSHIP. Aave credit delegation lets another address borrow
// against YOUR collateral — most users never do this intentionally, so granting
// it to an address outside a personal allowlist is a strong phishing signal. The
// allowlist is LOCAL user config kept as a Cedar `Set<String>` literal (mirrors
// the GEN-01 / VALUE denylist pattern), so the manifest declares NO `policy_rpc`
// and NO `custom_context`. `delegatee` is a non-optional base String field of
// `DelegateBorrowContext`. Deny when the delegatee is NOT allowlisted. Addresses
// are lowercase hex; the entries below are placeholders for the user's list.
@id("aave-delegate-borrow-allowlist-deny")
@severity("deny")
@reason("Credit delegation to an address not on your allowlist is blocked")
forbid(principal, action == Lending::Action::"DelegateBorrow", resource)
when {
  !(["0x000000000000000000000000000000000000beef",
     "0x0000000000000000000000000000000000c0ffee"].contains(context.delegatee))
};
