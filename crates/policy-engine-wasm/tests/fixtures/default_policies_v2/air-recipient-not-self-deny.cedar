// dambi default v2 policy — explorer id: AIR-01 (airdrop claim whose
// recipient is an address other than mine — eligibility is mine but the tokens
// go to the attacker).
//
// PURE ACTION-FIELD (Aggregator type 5 — Set-Membership / identity, no
// enrichment). A normal claim pays the eligible holder (self); a malicious
// claim page pre-fills `recipient` with an attacker address. `recipient` is a
// non-optional base `String` field of `Airdrop::ClaimContext`, and "self" is
// the evaluated wallet's address — `principal.address` (the `Wallet` entity's
// `address`). Cedar compares the two directly, so this manifest declares NO
// `policy_rpc` and NO `custom_context`. Deny when the recipient is not self.
// Both sides are lowercase-hex normalised by the lowering.
@id("air-recipient-not-self-deny")
@severity("deny")
@reason("This airdrop claim pays out to an address that is not your wallet")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context.recipient != principal.address
};
