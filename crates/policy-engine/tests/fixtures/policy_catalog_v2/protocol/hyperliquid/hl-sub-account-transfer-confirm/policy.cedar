// blocks: moving USDC to or from a Hyperliquid sub-account without an explicit confirm.
// why:    fund-movement — subAccountTransfer relocates balance between the master account and a sub-account address; an injected transfer can siphon balance into an attacker-chosen sub-account, so confirm the sub-account and direction. (hl docs — subAccountTransfer)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — confirm a Hyperliquid subAccountTransfer (USDC move to/from a sub-account).
// subAccountTransfer now lowers to the generic Token::Erc20Transfer (recipient =
// sub-account address); the `hl_sub_account_transfer` manifest trigger keeps this
// policy scoped to HL sub-account moves, so it stays an unconditional pure-static
// confirm.
@id("hl-sub-account-transfer-confirm")
@severity("warn")
@reason("Moving USDC to or from a Hyperliquid sub-account — confirm the sub-account and direction before the request is sent")
forbid(principal, action == Token::Action::"Erc20Transfer", resource);
