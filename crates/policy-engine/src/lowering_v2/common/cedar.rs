//! Small Cedar-encoding helpers shared across every new-model lowering.

use policy_state::primitives::{Address, U256};

/// Lower-hex (`0x…`) rendering of a [`U256`] (alloy `LowerHex`).
pub(crate) fn u256_hex(v: U256) -> String {
    format!("{v:#x}")
}

/// Lowercase `0x`-hex rendering of an [`Address`] (alloy `LowerHex`), matching
/// the spec's "always lowercase" address convention.
pub(crate) fn addr(a: &Address) -> String {
    format!("{a:#x}")
}
