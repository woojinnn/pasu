// blocks: signing a Permit2 allowance whose signature deadline sits more than a year out, leaving the signed message replayable far into the future
// why:    stale-signature replay risk — a Permit2 sign_allowance with a far-future sigDeadline is a standing, redeemable off-chain authorization a relayer can submit long after signing; a short sigDeadline bounds that window (Uniswap Permit2 docs)
// bucket: protocol/permit2
// tags: bucket:protocol cat:permit2 enrichment method:clock.now venue:permit2 action-family:far-expiration
// methods: clock.now -> ../../../_methods/clock.now.md
// dormant: until clock.now is registered in method-catalog.json
// NOTE: `sigDeadline` is Long (unix seconds) per VOCAB §4 — the year window is a Cedar literal.
//       (offchain_sig action; sigDeadline is the signature-validity bound, distinct from expiresAt.)
@id("permit2-sign-far-sig-deadline")
@severity("warn")
@reason("This Permit2 signature stays redeemable for over a year — shorten the deadline before signing")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.sigDeadline > context.custom.nowTs + 31536000
};
