//! Address primitives.

pub use alloy_primitives::Address;

/// Format an address as a lowercase `0x`-prefixed hex string.
#[must_use]
pub fn lowercase_hex(addr: &Address) -> String {
    format!("{addr:#x}")
}

/// Address used as an approval spender.
pub type Spender = Address;
