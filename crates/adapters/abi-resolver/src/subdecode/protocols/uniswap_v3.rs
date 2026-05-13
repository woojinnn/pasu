//! Uniswap V3 packed-path decoder.
//!
//! The `bytes path` argument of `SwapRouter.exactInput` /
//! `SwapRouter.exactOutput` (and the Universal Router's `V3_SWAP_*` opcodes) is
//! a packed sequence:
//!
//! ```text
//! [tokenA (20)][fee0 (3)][tokenB (20)][fee1 (3)] … [tokenN (20)]
//! ```
//!
//! so `path.len() == 20 + 23 * hops` for `hops >= 1`. This module returns the
//! token address sequence and the inter-hop fee tiers.

use alloy_primitives::Address;

/// Render a Uniswap V3 packed path as a human-readable token-fee chain, e.g.
/// `0xC02a…cc2 --[fee=500]--> 0xa0b8…b48 --[fee=3000]--> 0xdac1…ec7`.
///
/// Used by the orchestrator to enrich the `path` argument on V3-style swap
/// steps (UR opcodes V3_SWAP_EXACT_IN / V3_SWAP_EXACT_OUT, and the V3
/// SwapRouter's `exactInput` / `exactOutput`). Returns `None` when the bytes
/// don't parse as a V3 packed path so callers can fall back to raw hex.
#[must_use]
pub fn format_packed_path(path: &[u8]) -> Option<String> {
    let (tokens, fees) = decode_v3_path(path).ok()?;
    let mut s = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            s.push_str(&format!(" --[fee={}]--> ", fees[i - 1]));
        }
        s.push_str(&format!("0x{}", hex::encode(token.0)));
    }
    Some(s)
}

/// Error from decoding a Uniswap V3 packed path.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathDecodeError {
    /// Path length is not `20 + 23 * N` for some `N >= 1`.
    #[error("invalid Uniswap V3 path length: {got} (must be 20 + 23*N, N >= 1)")]
    InvalidLength {
        /// Observed byte length.
        got: usize,
    },
}

/// Decode a Uniswap V3 packed path.
///
/// Returns `(tokens, fees)` with `tokens.len() == hops + 1` and
/// `fees.len() == hops`.
///
/// # Errors
///
/// Returns [`PathDecodeError::InvalidLength`] when `path.len()` is not
/// `20 + 23 * N` for some `N >= 1` (i.e. there is at least one hop).
pub fn decode_v3_path(path: &[u8]) -> Result<(Vec<Address>, Vec<u32>), PathDecodeError> {
    if path.len() < 20 + 23 || !(path.len() - 20).is_multiple_of(23) {
        return Err(PathDecodeError::InvalidLength { got: path.len() });
    }
    let hops = (path.len() - 20) / 23;
    let mut tokens = Vec::with_capacity(hops + 1);
    let mut fees = Vec::with_capacity(hops);

    let mut cursor = 0;
    for _ in 0..hops {
        tokens.push(Address::from_slice(&path[cursor..cursor + 20]));
        cursor += 20;
        let fee = (u32::from(path[cursor]) << 16)
            | (u32::from(path[cursor + 1]) << 8)
            | u32::from(path[cursor + 2]);
        fees.push(fee);
        cursor += 3;
    }
    tokens.push(Address::from_slice(&path[cursor..cursor + 20]));
    Ok((tokens, fees))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_hop_decodes_two_tokens_and_one_fee() {
        let mut path = Vec::new();
        path.extend_from_slice(&[0x11; 20]);
        path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
        path.extend_from_slice(&[0x22; 20]);

        let (tokens, fees) = decode_v3_path(&path).unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Address::from([0x11; 20]));
        assert_eq!(tokens[1], Address::from([0x22; 20]));
        assert_eq!(fees, vec![3000]);
    }

    #[test]
    fn multi_hop_two_fees() {
        let mut path = Vec::new();
        path.extend_from_slice(&[0x11; 20]);
        path.extend_from_slice(&[0x00, 0x01, 0xf4]); // fee 500
        path.extend_from_slice(&[0x22; 20]);
        path.extend_from_slice(&[0x00, 0x0b, 0xb8]); // fee 3000
        path.extend_from_slice(&[0x33; 20]);

        let (tokens, fees) = decode_v3_path(&path).unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(fees, vec![500, 3000]);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            decode_v3_path(&[]),
            Err(PathDecodeError::InvalidLength { got: 0 })
        );
    }

    #[test]
    fn rejects_too_short_for_one_hop() {
        // 42 bytes = 20 + 22 — neither single token nor a full hop.
        let bad = vec![0u8; 42];
        assert_eq!(
            decode_v3_path(&bad),
            Err(PathDecodeError::InvalidLength { got: 42 })
        );
    }

    #[test]
    fn rejects_length_not_aligned_to_hops() {
        // 65 bytes = 20 + 45 = not a multiple of 23 from token0.
        let bad = vec![0u8; 65];
        assert_eq!(
            decode_v3_path(&bad),
            Err(PathDecodeError::InvalidLength { got: 65 })
        );
    }
}
