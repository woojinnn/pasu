// blocks: a swap routed through an explicitly denied venue/protocol
// why:    a denylist (Fireblocks block-policy) hard-blocks venues an operator has flagged as compromised, sanctioned, or unacceptable; matching the discrete `venue.name` discriminator stops the swap before signing
// bucket: wallet/venue-denylist
// tags: bucket:wallet cat:venue-denylist static action-family:venue-denylist could-be-compliance:true
// Denied-venue set is operator-configured (snake_case venue names; placeholders below). Pure-static.
@id("venue-denylist")
@severity("deny")
@reason("This swap routes through a denied venue — blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  ["trader_joe_lb", "maverick_v2"].contains(context.venue.name)
};
