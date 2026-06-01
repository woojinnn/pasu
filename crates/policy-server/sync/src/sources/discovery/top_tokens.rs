//! Hardcoded top-N ERC-20 catalog per chain + batched `balanceOf`
//! lookup via Multicall3.
//!
//! Used as the discovery fallback when `ETHERSCAN_API_KEY` isn't set.
//! Misses long-tail tokens but covers the canonical large-cap stables
//! and majors a typical wallet holds. One Multicall round-trip per
//! chain — cheap on RPC quota.

use std::str::FromStr;
use std::sync::Arc;

use simulation_state::primitives::{Address, ChainId, U256};
use simulation_state::token::TokenKey;

use crate::error::SyncError;
use crate::fetchers::rpc::multicall::{Call3, Multicall};
use crate::fetchers::rpc::{BlockTag, RpcRouter};

use super::DiscoveredToken;

/// One row in the per-chain canonical catalog.
struct TopToken {
    address: &'static str,
    symbol: &'static str,
    decimals: u8,
}

/// Top tokens per chain. Curated by market cap / common wallet hits.
/// Add to this list as new majors become relevant.
fn top_tokens_for(chain: &ChainId) -> &'static [TopToken] {
    match chain.as_str() {
        "eip155:1" => ETH_MAINNET,
        "eip155:42161" => ARBITRUM,
        "eip155:8453" => BASE,
        "eip155:10" => OPTIMISM,
        "eip155:137" => POLYGON,
        _ => &[],
    }
}

const ETH_MAINNET: &[TopToken] = &[
    TopToken {
        address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        symbol: "USDC",
        decimals: 6,
    },
    TopToken {
        address: "0xdac17f958d2ee523a2206206994597c13d831ec7",
        symbol: "USDT",
        decimals: 6,
    },
    TopToken {
        address: "0x6b175474e89094c44da98b954eedeac495271d0f",
        symbol: "DAI",
        decimals: 18,
    },
    TopToken {
        address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        symbol: "WETH",
        decimals: 18,
    },
    TopToken {
        address: "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
        symbol: "WBTC",
        decimals: 8,
    },
    TopToken {
        address: "0x514910771af9ca656af840dff83e8264ecf986ca",
        symbol: "LINK",
        decimals: 18,
    },
    TopToken {
        address: "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984",
        symbol: "UNI",
        decimals: 18,
    },
    TopToken {
        address: "0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9",
        symbol: "AAVE",
        decimals: 18,
    },
    TopToken {
        address: "0xc944e90c64b2c07662a292be6244bdf05cda44a7",
        symbol: "GRT",
        decimals: 18,
    },
    TopToken {
        address: "0x4d224452801aced8b2f0aebe155379bb5d594381",
        symbol: "APE",
        decimals: 18,
    },
    TopToken {
        address: "0xae7ab96520de3a18e5e111b5eaab095312d7fe84",
        symbol: "stETH",
        decimals: 18,
    },
    TopToken {
        address: "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0",
        symbol: "wstETH",
        decimals: 18,
    },
    TopToken {
        address: "0x6982508145454ce325ddbe47a25d4ec3d2311933",
        symbol: "PEPE",
        decimals: 18,
    },
    TopToken {
        address: "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce",
        symbol: "SHIB",
        decimals: 18,
    },
    TopToken {
        address: "0x4fabb145d64652a948d72533023f6e7a623c7c53",
        symbol: "BUSD",
        decimals: 18,
    },
    TopToken {
        address: "0x0000000000085d4780b73119b644ae5ecd22b376",
        symbol: "TUSD",
        decimals: 18,
    },
    TopToken {
        address: "0x853d955acef822db058eb8505911ed77f175b99e",
        symbol: "FRAX",
        decimals: 18,
    },
    TopToken {
        address: "0xd533a949740bb3306d119cc777fa900ba034cd52",
        symbol: "CRV",
        decimals: 18,
    },
    TopToken {
        address: "0xba100000625a3754423978a60c9317c58a424e3d",
        symbol: "BAL",
        decimals: 18,
    },
    TopToken {
        address: "0x5a98fcbea516cf06857215779fd812ca3bef1b32",
        symbol: "LDO",
        decimals: 18,
    },
    TopToken {
        address: "0xc011a73ee8576fb46f5e1c5751ca3b9fe0af2a6f",
        symbol: "SNX",
        decimals: 18,
    },
    TopToken {
        address: "0x4e15361fd6b4bb609fa63c81a2be19d873717870",
        symbol: "FTM",
        decimals: 18,
    },
    TopToken {
        address: "0x9f8f72aa9304c8b593d555f12ef6589cc3a579a2",
        symbol: "MKR",
        decimals: 18,
    },
    TopToken {
        address: "0xc18360217d8f7ab5e7c516566761ea12ce7f9d72",
        symbol: "ENS",
        decimals: 18,
    },
    TopToken {
        address: "0x111111111117dc0aa78b770fa6a738034120c302",
        symbol: "1INCH",
        decimals: 18,
    },
    TopToken {
        address: "0xb50721bcf8d664c30412cfbc6cf7a15145234ad1",
        symbol: "ARB",
        decimals: 18,
    },
    TopToken {
        address: "0xc944e90c64b2c07662a292be6244bdf05cda44a7",
        symbol: "GRT",
        decimals: 18,
    },
    TopToken {
        address: "0x6810e776880c02933d47db1b9fc05908e5386b96",
        symbol: "GNO",
        decimals: 18,
    },
    TopToken {
        address: "0xd31a59c85ae9d8edefec411d448f90841571b89c",
        symbol: "SOL",
        decimals: 9,
    },
    TopToken {
        address: "0x57ab1ec28d129707052df4df418d58a2d46d5f51",
        symbol: "sUSD",
        decimals: 18,
    },
];

