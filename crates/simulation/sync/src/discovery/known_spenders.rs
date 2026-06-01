//! Hardcoded per-chain catalog of "famous" smart contracts that wallets
//! commonly grant ERC-20 approvals to (Permit2, DEX routers, lending
//! pools, marketplaces). Used for approval discovery via batched
//! `allowance(owner, spender)` Multicall round-trips.
//!
//! Coverage is intentionally biased toward high-volume protocols
//! (Uniswap, 1inch, Aave, Compound, OpenSea, Permit2). Long-tail
//! spenders get missed; a future enhancement can scan recent `Approval`
//! events via `eth_getLogs` to catch those.

use simulation_state::primitives::ChainId;

/// One row in the per-chain spender catalog.
pub(super) struct KnownSpender {
    pub address: &'static str,
    /// Human-readable name. Logged for diagnostics; never reaches users.
    #[allow(dead_code)]
    pub name: &'static str,
}

/// Returns the curated spender catalog for `chain`. Empty slice for
/// chains we don't have a catalog for (caller falls through to empty
/// approval set).
pub(super) fn known_spenders_for(chain: &ChainId) -> &'static [KnownSpender] {
    match chain.as_str() {
        "eip155:1" => ETH_MAINNET,
        "eip155:42161" => ARBITRUM,
        "eip155:8453" => BASE,
        "eip155:10" => OPTIMISM,
        "eip155:137" => POLYGON,
        _ => &[],
    }
}

// ─── Ethereum Mainnet ────────────────────────────────────────────────────
//
// Curated 2026; addresses verified against etherscan + the project docs
// of each protocol. The list intentionally skips niche / one-off
// deployments — high-volume protocols only.
const ETH_MAINNET: &[KnownSpender] = &[
    // ── Permit2 (Uniswap; used by many aggregators as the universal allowance proxy) ──
    KnownSpender {
        address: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        name: "Permit2",
    },
    // ── Uniswap ──
    KnownSpender {
        address: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
        name: "Uniswap V2 Router",
    },
    KnownSpender {
        address: "0xe592427a0aece92de3edee1f18e0157c05861564",
        name: "Uniswap V3 SwapRouter",
    },
    KnownSpender {
        address: "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
        name: "Uniswap V3 SwapRouter02",
    },
    KnownSpender {
        address: "0x3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad",
        name: "Uniswap Universal Router",
    },
    // ── Aggregators ──
    KnownSpender {
        address: "0x1111111254eeb25477b68fb85ed929f73a960582",
        name: "1inch v5 Router",
    },
    KnownSpender {
        address: "0x111111125421ca6dc452d289314280a0f8842a65",
        name: "1inch v6 Router",
    },
    KnownSpender {
        address: "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
        name: "CowSwap GPv2 Settlement",
    },
    KnownSpender {
        address: "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
        name: "0x ExchangeProxy",
    },
    // ── Lending ──
    KnownSpender {
        address: "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2",
        name: "Aave V3 Pool",
    },
    KnownSpender {
        address: "0x7d2768de32b0b80b7a3454c06bdac94a69ddc7a9",
        name: "Aave V2 LendingPool",
    },
    KnownSpender {
        address: "0xc3d688b66703497daa19211eedff47f25384cdc3",
        name: "Compound V3 USDC",
    },
    // ── Curve ──
    KnownSpender {
        address: "0xf0d4c12a5768d806021f80a262b4d39d26c58b8d",
        name: "Curve Router",
    },
    // ── Lido ──
    KnownSpender {
        address: "0xae7ab96520de3a18e5e111b5eaab095312d7fe84",
        name: "Lido stETH (wrapping)",
    },
    KnownSpender {
        address: "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0",
        name: "Lido wstETH",
    },
    // ── Pendle ──
    KnownSpender {
        address: "0x888888888889758f76e7103c6cbf23abbf58f946",
        name: "Pendle Router V4",
    },
    // ── EigenLayer ──
    KnownSpender {
        address: "0x858646372cc42e1a627fce94aa7a7033e7cf075a",
        name: "EigenLayer StrategyManager",
    },
    // ── Marketplaces / NFT (still ERC-20 approve via WETH for bids) ──
    KnownSpender {
        address: "0x0000000000000068f116a894984e2db1123eb395",
        name: "OpenSea Seaport 1.6",
    },
    KnownSpender {
        address: "0x00000000000001ad428e4906ae43d8f9852d0dd6",
        name: "OpenSea Seaport 1.5",
    },
    KnownSpender {
        address: "0x000000000000ad05ccc4f10045630fb830b95127",
        name: "Blur Exchange",
    },
];

