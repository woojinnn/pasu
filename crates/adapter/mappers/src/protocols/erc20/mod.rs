//! ERC-20 / ERC-721 mappers: approve / transfer / transferFrom / setApprovalForAll.

mod common;
pub mod approve;
pub mod set_approval_for_all;
pub mod transfer;
pub mod transfer_from;

pub use approve::{approve_mapper_arc, approve_mapper_key, Erc20ApproveMapper, APPROVE_MAPPER_ID};
pub use set_approval_for_all::{
    set_approval_for_all_mapper_arc, set_approval_for_all_mapper_key, SetApprovalForAllMapper,
    SET_APPROVAL_FOR_ALL_MAPPER_ID,
};
pub use transfer::{
    transfer_mapper_arc, transfer_mapper_key, Erc20TransferMapper, TRANSFER_MAPPER_ID,
};
pub use transfer_from::{
    transfer_from_mapper_arc, transfer_from_mapper_key, Erc20TransferFromMapper,
    TRANSFER_FROM_MAPPER_ID,
};

// Backwards-compat re-export: the old monolithic file exposed a `pub const
// ERC20_TRANSFER_FROM_DECODER_ID` (re-exporting the abi_resolver constant).
// Some downstream code may still import it from `mappers::protocols::erc20::ERC20_TRANSFER_FROM_DECODER_ID`.
pub use abi_resolver::ids::ERC20_TRANSFER_FROM_DECODER_ID;

#[cfg(test)]
mod tests;
