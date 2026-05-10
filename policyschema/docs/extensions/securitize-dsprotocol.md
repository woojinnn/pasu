# `securitize.dsProtocol` Extension

Securitize DS Protocol — 증권형 토큰 (regulated securities).

```jsonc
{
  "namespace": "securitize.dsProtocol",
  "data": {
    "tokenAddress": "0x...",
    "investorId": "...",        // KYC ID
    "complianceState": "approved" | "pending" | "rejected"
  }
}
```

매핑: ActionType `Subscribe`/`ClaimSubscription`/`TransferRestricted`. 폐쇄소스 — confidence `medium` ceiling.

v0.1 *세미-어댑터 미구현*.
