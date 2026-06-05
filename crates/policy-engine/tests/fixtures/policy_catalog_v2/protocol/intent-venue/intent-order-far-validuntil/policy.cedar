// blocks: signing an intent order whose validUntil sits more than a week out, letting it fill at a stale price after the market moves
// why:    stale-TTL signed-order risk — a long-lived off-chain order is a standing fill right a solver can exercise later at a price that has since drifted against the signer
// bucket: protocol/intent-venue  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:protocol cat:intent-venue enrichment method:clock.now action-family:stale-ttl
// methods: clock.now -> ../../../_methods/clock.now.md
// Catalog B8 — warn on an intent order valid for more than a week.
// Long-lived signed orders can fill at a stale price after the market moves.
// ENRICHMENT (clock.now) — dormant until served. validUntil is unix seconds (Long).
@id("intent-order-far-validuntil")
@severity("warn")
@reason("This intent order stays fillable for over a week — a long TTL risks a stale-price fill")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has nowTs
  && context.validUntil > context.custom.nowTs + 604800
};
