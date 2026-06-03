// blocks: withdrawing HYPE from the Hyperliquid staking (delegation) balance back to the spot wallet without confirming the amount.
// why:    staking fund movement — cWithdraw pulls HYPE out of the staking balance (the counterpart to cDeposit); a maliciously injected withdrawal unstakes funds without consent. Confirm the amount unconditionally.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-staking venue:hyperliquid
//
// Catalog — confirm a Hyperliquid staking withdrawal (cWithdraw) (hl docs).
// Pulls HYPE out of the staking balance. Pure-static.
@id("hl-c-withdraw-staking-confirm")
@severity("warn")
@reason("Withdrawing HYPE from Hyperliquid staking — confirm the amount before the request is sent")
forbid(principal, action == HyperliquidCore::Action::"HlCWithdraw", resource);
