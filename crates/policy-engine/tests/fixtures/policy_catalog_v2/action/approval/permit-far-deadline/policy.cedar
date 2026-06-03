// blocks: signing an EIP-2612 permit whose deadline is more than one year in the future
// why:    stale/never-expiring signature risk — a far-future EIP-2612 deadline lets a phisher hold the signed permit indefinitely and redeem it long after the user forgets it
// bucket: action/approval  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:approval enrichment method:clock.now action-family:stale-ttl
// methods: clock.now -> ../../../_methods/clock.now.md
// Catalog B2 — warn on an EIP-2612 permit whose deadline is > 1 year out.
// A never-expiring signature lets an attacker sit on it indefinitely.
// ENRICHMENT (clock.now) — dormant until the dispatcher serves it.
@id("permit-far-deadline")
@severity("warn")
@reason("This permit signature stays valid for over a year — prefer a short deadline")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.deadline > context.custom.nowTs + 31536000
};