const ARBITRUM: &[TopToken] = &[
    TopToken {
        address: "0xaf88d065e77c8cc2239327c5edb3a432268e5831",
        symbol: "USDC",
        decimals: 6,
    },
    TopToken {
        address: "0xff970a61a04b1ca14834a43f5de4533ebddb5cc8",
        symbol: "USDC.e",
        decimals: 6,
    },
    TopToken {
        address: "0xfd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb9",
        symbol: "USDT",
        decimals: 6,
    },
    TopToken {
        address: "0xda10009cbd5d07dd0cecc66161fc93d7c9000da1",
        symbol: "DAI",
        decimals: 18,
    },
    TopToken {
        address: "0x82af49447d8a07e3bd95bd0d56f35241523fbab1",
        symbol: "WETH",
        decimals: 18,
    },
    TopToken {
        address: "0x2f2a2543b76a4166549f7aab2e75bef0aefc5b0f",
        symbol: "WBTC",
        decimals: 8,
    },
    TopToken {
        address: "0x912ce59144191c1204e64559fe8253a0e49e6548",
        symbol: "ARB",
        decimals: 18,
    },
    TopToken {
        address: "0xf97f4df75117a78c1a5a0dbb814af92458539fb4",
        symbol: "LINK",
        decimals: 18,
    },
    TopToken {
        address: "0xfa7f8980b0f1e64a2062791cc3b0871572f1f7f0",
        symbol: "UNI",
        decimals: 18,
    },
    TopToken {
        address: "0xba5ddd1f9d7f570dc94a51479a000e3bce967196",
        symbol: "AAVE",
        decimals: 18,
    },
    TopToken {
        address: "0x11cdb42b0eb46d95f990bedd4695a6e3fa034978",
        symbol: "CRV",
        decimals: 18,
    },
    TopToken {
        address: "0x5979d7b546e38e414f7e9822514be443a4800529",
        symbol: "wstETH",
        decimals: 18,
    },
];

const BASE: &[TopToken] = &[
    TopToken {
        address: "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
        symbol: "USDC",
        decimals: 6,
    },
    TopToken {
        address: "0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca",
        symbol: "USDbC",
        decimals: 6,
    },
    TopToken {
        address: "0x4200000000000000000000000000000000000006",
        symbol: "WETH",
        decimals: 18,
    },
    TopToken {
        address: "0xc1cba3fcea344f92d9239c08c0568f6f2f0ee452",
        symbol: "wstETH",
        decimals: 18,
    },
    TopToken {
        address: "0x50c5725949a6f0c72e6c4a641f24049a917db0cb",
        symbol: "DAI",
        decimals: 18,
    },
];

const OPTIMISM: &[TopToken] = &[
    TopToken {
        address: "0x0b2c639c533813f4aa9d7837caf62653d097ff85",
        symbol: "USDC",
        decimals: 6,
    },
    TopToken {
        address: "0x7f5c764cbc14f9669b88837ca1490cca17c31607",
        symbol: "USDC.e",
        decimals: 6,
    },
    TopToken {
        address: "0x94b008aa00579c1307b0ef2c499ad98a8ce58e58",
        symbol: "USDT",
        decimals: 6,
    },
    TopToken {
        address: "0xda10009cbd5d07dd0cecc66161fc93d7c9000da1",
        symbol: "DAI",
        decimals: 18,
    },
    TopToken {
        address: "0x4200000000000000000000000000000000000006",
        symbol: "WETH",
        decimals: 18,
    },
    TopToken {
        address: "0x68f180fcce6836688e9084f035309e29bf0a2095",
        symbol: "WBTC",
        decimals: 8,
    },
    TopToken {
        address: "0x4200000000000000000000000000000000000042",
        symbol: "OP",
        decimals: 18,
    },
];

