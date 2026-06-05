// blocks: an un-decoded interaction whose target or chain is in the install-time FATF "call-for-action" (blacklist) pack.
// why:    jurisdiction install-pack (static) — FATF maintains a "high-risk jurisdictions subject to a call for action" list (the FATF blacklist, e.g. North Korea, Iran, Myanmar). Jurisdiction is NOT an on-chain field, so this is a baked install-time set of contracts/chains the operator maps to a FATF call-for-action jurisdiction. Pure-static — binds no live calldata semantic. (FATF "Call for Action" high-risk jurisdictions list.)
// bucket: compliance/jurisdiction
// tags: bucket:compliance cat:jurisdiction static could-be-compliance:true action-family:jurisdiction
//
// Binds the Core::Unknown catch-all. Baked sets are install-time / operator-configured
// (lowercase 0x targets; CAIP-2 chain ids; placeholders below). Deny when the call
// target is in the FATF-blacklist contract set OR the chain is a mapped blacklist chain.
@id("jurisdiction-fatf-blacklist-pack-deny")
@severity("deny")
@reason("Target or chain is in the FATF call-for-action (blacklist) pack — interaction blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["0x3333333333333333333333333333333333333333", "0x4444444444444444444444444444444444444444"].contains(context.target)
  || ["eip155:0"].contains(context.chain)
};
