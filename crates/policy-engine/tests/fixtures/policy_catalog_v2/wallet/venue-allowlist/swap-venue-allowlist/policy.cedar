// blocks: a swap silently routed through a venue outside the user's vetted AMM allowlist
// why:    venue-allowlisting confines swaps to known-good AMMs; an unknown venue.name (fresh fork, untrusted aggregator, or look-alike router) is a malicious-/unvetted-contract risk
// bucket: wallet/venue-allowlist  (precedence-won)
// tags: bucket:wallet cat:venue-allowlist static
//
// Catalog C5 — warn on a swap routed through a non-allowlisted venue.
// Restricts swaps to a vetted set of AMMs; an unknown venue.name (e.g. a fresh
// fork or aggregator) trips the warn. Pure-static (`venue.name` discriminator).
@id("swap-venue-allowlist")
@severity("warn")
@reason("This swap routes through a venue outside your allowlist — confirm the protocol")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  !(["uniswap_v2", "uniswap_v3", "uniswap_v4", "curve_v1", "curve_v2", "balancer_v2", "balancer_v3"].contains(context.venue.name))
};
