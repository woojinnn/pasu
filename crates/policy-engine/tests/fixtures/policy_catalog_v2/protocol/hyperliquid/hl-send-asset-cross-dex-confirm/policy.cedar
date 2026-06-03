// blocks: a Hyperliquid sendAsset that moves a token across DEXs (sourceDex != destinationDex) without confirming destination, token and amount.
// why:    cross-venue fund movement — a cross-DEX sendAsset hops funds between Hyperliquid sub-venues to an arbitrary destination, a higher-risk exfiltration path than a same-DEX move; same-DEX transfers are not gated here. Confirm the destination on the cross-DEX case.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer venue:hyperliquid
//
// Catalog — confirm a cross-DEX Hyperliquid sendAsset (hl docs).
// Gate only when source and destination DEX differ. Pure-static.
@id("hl-send-asset-cross-dex-confirm")
@severity("warn")
@reason("Cross-DEX asset transfer on Hyperliquid — confirm the destination, token and amount")
forbid(principal, action == HyperliquidCore::Action::"HlSendAsset", resource)
when { context.sourceDex != context.destinationDex };
