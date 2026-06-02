// scopeball default v2 policy — explorer id: AIR-09 (claimAndStake / claimAndLock
// — tokens are locked into staking/vesting the moment they are claimed).
//
// PURE ACTION-FIELD introspection (no enrichment). The user intends only to
// "receive", but the claim binds the tokens into a lock for days–months. In the
// lowered `Airdrop::ClaimContext` the immediate-lock flavour is carried by the
// distributor discriminator: `claimTarget.kind == "staking_claim"` is the
// staking-rewards / stake-on-claim mechanism (Lido/Pendle/Convex-style). That
// is a non-optional base `String` field, so Cedar reads it directly and the
// manifest declares NO `policy_rpc` and NO `custom_context`. The user's tokens
// are not stolen, so this is a warn, not a deny.
@id("air-claim-locks-received-warn")
@severity("warn")
@reason("This claim locks the received tokens into staking/vesting — they will not be immediately usable")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context.claimTarget.kind == "staking_claim"
};
