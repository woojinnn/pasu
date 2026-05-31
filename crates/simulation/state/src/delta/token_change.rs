//! `TokenChange` — a single change line describing one mutation to one token.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::approval::AllowanceSpec;
use crate::primitives::{Address, SignedI256, Spender};
use crate::token::{TokenKey, TokenKind};

/// Which approval scope is being revoked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    /// ERC20 per-spender allowance.
    Erc20,
    /// ERC721/ERC1155 operator approval (setApprovalForAll).
    SetForAll,
    /// Permit2 allowance.
    Permit2,
    /// ERC721 per-token (`tokens[k].approved_to`).
    Erc721Token,
}

/// A single mutation to one token's state (balance or approval).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenChange {
    /// Balance increase or decrease; a negative `delta` is a debit.
    BalanceDelta {
        /// Fungibility-unit identifier of the affected token.
        key: TokenKey,
        /// Signed balance change; negative means the balance is reduced.
        #[tsify(type = "string")]
        delta: SignedI256,
    },

    /// Grants or raises an approval (ERC20 approve / setApprovalForAll / Permit2).
    ApprovalSet {
        /// Fungibility-unit identifier of the approved token.
        key: TokenKey,
        /// Address being granted spending rights.
        #[tsify(type = "string")]
        spender: Spender,
        /// Allowance amount and metadata granted to the spender.
        allowance: AllowanceSpec,
    },

    /// Revokes a previously granted approval.
    ApprovalRevoke {
        /// Fungibility-unit identifier of the token whose approval is revoked.
        key: TokenKey,
        /// Address whose spending rights are being revoked.
        #[tsify(type = "string")]
        spender: Spender,
        /// Which approval scope is being revoked.
        scope: ApprovalScope,
    },

    /// ERC721 per-token approve(tokenId, spender).
    Erc721ApprovedTo {
        /// Fungibility-unit identifier of the ERC721 token being approved.
        key: TokenKey,
        /// Newly approved address, or `None` to clear the approval.
        #[tsify(optional, type = "string")]
        spender: Option<Address>,
    },

    /// A previously unseen token appears as a result (carries a kind hint).
    Mint {
        /// Fungibility-unit identifier of the newly minted token.
        key: TokenKey,
        /// Hint describing the token's kind (ERC20, ERC721, etc.).
        kind_hint: TokenKind,
    },
}
