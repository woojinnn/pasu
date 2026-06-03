// blocks: depositing funds into a Hyperliquid vault without an explicit confirm.
// why:    a vault deposit moves USDC out of the user's own balance into a vault contract whose strategy and withdrawal terms are opaque at sign-time; confirm the vault address before committing funds. Withdrawals (isDeposit=false) return funds to the user and are not gated here. (hl docs — vaultTransfer)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-vault
//
// Catalog — confirm a Hyperliquid vaultTransfer DEPOSIT only.
// `isDeposit` (Bool) is true on a deposit into the vault. Pure-static confirm.
@id("hl-vault-transfer-deposit-confirm")
@severity("warn")
@reason("Depositing funds into a Hyperliquid vault — confirm the vault address before committing funds")
forbid(principal, action == HyperliquidCore::Action::"HlVaultTransfer", resource)
when {
  context.isDeposit
};
