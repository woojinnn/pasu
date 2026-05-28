//! TokenChange — 한 토큰에 대한 변경 한 줄.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::approval::AllowanceSpec;
use crate::primitives::{Address, SignedI256, Spender};
use crate::token::{TokenKey, TokenKind};

/// approval 의 어떤 scope 를 회수하는지.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    /// ERC20 `approve(spender, amount)`.
    Erc20,
    /// ERC721 / ERC1155 `setApprovalForAll(operator, bool)`.
    SetForAll,
    /// Permit2 `approve` / `permit` 로 부여된 권한.
    Permit2,
    /// ERC721 per-token (`tokens[k].approved_to`).
    Erc721Token,
}

/// 한 토큰에 대한 변경 한 줄 (잔고 증감 / approval 부여 / 회수 / mint).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenChange {
    /// 잔고 증감. delta 음수 = 차감.
    BalanceDelta {
        /// 대상 토큰.
        key: TokenKey,
        /// 잔고 증감 (음수 = 차감) base unit.
        #[tsify(type = "string")]
        delta: SignedI256,
    },

    /// approve / `set_for_all` / permit2 추가.
    ApprovalSet {
        /// 권한이 부여된 토큰.
        key: TokenKey,
        /// 권한을 받는 spender 주소.
        #[tsify(type = "string")]
        spender: Spender,
        /// 새로 설정된 한도 / 만료.
        allowance: AllowanceSpec,
    },

    /// 권한 회수.
    ApprovalRevoke {
        /// 권한이 회수된 토큰.
        key: TokenKey,
        /// 권한을 잃은 spender 주소.
        #[tsify(type = "string")]
        spender: Spender,
        /// 회수된 권한의 scope.
        scope: ApprovalScope,
    },

    /// ERC721 per-token approve(tokenId, spender).
    Erc721ApprovedTo {
        /// 대상 NFT (token id 포함).
        key: TokenKey,
        /// 새 spender (None = 회수).
        #[tsify(optional, type = "string")]
        spender: Option<Address>,
    },

    /// 처음 보는 토큰이 결과로 생긴 경우 (kind hint 동봉).
    Mint {
        /// 새로 발견된 토큰.
        key: TokenKey,
        /// 분류 hint — wallet 에 holding 을 처음 만들 때 사용.
        kind_hint: TokenKind,
    },
}
