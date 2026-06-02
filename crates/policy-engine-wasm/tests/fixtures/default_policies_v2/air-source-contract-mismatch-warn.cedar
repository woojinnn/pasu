// scopeball default v2 policy — explorer id: AIR-02 (claiming from a distributor
// contract that is not the legitimate source I am eligible from).
//
// METHOD-BACKED (Aggregator type 9 + state read; sv = PART). Wallet state holds
// the eligible claim, but `AirdropClaim.source` is a `ProtocolRef` NAME, not an
// address — resolving the legitimate contract set needs an external registry.
// The `claim.source_held_state` reducer fact compares the action's distributor
// contract against the known-eligible source and projects `sourceMatches` into
// `context.custom.sourceContractMatches` (Bool). Defined-not-wired → `optional`,
// so its absence fail-opens (the policy no-ops rather than SystemFail) until the
// name→address registry lands. Warn when the source does NOT match (the coarse
// unknown-contract case is also absorbed by GEN-09 blind-sign).
@id("air-source-contract-mismatch-warn")
@severity("warn")
@reason("This claim targets a contract that does not match your eligible airdrop source")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has sourceContractMatches
  && context.custom.sourceContractMatches == false
};
