//! Uniswap SwapRouter02 (`swap-router-contracts`) — V2+V3 combined router with
//! NO `deadline` parameter (deadline is enforced via outer `Multicall.multicall(deadline, data)`).
//! Address (mainnet): `0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45`

pub mod exact_input;
pub mod exact_input_single;
pub mod exact_output;
pub mod exact_output_single;
