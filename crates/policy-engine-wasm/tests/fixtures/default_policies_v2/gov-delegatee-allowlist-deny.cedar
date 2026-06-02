// scopeball default v2 policy — explorer id: GOV-01 (governance voting power
// delegated to an unknown/unexpected address).
//
// LITERAL SET-MEMBERSHIP (no enrichment). Delegation phishing is the #1
// governance attack surface — a MakerDAO delegate lost $11M to a delegation
// phish, and >90% of delegation prompts targeting a user are reported malicious.
// The balance delta is zero, so value-diff detection misses it entirely; the
// decoded `delegatee` is the only signal. This wires to `Airdrop::Delegate`
// (ERC20Votes/Comp `delegate(address)`), whose `delegatee` is a non-optional
// base `String` field of `DelegateContext`. "Self" is `principal.address`; the
// personal allowlist is LOCAL user config kept as a Cedar `Set<String>` literal
// (mirrors GEN-01 / AAVE-05), so the manifest declares NO `policy_rpc` and NO
// `custom_context`. Deny when the delegatee is neither self nor allowlisted.
// "내 현재 delegate"를 담는 durable State가 없어 *변경* 탐지는 반쪽(PARTIAL)이나,
// allowlist 밖 위임 차단은 Action 필드만으로 표현 가능. Addresses are lowercase
// hex; the allowlist entries below are placeholders for the user's list.
@id("gov-delegatee-allowlist-deny")
@severity("deny")
@reason("This delegates your governance voting power to an address that is not you or on your allowlist")
forbid(principal, action == Airdrop::Action::"Delegate", resource)
when {
  context.delegatee != principal.address
  && !(["0x000000000000000000000000000000000000beef",
        "0x0000000000000000000000000000000000c0ffee"].contains(context.delegatee))
};
