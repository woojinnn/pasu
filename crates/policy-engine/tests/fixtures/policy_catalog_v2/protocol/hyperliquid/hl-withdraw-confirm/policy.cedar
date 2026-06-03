// blocks: withdrawing USDC off Hyperliquid to an EVM address without confirming the destination and amount.
// why:    fund-movement exfiltration — a withdraw moves balance off the venue to an arbitrary EVM recipient; confirming destination and amount guards against a maliciously injected withdrawal.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static
//
// Catalog F5 — confirm before withdrawing funds off Hyperliquid.
// A fund-movement action that takes USDC off the venue to an EVM address.
// Pure-static confirm.
@id("hl-withdraw-confirm")
@severity("warn")
@reason("Withdrawing funds off Hyperliquid — confirm the destination and amount before the request is sent")
forbid(principal, action == HyperliquidCore::Action::"HlWithdraw", resource);
