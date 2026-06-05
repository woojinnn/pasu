// scopeball default v2 policy — explorer id: AIR-05 (a claim — or a standalone
// delegate — hands governance voting power to someone else).
//
// ACTION-FIELD + LITERAL SET-MEMBERSHIP (Aggregator type 5; no enrichment). A
// malicious page sets `delegatee` to an attacker. The balance delta is zero, so
// value-diff detection misses it — the decoded `delegatee` field is the only
// signal. This wires to the `Airdrop::Delegate` action, whose `delegatee` is a
// non-optional base `String` field of `DelegateContext`. "Self" is
// `principal.address` (the evaluated `Wallet`); the personal allowlist is LOCAL
// user config kept as a Cedar `Set<String>` literal (mirrors GEN-01 / AAVE-05),
// so the manifest declares NO `policy_rpc` and NO `custom_context`. Deny when
// the delegatee is neither self nor allowlisted. Addresses are lowercase hex;
// the allowlist entries below are placeholders for the user's list.
@id("air-delegatee-not-self-deny")
@severity("deny")
@reason("This delegates your voting power to an address that is not you or on your allowlist")
forbid(principal, action == Airdrop::Action::"Delegate", resource)
when {
  context.delegatee != principal.address
  && !(["0x000000000000000000000000000000000000beef",
        "0x0000000000000000000000000000000000c0ffee"].contains(context.delegatee))
};
