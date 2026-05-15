//! Mappers for canonical WETH9 (deposit/withdraw → wrap/unwrap).

mod common;
pub mod deposit;
pub mod withdraw;

pub use deposit::{deposit_mapper_arc, deposit_mapper_key, WethDepositMapper, WETH_DEPOSIT_MAPPER_ID};
pub use withdraw::{
    withdraw_mapper_arc, withdraw_mapper_key, WethWithdrawMapper, WETH_WITHDRAW_MAPPER_ID,
};

#[cfg(test)]
mod tests;
