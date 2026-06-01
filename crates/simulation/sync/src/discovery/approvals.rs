//! ERC-20 approval discovery via batched `allowance(owner, spender)`
//! Multicall round-trips, scoped to a hardcoded catalog of "famous"
//! spenders (Permit2, DEX routers, lending pools, marketplaces) per
//! chain — see [`super::known_spenders`].
//!
//! Cost: one Multicall3 `aggregate3` per chain. With ~20 known tokens
//! held × ~20 known spenders = ~400 calls in one round-trip, this fits
//! comfortably inside publicnode / drpc free-tier `eth_call` limits.
//!
//! Coverage: catches the canonical Uniswap / 1inch / Aave / Permit2 /
//! OpenSea approvals (~95% of real wallets). Long-tail spenders get
//! missed; a future `eth_getLogs` pass on the `Approval` event would
//! catch those.

use std::str::FromStr;
use std::sync::Arc;

use simulation_state::primitives::{Address, ChainId, Spender, U256};

use crate::error::SyncError;
use crate::fetchers::rpc::multicall::{Call3, Multicall};
use crate::fetchers::rpc::{BlockTag, RpcRouter};

use super::known_spenders::known_spenders_for;

/// One approval row discovered for a wallet. The caller turns this into
/// an [`AllowanceSpec`](simulation_state::approval::AllowanceSpec) and
/// merges it into `WalletState.approvals.erc20`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredApproval {
    pub chain: ChainId,
    pub token: Address,
    pub spender: Spender,
    /// Current `allowance(owner, spender)` value on-chain. Zero means
    /// the user has revoked or never approved this spender — caller
    /// must filter zero rows out before persisting.
    pub amount: U256,
}

/// Discover ERC-20 approvals the `owner` has granted on `chain`.
///
/// For each (token in `tokens`, spender in known catalog) pair, calls
/// `allowance(owner, spender)` via Multicall. Returns one
/// [`DiscoveredApproval`] per non-zero allowance.
///
/// Returns an empty `Vec` for chains without a spender catalog or when
/// `tokens` is empty (no point asking about allowance on nothing).
pub async fn discover_approvals(
    router: &Arc<RpcRouter>,
    chain: &ChainId,
    owner: Address,
    tokens: &[Address],
) -> Result<Vec<DiscoveredApproval>, SyncError> {
    let spenders = known_spenders_for(chain);
    if spenders.is_empty() || tokens.is_empty() {
        return Ok(Vec::new());
    }

    // Parse spender addresses once.
    let mut spender_addrs: Vec<Address> = Vec::with_capacity(spenders.len());
    for s in spenders {
        let a = Address::from_str(s.address).map_err(|e| SyncError::FetchFailed {
            source_id: "approvals".into(),
            reason: format!("bad hardcoded spender `{}`: {e}", s.address),
        })?;
        spender_addrs.push(a);
    }

    // Build the batch: one Call3 per (token, spender) pair.
    let mut calls: Vec<Call3> = Vec::with_capacity(tokens.len() * spender_addrs.len());
    // Map call index → (token_idx, spender_idx) so we can decode results.
    let mut mapping: Vec<(usize, usize)> = Vec::with_capacity(calls.capacity());

    for (ti, token) in tokens.iter().enumerate() {
        for (si, spender) in spender_addrs.iter().enumerate() {
            calls.push(Call3 {
                target: *token,
                allow_failure: true, // missing-token / non-ERC20 must not nuke the batch
                call_data: encode_allowance(owner, *spender),
            });
            mapping.push((ti, si));
        }
    }

    let mc = Multicall::new(router.clone());
    let results = mc.aggregate3(chain, calls, BlockTag::Latest).await?;

    let mut out = Vec::new();
    for (idx, res) in results.iter().enumerate() {
        if !res.success {
            continue;
        }
        let amount = decode_uint256(&res.return_data);
        if amount.is_zero() {
            continue;
        }
        let (ti, si) = mapping[idx];
        out.push(DiscoveredApproval {
            chain: chain.clone(),
            token: tokens[ti],
            spender: spender_addrs[si],
            amount,
        });
    }
    Ok(out)
}

/// ERC-20 `allowance(address owner, address spender)` selector + 64
/// bytes of left-padded address args. `keccak("allowance(address,address)")[..4]
/// = 0xdd62ed3e`.
fn encode_allowance(owner: Address, spender: Address) -> Vec<u8> {
    const SELECTOR: [u8; 4] = [0xdd, 0x62, 0xed, 0x3e];
    let mut buf = Vec::with_capacity(4 + 64);
    buf.extend_from_slice(&SELECTOR);
    // Owner — left-padded to 32 bytes.
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(owner.as_slice());
    // Spender — left-padded to 32 bytes.
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(spender.as_slice());
    buf
}

/// Decode a single uint256 return — 32 bytes big-endian. Returns 0 on
/// short / malformed return data (failed calls are filtered upstream).
fn decode_uint256(return_data: &[u8]) -> U256 {
    if return_data.len() < 32 {
        return U256::ZERO;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&return_data[..32]);
    U256::from_be_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_selector_is_correct() {
        let data = encode_allowance(Address::ZERO, Address::ZERO);
        // keccak("allowance(address,address)")[..4] = 0xdd62ed3e
        assert_eq!(&data[..4], &[0xdd, 0x62, 0xed, 0x3e]);
        assert_eq!(data.len(), 4 + 32 + 32);
    }

    #[test]
    fn allowance_encoding_places_owner_then_spender() {
        let owner = Address::from([0x11u8; 20]);
        let spender = Address::from([0x22u8; 20]);
        let data = encode_allowance(owner, spender);
        // 12 padding zero bytes then owner bytes
        assert_eq!(&data[4..16], &[0u8; 12]);
        assert_eq!(&data[16..36], &[0x11u8; 20]);
        // Then 12 padding zero bytes then spender bytes
        assert_eq!(&data[36..48], &[0u8; 12]);
        assert_eq!(&data[48..68], &[0x22u8; 20]);
    }

    #[test]
    fn decode_uint256_short_returns_zero() {
        assert_eq!(decode_uint256(&[]), U256::ZERO);
        assert_eq!(decode_uint256(&[1, 2, 3]), U256::ZERO);
    }

    #[test]
    fn decode_uint256_max_value() {
        assert_eq!(decode_uint256(&[0xffu8; 32]), U256::MAX);
    }
}
