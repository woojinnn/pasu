//! `TokenAction` — cross-cutting token operations (`ERC20`/`ERC721`/`ERC1155`
//! approve/permit/transfer, etc.). See spec §4.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// `ERC20` `approve` action.
pub mod erc20_approve;
/// `ERC20` `EIP-2612` `permit` action.
pub mod erc20_permit;
/// `ERC20` `transfer` action.
pub mod erc20_transfer;
/// `ERC721`/`ERC1155` single-token `approve` action.
pub mod nft_approve;
/// `ERC721`/`ERC1155` `setApprovalForAll` action.
pub mod nft_set_for_all;
/// `ERC721`/`ERC1155` transfer action.
pub mod nft_transfer;
/// `Uniswap` `Permit2` on-chain `approve` action.
pub mod permit2_approve;
/// `Uniswap` `Permit2` signed allowance action.
pub mod permit2_sign;
/// Revoke-approval action and its scope enum.
pub mod revoke;

pub use self::erc20_approve::*;
pub use self::erc20_permit::*;
pub use self::erc20_transfer::*;
pub use self::nft_approve::*;
pub use self::nft_set_for_all::*;
pub use self::nft_transfer::*;
pub use self::permit2_approve::*;
pub use self::permit2_sign::*;
pub use self::revoke::*;

/// Domain-agnostic, token-level actions that can occur anywhere.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TokenAction {
    /// `ERC20` `approve(spender, amount)`.
    Erc20Approve(Erc20ApproveAction),
    /// `ERC20` `EIP-2612` `permit` — gasless allowance via signature.
    Erc20Permit(Erc20PermitAction),
    /// `Uniswap` `Permit2` on-chain `approve` call.
    Permit2Approve(Permit2ApproveAction),
    /// `Uniswap` `Permit2` signed allowance (off-chain signature).
    Permit2SignAllowance(Permit2SignAction),
    /// `ERC20` `transfer(recipient, amount)`.
    Erc20Transfer(Erc20TransferAction),
    /// `ERC721`/`ERC1155` single-token `approve`.
    NftApprove(NftApproveAction),
    /// `ERC721`/`ERC1155` `setApprovalForAll` toggle.
    NftSetApprovalForAll(NftSetForAllAction),
    /// `ERC721`/`ERC1155` transfer.
    NftTransfer(NftTransferAction),
    /// Revoke a previously granted approval (any scope).
    RevokeApproval(RevokeApprovalAction),
}

impl TokenAction {
    /// The action's `serde` `action` tag (e.g. `"erc20_approve"`, `"nft_set_approval_for_all"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Erc20Approve(_) => "erc20_approve",
            Self::Erc20Permit(_) => "erc20_permit",
            Self::Permit2Approve(_) => "permit2_approve",
            Self::Permit2SignAllowance(_) => "permit2_sign_allowance",
            Self::Erc20Transfer(_) => "erc20_transfer",
            Self::NftApprove(_) => "nft_approve",
            Self::NftSetApprovalForAll(_) => "nft_set_approval_for_all",
            Self::NftTransfer(_) => "nft_transfer",
            Self::RevokeApproval(_) => "revoke_approval",
        }
    }

    /// Token actions never carry a venue; always `None`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        None
    }
}
