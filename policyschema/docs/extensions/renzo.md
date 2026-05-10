# `renzo` Extension

Renzo — ezETH LRT (EigenLayer 재스테이킹).

| 진입점 | 주소 |
|---|---|
| RestakeManager | `0x74a09653A083691711cF8215a6ab074BB4e99ef5` |
| ezETH | `0xbf5495Efe5DB9ce00f80364C8B423567e58d2110` |

핵심: `depositETH() payable` (LRTDepositManager 또는 RestakeManager). 폐쇄소스 부분 다수 — confidence `medium` ceiling.

매핑: ActionType `MintLrt`. v0.1 *세미-어댑터 미구현*.
