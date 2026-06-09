// blocks: moving funds to/from a Hyperliquid vault without an explicit confirm.
// why:    a vault transfer moves USDC between the user's balance and a vault contract whose strategy and withdrawal terms are opaque at sign-time; confirm the vault address before committing funds. (hl docs — vaultTransfer)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-vault
//
// Catalog — confirm a Hyperliquid vaultTransfer.
// vaultTransfer now lowers to the generic Token::Erc20Transfer (recipient = vault
// address). The `isDeposit` direction is NOT representable in Erc20Transfer and is
// dropped, so this confirm now fires on BOTH deposits and withdrawals (the
// `hl_vault_transfer` manifest trigger keeps it scoped to HL vault transfers).
@id("hl-vault-transfer-deposit-confirm")
@severity("warn")
@reason("Moving funds to or from a Hyperliquid vault — confirm the vault address before the request is sent")
forbid(principal, action == Token::Action::"Erc20Transfer", resource);
