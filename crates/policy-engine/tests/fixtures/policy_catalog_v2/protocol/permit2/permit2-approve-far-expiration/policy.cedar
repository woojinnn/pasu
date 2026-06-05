// blocks: granting an on-chain Permit2 allowance whose expiration sits more than a year out, leaving a long-lived standing pull right on the token
// why:    long-lived standing-allowance risk — a Permit2 allowance with a far-future expiration stays exploitable for a year+ if the spender is later compromised; a short expiry bounds the drainer window (Uniswap Permit2 docs)
// bucket: protocol/permit2
// tags: bucket:protocol cat:permit2 enrichment method:clock.now venue:permit2 action-family:far-expiration
// methods: clock.now -> ../../../_methods/clock.now.md
// dormant: until clock.now is registered in method-catalog.json
// NOTE: `expiresAt` is Long (unix seconds) per VOCAB §4 — the year window is a Cedar literal.
@id("permit2-approve-far-expiration")
@severity("warn")
@reason("This Permit2 allowance stays valid for over a year — shorten the expiration before signing")
forbid(principal, action == Token::Action::"Permit2Approve", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.expiresAt > context.custom.nowTs + 31536000
};
