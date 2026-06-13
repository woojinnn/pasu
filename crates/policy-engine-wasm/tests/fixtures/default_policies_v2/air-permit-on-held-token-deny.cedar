// dambi default v2 policy — explorer id: AIR-03 ("gasless claim" that is
// actually a permit/approve phishing an ALREADY-HELD token out of the wallet).
//
// METHOD-BACKED state read (Aggregator type 9; sv = COV). A genuine claim
// receives a future airdrop; a drainer disguises an approval over a token the
// wallet ALREADY holds. The claimed/permit token is normalised to a `TokenKey`
// and tested for membership in `WalletState.tokens` — durable wallet state, not
// an action field. The `claim.source_held_state` reducer fact emits
// `tokenAlreadyHeld`, projected into `context.custom.permitTokenIsHeld` (Bool).
// Defined-not-wired → `optional`, so its absence fail-opens until the sim-server
// fact host serves it. Deny when the permit/claim token is already held (you are
// being drained, not airdropped).
@id("air-permit-on-held-token-deny")
@severity("deny")
@reason("This 'claim' would grant approval over a token you already hold — a draining permit, not an airdrop")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has permitTokenIsHeld
  && context.custom.permitTokenIsHeld
};
