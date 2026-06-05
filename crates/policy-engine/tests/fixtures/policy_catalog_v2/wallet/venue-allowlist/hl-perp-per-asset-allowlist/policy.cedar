// blocks: a Hyperliquid order on a market/asset outside the user's allowed-symbol set
// why:    per-asset allowlisting (Fireblocks asset-policy) confines perp trading to vetted markets; an order on an unlisted symbol (illiquid/manipulated/wrong-asset) is blocked before signing
// bucket: wallet/venue-allowlist
// tags: bucket:wallet cat:venue-allowlist static venue:hyperliquid action-family:asset-allowlist
// HyperliquidCore flow is DENY-CLOSED; this allowlist deny matches that posture.
// Match on the discrete `market.symbol` discriminator (HL numerics are strings).
// User-configured symbol allowlist (placeholders below). Pure-static.
@id("hl-perp-per-asset-allowlist")
@severity("deny")
@reason("Hyperliquid order is on a market outside your allowed-asset list — order blocked")
forbid(principal, action == HyperliquidCore::Action::"HlOrder", resource)
unless {
  ["BTC", "ETH", "SOL"].contains(context.market.symbol)
};
