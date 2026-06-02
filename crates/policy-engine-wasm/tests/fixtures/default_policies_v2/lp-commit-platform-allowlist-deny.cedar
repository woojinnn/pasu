// scopeball default v2 policy — explorer id: LP-02 (commit / approve on a
// launchpad platform outside the personal allowlist).
//
// LITERAL SET-MEMBERSHIP. Fake clones of legitimate launchpads (e.g. PinkSale)
// are the real loss path: the user is lured into committing funds on a
// look-alike. This reduces to a destination-allowlist control — "is this a
// platform I authorised?" — so it needs no wallet state (sv = COV).
//
// The allowlist is LOCAL user config kept as a Cedar `Set<String>` literal
// (mirrors the GEN-01 / VALUE / AAVE-05 allowlist pattern), so the manifest
// declares NO `policy_rpc` and NO `custom_context`. The platform is carried as
// `context.platform.name` — the `name` of a `Core::ProtocolRef` — a
// non-optional base `String`. Deny when the platform name is NOT allowlisted.
// Entries below are placeholders for the user's list (protocol slugs, not
// addresses, since `ProtocolRef` is name-based).
@id("lp-commit-platform-allowlist-deny")
@severity("deny")
@reason("Committing to a launchpad platform not on your allowlist is blocked")
forbid(principal, action == Launchpad::Action::"Commit", resource)
when {
  !(["coinlist", "fjord", "pinksale"].contains(context.platform.name))
};
