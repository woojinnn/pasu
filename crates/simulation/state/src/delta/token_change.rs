//! TokenChange — 한 토큰에 대한 변경 한 줄.

use serde::{Deserialize, Serialize};

use crate::approval::AllowanceSpec;
use crate::primitives::{Address, SignedI256, Spender};
use crate::token::{TokenKey, TokenKind};

/// approval 의 어떤 scope 를 회수하는지.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    Erc20,
    SetForAll,
    Permit2,
    /// ERC721 per-token (tokens[k].approved_to).
    Erc721Token,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenChange {
    /// 잔고 증감. delta 음수 = 차감.
    BalanceDelta { key: TokenKey, delta: SignedI256 },

    /// approve / set_for_all / permit2 추가.
    ApprovalSet {
        key: TokenKey,
        spender: Spender,
        allowance: AllowanceSpec,
    },

    /// 권한 회수.
    ApprovalRevoke {
        key: TokenKey,
        spender: Spender,
        scope: ApprovalScope,
    },

    /// ERC721 per-token approve(tokenId, spender).
    Erc721ApprovedTo {
        key: TokenKey,
        spender: Option<Address>,
    },

    /// 처음 보는 토큰이 결과로 생긴 경우 (kind hint 동봉).
    Mint { key: TokenKey, kind_hint: TokenKind },
}
