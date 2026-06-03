// blocks: moving USDC to or from a Hyperliquid sub-account without an explicit confirm.
// why:    fund-movement — subAccountTransfer relocates balance between the master account and a sub-account address; an injected transfer can siphon balance into an attacker-chosen sub-account, so confirm the sub-account and direction. (hl docs — subAccountTransfer)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — confirm a Hyperliquid subAccountTransfer (USDC move to/from a sub-account).
// HL usd is a decimal string (magnitude not comparable in Cedar), so this is an
// unconditional pure-static confirm on the action.
@id("hl-sub-account-transfer-confirm")
@severity("warn")
@reason("Moving USDC to or from a Hyperliquid sub-account — confirm the sub-account and direction before the request is sent")
forbid(principal, action == HyperliquidCore::Action::"HlSubAccountTransfer", resource);
