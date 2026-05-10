# `vault.erc4626` Extension

ERC-4626 Tokenized Vault Standard — *모든* 호환 vault에 보편 매핑.

핵심:
- `deposit(uint256 assets, address receiver)` → `VaultDeposit`
- `mint(uint256 shares, address receiver)` → `VaultDeposit` (`isShareDenominated: true`)
- `withdraw(uint256 assets, address receiver, address owner)` → `VaultWithdraw`
- `redeem(uint256 shares, address receiver, address owner)` → `VaultWithdraw` (`isShareDenominated: true`)

```jsonc
{
  "namespace": "vault.erc4626",
  "data": {
    "vault": "0x...",
    "asset": "0x...",            // underlying
    "previewedShares": "..."      // off-chain preview (optional)
  }
}
```

v0.1 *세미-어댑터 미구현*.
