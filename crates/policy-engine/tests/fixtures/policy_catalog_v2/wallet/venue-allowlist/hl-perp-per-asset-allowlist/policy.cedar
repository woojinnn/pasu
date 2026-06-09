// blocks: a Hyperliquid order on a market/asset outside the user's allowed-symbol set
// why:    per-asset allowlisting (Fireblocks asset-policy) confines perp trading to vetted markets; an order on an unlisted symbol (illiquid/manipulated/wrong-asset) is blocked before signing
// bucket: wallet/venue-allowlist
// tags: bucket:wallet cat:venue-allowlist static venue:hyperliquid action-family:asset-allowlist
// HL orders decode to the generic `Perp::PlaceOrder`; scoped to Hyperliquid via
// the venue guard, then DENY-CLOSED unless the order's `market.symbol` is in the
// allowlist. User-configured symbol allowlist (placeholders below). Pure-static.
@id("hl-perp-per-asset-allowlist")
@severity("deny")
@reason("Hyperliquid order is on a market outside your allowed-asset list — order blocked")
forbid(principal, action == Perp::Action::"PlaceOrder", resource)
when { context.venue.name == "hyperliquid" }
unless {
  ["BTC", "ETH", "SOL"].contains(context.market.symbol)
};
