// blocks: moving USDC between your Hyperliquid spot and perp balances without confirming the amount and direction.
// why:    intra-account fund movement — usdClassTransfer shifts collateral between the spot and perp wallets; a maliciously injected transfer can drain the perp margin (raising liquidation risk) or strand funds on the wrong side. Confirm amount and toPerp direction unconditionally.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer venue:hyperliquid
//
// Catalog — confirm before transferring USDC between Hyperliquid spot and perp (hl docs).
// Shifts collateral between spot and perp wallets. Pure-static.
@id("hl-usd-class-transfer-confirm")
@severity("warn")
@reason("Transferring USDC between Hyperliquid spot and perp — confirm the amount and direction")
forbid(principal, action == HyperliquidCore::Action::"HlUsdClassTransfer", resource);
