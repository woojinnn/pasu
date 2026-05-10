# `nft.seaport` Extension

OpenSea Seaport (v1.x/v2) — NFT 거래 프로토콜.

| 진입점 | 주소 (mainnet, v1.6) |
|---|---|
| Seaport | `0x0000000000000068F116a894984e2DB1123eB395` |

핵심:
- `fulfillOrder(Order, bytes32 fulfillerConduitKey)` → `NftBuy`/`NftSell`
- `fulfillBasicOrder(BasicOrderParameters)` → `NftBuy`
- `fulfillAdvancedOrder(...)` / `matchOrders(...)`

```jsonc
{
  "namespace": "nft.seaport",
  "data": {
    "orderHash": "0x...",
    "offerer": "0x...",
    "considerationAmount": "1000000000000000000",
    "marketplaceFeeBps": 250
  }
}
```

v0.1 *세미-어댑터 미구현*.