const POLYGON: &[TopToken] = &[
    TopToken {
        address: "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359",
        symbol: "USDC",
        decimals: 6,
    },
    TopToken {
        address: "0x2791bca1f2de4661ed88a30c99a7a9449aa84174",
        symbol: "USDC.e",
        decimals: 6,
    },
    TopToken {
        address: "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
        symbol: "USDT",
        decimals: 6,
    },
    TopToken {
        address: "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
        symbol: "DAI",
        decimals: 18,
    },
    TopToken {
        address: "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
        symbol: "WETH",
        decimals: 18,
    },
    TopToken {
        address: "0x1bfd67037b42cf73acf2047067bd4f2c47d9bfd6",
        symbol: "WBTC",
        decimals: 8,
    },
    TopToken {
        address: "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270",
        symbol: "WMATIC",
        decimals: 18,
    },
];

/// ERC-20 `balanceOf(address)` selector + 32-byte left-padded address arg.
fn encode_balance_of(holder: Address) -> Vec<u8> {
    const SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31]; // keccak("balanceOf(address)")[..4]
    let mut buf = Vec::with_capacity(36);
    buf.extend_from_slice(&SELECTOR);
    // Left-pad address to 32 bytes.
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(holder.as_slice());
    buf
}

/// Decode a `balanceOf` return — a single uint256 big-endian, exactly
/// 32 bytes. Returns 0 on short / malformed data (we already filter
/// failed calls upstream).
fn decode_balance(return_data: &[u8]) -> U256 {
    if return_data.len() < 32 {
        return U256::ZERO;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&return_data[..32]);
    U256::from_be_bytes(buf)
}

/// Top-N discovery for `chain`: one Multicall round-trip with N
/// `balanceOf(holder)` calls, filtered to non-zero results.
///
/// Returns an empty vec for unsupported chains (no catalog entry).
pub async fn discover_top_tokens(
    router: &Arc<RpcRouter>,
    chain: &ChainId,
    holder: Address,
) -> Result<Vec<DiscoveredToken>, SyncError> {
    let catalog = top_tokens_for(chain);
    if catalog.is_empty() {
        return Ok(Vec::new());
    }

    // Build the Multicall batch.
    let mut calls = Vec::with_capacity(catalog.len());
    let mut parsed_addrs = Vec::with_capacity(catalog.len());
    for spec in catalog {
        let target = Address::from_str(spec.address).map_err(|e| SyncError::FetchFailed {
            source_id: "top_tokens".into(),
            reason: format!("bad hardcoded address `{}`: {e}", spec.address),
        })?;
        parsed_addrs.push(target);
        calls.push(Call3 {
            target,
            allow_failure: true, // a non-existent token on this chain shouldn't blow up the whole batch
            call_data: encode_balance_of(holder),
        });
    }

    let mc = Multicall::new(router.clone());
    let results = mc.aggregate3(chain, calls, BlockTag::Latest).await?;

    let mut out = Vec::with_capacity(catalog.len());
    for (i, res) in results.iter().enumerate() {
        if !res.success {
            continue;
        }
        let bal = decode_balance(&res.return_data);
        if bal.is_zero() {
            continue;
        }
        let spec = &catalog[i];
        out.push(DiscoveredToken {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: parsed_addrs[i],
            },
            symbol: spec.symbol.to_string(),
            decimals: spec.decimals,
            balance: bal,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_of_selector_is_correct() {
        let addr = Address::ZERO;
        let data = encode_balance_of(addr);
        assert_eq!(&data[..4], &[0x70, 0xa0, 0x82, 0x31]);
        assert_eq!(data.len(), 4 + 32);
    }

    #[test]
    fn balance_decode_zero_when_short() {
        assert_eq!(decode_balance(&[]), U256::ZERO);
        assert_eq!(decode_balance(&[1, 2, 3]), U256::ZERO);
    }

    #[test]
    fn balance_decode_max() {
        let max_be = [0xffu8; 32];
        assert_eq!(decode_balance(&max_be), U256::MAX);
    }

    #[test]
    fn known_chains_have_catalog() {
        assert!(!top_tokens_for(&ChainId::new("eip155:1")).is_empty());
        assert!(!top_tokens_for(&ChainId::new("eip155:42161")).is_empty());
        assert!(!top_tokens_for(&ChainId::new("eip155:8453")).is_empty());
    }

    #[test]
    fn unknown_chain_returns_empty() {
        assert!(top_tokens_for(&ChainId::new("solana:mainnet")).is_empty());
    }
}
