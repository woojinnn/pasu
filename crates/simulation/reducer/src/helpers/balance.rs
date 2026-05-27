//! Balance manipulation primitives: `debit`, `credit`, `transfer`.

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenKey;
use simulation_state::{StateDelta, WalletState};

use crate::error::ReducerResult;

/// Decrease the effective fungible balance of `key` by `amount` and emit a
/// matching `TokenChange::BalanceDelta` into `delta`. Errors on underflow,
/// missing holding, or non-fungible balance form.
pub fn debit(
    _state: &WalletState,
    _delta: &mut StateDelta,
    _key: &TokenKey,
    _amount: U256,
) -> ReducerResult<()> {
    todo!()
}

/// Increase the effective fungible balance of `key` by `amount` and emit a
/// matching `TokenChange::BalanceDelta` into `delta`.
pub fn credit(
    _state: &WalletState,
    _delta: &mut StateDelta,
    _key: &TokenKey,
    _amount: U256,
) -> ReducerResult<()> {
    todo!()
}

/// Outgoing `ERC20`-style transfer from this wallet to `recipient`.
///
/// Decreases the effective balance of `key` by `amount` (via `debit`) and
/// records the recipient in `delta` for audit. The recipient wallet itself
/// is not tracked here — the simulator only models one wallet's state.
pub fn transfer(
    _state: &WalletState,
    _delta: &mut StateDelta,
    _key: &TokenKey,
    _recipient: Address,
    _amount: U256,
) -> ReducerResult<()> {
    todo!()
}
