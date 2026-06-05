// blocks: signing a Permit2 allowance whose on-chain expiration is more than one year in the future.
// why:    long-lived standing pull — a far-future Permit2 `expiresAt` keeps the spender's allowance live long after the user forgets it, the off-chain analog of a never-revoked unlimited approve (Uniswap Permit2 AllowanceTransfer.expiration).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:clock.now action-family:stale-ttl venue:permit2
// methods: clock.now -> ../../../_methods/clock.now.md
//
// `clock.now` enriches wall-clock unix seconds. Warn when the Permit2 expiration
// is more than 1 year (31536000s) out. ENRICHMENT (clock.now) — dormant until
// the dispatcher serves it.
@id("permit2-far-expiration")
@severity("warn")
@reason("This Permit2 allowance stays valid for over a year — prefer a short expiration")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.expiresAt > context.custom.nowTs + 31536000
};
