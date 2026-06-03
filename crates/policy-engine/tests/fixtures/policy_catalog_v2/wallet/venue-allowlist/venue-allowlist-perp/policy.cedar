// blocks: opening a perpetual position on a perp venue outside the user's allowlist
// why:    venue-allowlisting (Fireblocks) confines leveraged trading to vetted perp DEXs; an unknown `venue.name` (untrusted/forked perp protocol) carries counterparty and contract risk and is blocked
// bucket: wallet/venue-allowlist
// tags: bucket:wallet cat:venue-allowlist static action-family:venue-allowlist
// Match on the discrete `venue.name` discriminator (snake_case perp venue names).
@id("venue-allowlist-perp")
@severity("deny")
@reason("This perp venue is not on your allowlist — position blocked")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
unless {
  ["hyperliquid", "gmx_v2", "dy_dx_v4"].contains(context.venue.name)
};
