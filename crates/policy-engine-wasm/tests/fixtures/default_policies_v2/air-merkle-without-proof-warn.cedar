// dambi default v2 policy — explorer id: AIR-04 (distributor claims to be a
// MerkleDistributor but asks for a signature with NO inclusion proof).
//
// PURE ACTION-FIELD introspection (decode-time cross-check, no enrichment). A
// genuine `merkle_distributor` claim ALWAYS carries an inclusion `proof` (the
// set of sibling hashes). A Merkle-flavoured target with `proof` absent is a
// signal the user is being made to sign something else. Both signals are base
// fields of `Airdrop::ClaimContext`: `claimTarget.kind` (non-optional `String`)
// and the OPTIONAL `proof?: Set<String>`. Cedar reads them directly via a
// `has`-guard, so the manifest declares NO `policy_rpc` and NO `custom_context`.
// Warn (precision is modest) when kind is merkle but the proof is missing.
@id("air-merkle-without-proof-warn")
@severity("warn")
@reason("This Merkle-distributor claim is missing its inclusion proof — you may be signing something else")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context.claimTarget.kind == "merkle_distributor"
  && !(context has proof)
};
