// dambi default v2 policy — explorer id: AIR-07 (claiming a token the wallet
// has never held and that no external token list recognises).
//
// METHOD-BACKED (state read AND external feed; sv = PART). Held-history alone is
// weak — normal airdrops are first-time tokens — so the signal is the AND of two
// inputs: the claimed token is NOT a key in `WalletState.tokens` AND it is NOT in
// an external token list. Cedar cannot do the held-set membership (the held set
// is durable state, not an action field) nor the external lookup, so a NEW
// `claim.token_unrecognized` fact folds both and projects the conjunction into
// `context.custom.claimTokenUnknown` (Bool). Defined-not-wired → `optional`, so
// its absence fail-opens until the fact host + token registry land. Warn (a soft
// signal) when the claimed token is unrecognised on both counts.
@id("air-unknown-token-warn")
@severity("warn")
@reason("This claim delivers a token you have never held and that no token list recognises")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has claimTokenUnknown
  && context.custom.claimTokenUnknown
};
