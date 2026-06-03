// blocks: an un-decoded interaction whose target or chain is in the install-time MiCA-restricted (non-authorised CASP) pack — soft warn.
// why:    jurisdiction install-pack (static) — under the EU Markets in Crypto-Assets regulation, crypto-asset services must be provided by an authorised CASP; interacting with a service the operator maps to a non-authorised / restricted-for-EU-users venue is a MiCA compliance concern for EU users. Jurisdiction/authorisation is NOT an on-chain field, so this is a baked install-time set. Warn (not deny) — MiCA scope is user/region-dependent and advisory here. (EU Regulation 2023/1114 (MiCA).)
// bucket: compliance/jurisdiction
// tags: bucket:compliance cat:jurisdiction static could-be-compliance:true action-family:jurisdiction
//
// Binds the Core::Unknown catch-all. Baked sets are install-time / operator-configured
// (lowercase 0x targets; CAIP-2 chain ids; placeholders below). Warn when the call
// target is in the MiCA-restricted contract set OR the chain is a mapped restricted chain.
@id("jurisdiction-mica-restricted-pack-warn")
@severity("warn")
@reason("Target or chain is in the MiCA-restricted (non-authorised CASP) pack — review before proceeding")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["0x5555555555555555555555555555555555555555", "0x6666666666666666666666666666666666666666"].contains(context.target)
  || ["eip155:0"].contains(context.chain)
};
