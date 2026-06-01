//! ERC-20 approval discovery via batched `allowance(owner, spender)` calls.
//!
//! For each held token on a cataloged chain, discovery checks a curated set of
//! common spender contracts using Multicall3. Only successful, non-zero
//! allowances are returned to the caller.

use std::str::FromStr;
use std::sync::Arc;

use policy_state::primitives::{Address, ChainId, Spender, U256};

use crate::error::SyncError;
use crate::fetchers::rpc::multicall::{Call3, Multicall};
use crate::fetchers::rpc::{BlockTag, RpcRouter};

use super::known_spenders::known_spenders_for;

/// One ERC-20 allowance discovered for a wallet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredApproval {
    /// Chain where the token and spender live.
    pub chain: ChainId,
    /// ERC-20 token contract.
    pub token: Address,
    /// Spender contract approved by the wallet.
    pub spender: Spender,
    /// Current on-chain allowance amount.
    pub amount: U256,
}

/// Discover non-zero ERC-20 allowances for held `tokens`.
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

    let mut spender_addrs = Vec::with_capacity(spenders.len());
    for spender in spenders {
        let address = Address::from_str(spender.address).map_err(|err| SyncError::FetchFailed {
            source_id: "approvals".into(),
            reason: format!("bad hardcoded spender `{}`: {err}", spender.address),
        })?;
        spender_addrs.push(address);
    }

    let mut calls = Vec::with_capacity(tokens.len() * spender_addrs.len());
    let mut mapping = Vec::with_capacity(calls.capacity());

    for (token_index, token) in tokens.iter().enumerate() {
        for (spender_index, spender) in spender_addrs.iter().enumerate() {
            calls.push(Call3 {
                target: *token,
                allow_failure: true,
                call_data: encode_allowance(owner, *spender),
            });
            mapping.push((token_index, spender_index));
        }
    }

    let multicall = Multicall::new(router.clone());
    let results = multicall.aggregate3(chain, calls, BlockTag::Latest).await?;

    let mut approvals = Vec::new();
    for (index, result) in results.iter().enumerate() {
        if !result.success {
            continue;
        }
        let amount = decode_uint256(&result.return_data);
        if amount.is_zero() {
            continue;
        }
        let Some((token_index, spender_index)) = mapping.get(index).copied() else {
            continue;
        };
        approvals.push(DiscoveredApproval {
            chain: chain.clone(),
            token: tokens[token_index],
            spender: spender_addrs[spender_index],
            amount,
        });
    }

    Ok(approvals)
}

/// ERC-20 `allowance(address,address)` selector plus ABI-encoded arguments.
fn encode_allowance(owner: Address, spender: Address) -> Vec<u8> {
    const SELECTOR: [u8; 4] = [0xdd, 0x62, 0xed, 0x3e];

    let mut out = Vec::with_capacity(4 + 32 + 32);
    out.extend_from_slice(&SELECTOR);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(owner.as_slice());
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(spender.as_slice());
    out
}

/// Decode a single `uint256` return value. Short or malformed data is treated
/// as zero so it will not be persisted as an approval.
fn decode_uint256(return_data: &[u8]) -> U256 {
    if return_data.len() < 32 {
        return U256::ZERO;
    }

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&return_data[..32]);
    U256::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_selector_is_correct() {
        let data = encode_allowance(Address::ZERO, Address::ZERO);

        assert_eq!(&data[..4], &[0xdd, 0x62, 0xed, 0x3e]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn allowance_encoding_places_owner_then_spender() {
        let owner = Address::from([0x11u8; 20]);
        let spender = Address::from([0x22u8; 20]);

        let data = encode_allowance(owner, spender);

        assert_eq!(&data[4..16], &[0u8; 12]);
        assert_eq!(&data[16..36], &[0x11u8; 20]);
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
        assert_eq!(decode_uint256(&[0xff; 32]), U256::MAX);
    }
}
