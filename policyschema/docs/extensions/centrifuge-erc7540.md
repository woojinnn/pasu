# `centrifuge.erc7540` Extension

Centrifuge — ERC-7540 비동기 vault (RWA 펀드).

핵심 (ERC-7540 Async Tokenized Vault):
- `requestDeposit(uint256 assets, address controller, address owner)` → `Subscribe`
- `deposit(uint256 assets, address receiver)` → `ClaimSubscription`
- `requestRedeem(uint256 shares, address controller, address owner)` → `RequestRedemption`
- `redeem(uint256 shares, address receiver, address owner)` → `ClaimRedemption`

```jsonc
{
  "namespace": "centrifuge.erc7540",
  "data": {
    "vault": "0x...",
    "requestId": "12345",
    "trancheId": "0x..."
  }
}
```

v0.1 *세미-어댑터 미구현*.
