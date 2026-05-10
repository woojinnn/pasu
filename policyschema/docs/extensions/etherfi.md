# `etherfi` Extension

ether.fi — eETH (LRT) liquid restaking + weETH wrapper.

| 진입점 | 주소 |
|---|---|
| LiquidityPool | `0x308861A430be4cce5502d0A12724771Fc6DaF216` |
| eETH | `0x35fA164735182de50811E8e2E824cFb9B6118ac2` |
| weETH | `0xCd5fE23C85820F7B72D0926FC9b05b43E359b7ee` |

핵심: `deposit(address _referral)` payable (mint eETH), `requestWithdraw(address _recipient, uint256 _amount)`, weETH `wrap(uint256 _eETHAmount)`/`unwrap(...)`.

매핑: ActionType `MintLrt`/`RequestLrtRedemption`. v0.1 *세미-어댑터 미구현*.