// ─── Arbitrum One ────────────────────────────────────────────────────────
const ARBITRUM: &[KnownSpender] = &[
    KnownSpender {
        address: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        name: "Permit2",
    },
    KnownSpender {
        address: "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
        name: "Uniswap V3 SwapRouter02",
    },
    KnownSpender {
        address: "0x5e325eda8064b456f4781070c0738d849c824258",
        name: "Uniswap Universal Router",
    },
    KnownSpender {
        address: "0x111111125421ca6dc452d289314280a0f8842a65",
        name: "1inch v6 Router",
    },
    KnownSpender {
        address: "0x794a61358d6845594f94dc1db02a252b5b4814ad",
        name: "Aave V3 Pool",
    },
    KnownSpender {
        address: "0x489ee077994b6658eafa855c308275ead8097c4a",
        name: "GMX Vault",
    },
    KnownSpender {
        address: "0xabbc5f99639c9b6bcb58544ddf04efa6802f4064",
        name: "GMX Router",
    },
    KnownSpender {
        address: "0xc873fecbd354f5a56e00e710b90ef4201db2448d",
        name: "Camelot Router",
    },
];

// ─── Base ────────────────────────────────────────────────────────────────
const BASE: &[KnownSpender] = &[
    KnownSpender {
        address: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        name: "Permit2",
    },
    KnownSpender {
        address: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        name: "Aerodrome Router",
    },
    KnownSpender {
        address: "0x2626664c2603336e57b271c5c0b26f421741e481",
        name: "Uniswap V3 SwapRouter02",
    },
    KnownSpender {
        address: "0x3fc91a3afd70395cd496c647d5a6cc9d4b2b7fad",
        name: "Uniswap Universal Router",
    },
    KnownSpender {
        address: "0x111111125421ca6dc452d289314280a0f8842a65",
        name: "1inch v6 Router",
    },
    KnownSpender {
        address: "0xa238dd80c259a72e81d7e4664a9801593f98d1c5",
        name: "Aave V3 Pool",
    },
];

// ─── Optimism ────────────────────────────────────────────────────────────
const OPTIMISM: &[KnownSpender] = &[
    KnownSpender {
        address: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        name: "Permit2",
    },
    KnownSpender {
        address: "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
        name: "Uniswap V3 SwapRouter02",
    },
    KnownSpender {
        address: "0xcb1355ff08ab38bbce60111f1bb2b784be25d7e8",
        name: "Uniswap Universal Router",
    },
    KnownSpender {
        address: "0x111111125421ca6dc452d289314280a0f8842a65",
        name: "1inch v6 Router",
    },
    KnownSpender {
        address: "0x794a61358d6845594f94dc1db02a252b5b4814ad",
        name: "Aave V3 Pool",
    },
    KnownSpender {
        address: "0x9c12939390052919af3155f41bf4160fd3666a6f",
        name: "Velodrome Router",
    },
];

// ─── Polygon ─────────────────────────────────────────────────────────────
const POLYGON: &[KnownSpender] = &[
    KnownSpender {
        address: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        name: "Permit2",
    },
    KnownSpender {
        address: "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
        name: "Uniswap V3 SwapRouter02",
    },
    KnownSpender {
        address: "0xec7be89e9d109e7e3fec59c222cf297125fefda2",
        name: "Uniswap Universal Router",
    },
    KnownSpender {
        address: "0x111111125421ca6dc452d289314280a0f8842a65",
        name: "1inch v6 Router",
    },
    KnownSpender {
        address: "0x794a61358d6845594f94dc1db02a252b5b4814ad",
        name: "Aave V3 Pool",
    },
    KnownSpender {
        address: "0xa5e0829caced8ffdd4de3c43696c57f7d7a678ff",
        name: "QuickSwap Router",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn supported_chains_have_at_least_permit2() {
        for chain in [
            "eip155:1",
            "eip155:42161",
            "eip155:8453",
            "eip155:10",
            "eip155:137",
        ] {
            let s = known_spenders_for(&ChainId::new(chain));
            assert!(
                s.iter()
                    .any(|k| k.address == "0x000000000022d473030f116ddee9f6b43ac78ba3"),
                "{chain} missing Permit2"
            );
        }
    }

    #[test]
    fn addresses_are_well_formed_lowercase_hex() {
        for chain in [
            "eip155:1",
            "eip155:42161",
            "eip155:8453",
            "eip155:10",
            "eip155:137",
        ] {
            for k in known_spenders_for(&ChainId::new(chain)) {
                assert!(
                    k.address.starts_with("0x") && k.address.len() == 42,
                    "{chain}: bad address `{}`",
                    k.address
                );
                assert!(
                    k.address
                        .chars()
                        .skip(2)
                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                    "{chain}: address not lowercase hex `{}`",
                    k.address
                );
                // Must parse as an Address.
                simulation_state::primitives::Address::from_str(k.address)
                    .unwrap_or_else(|e| panic!("{chain}: address parse `{}`: {e}", k.address));
            }
        }
    }

    #[test]
    fn unknown_chain_returns_empty() {
        assert!(known_spenders_for(&ChainId::new("solana:mainnet")).is_empty());
    }
}
