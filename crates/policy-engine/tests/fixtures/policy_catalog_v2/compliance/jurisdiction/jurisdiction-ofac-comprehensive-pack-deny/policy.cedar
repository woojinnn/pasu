// blocks: an un-decoded interaction whose target or chain is in the install-time OFAC comprehensive-program pack.
// why:    jurisdiction install-pack (static) — OFAC runs comprehensive country sanctions programs (e.g. Cuba, Iran, North Korea, Syria, Crimea/DNR/LNR regions). Jurisdiction is NOT an on-chain field, so this is expressed as a baked install-time set of sanctioned-program contracts (`target`) plus any chain ids the operator maps to a comprehensively-sanctioned jurisdiction (`chain`). Pure-static — binds no live calldata semantic. (OFAC country/region sanctions programs.)
// bucket: compliance/jurisdiction
// tags: bucket:compliance cat:jurisdiction static could-be-compliance:true action-family:jurisdiction
//
// Binds the Core::Unknown catch-all. Baked sets are install-time / operator-configured
// (lowercase 0x targets; CAIP-2 chain ids; placeholders below). Deny when the call
// target is in the OFAC-program contract set OR the chain is a mapped sanctioned chain.
@id("jurisdiction-ofac-comprehensive-pack-deny")
@severity("deny")
@reason("Target or chain is in the OFAC comprehensive-sanctions pack — interaction blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["0x1111111111111111111111111111111111111111", "0x2222222222222222222222222222222222222222"].contains(context.target)
  || ["eip155:0"].contains(context.chain)
};
