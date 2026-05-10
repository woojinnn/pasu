//! 토큰·라우터 주소 큐레이트 레지스트리.
//!
//! liam191의 `lookup_token_metadata` 패턴 차용 — 알려진 mainnet 주소를
//! 하드코딩하고 모르는 주소는 `UNKNOWN` (decimals=18) fallback.
//!
//! 실제 production에서는 외부 registry (CoinGecko·1inch token list 등)를
//! 의존성 주입할 수 있도록 trait으로 분리하는 게 자연스러움. v0.2 후보.

use alloy_primitives::address;

use crate::types::{Address, ChainId, Token};

// ===========================================================================
// 알려진 mainnet (chainId = 1) 토큰
// ===========================================================================

const ETH_NATIVE_SENTINEL: Address = address!("0000000000000000000000000000000000000000");
const ETH_EEEE_SENTINEL: Address = address!("EeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE");

const USDC_MAINNET: Address = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
const USDT_MAINNET: Address = address!("dAC17F958D2ee523a2206206994597C13D831ec7");
const DAI_MAINNET: Address = address!("6B175474E89094C44Da98b954EedeAC495271d0F");
const WETH_MAINNET: Address = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
const WBTC_MAINNET: Address = address!("2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599");
const STETH_MAINNET: Address = address!("ae7ab96520DE3A18E5e111B5EaAb095312D7fE84");
const WSTETH_MAINNET: Address = address!("7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0");

// Base (chainId = 8453)
const USDC_BASE: Address = address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
const WETH_BASE: Address = address!("4200000000000000000000000000000000000006");

// BSC (chainId = 56)
const BUSD_BSC: Address = address!("e9e7CEA3DedcA5984780Bafc599bD69ADd087D56");
const WBNB_BSC: Address = address!("bb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c");
const CAKE_BSC: Address = address!("0E09FaBB73Bd3Ade0a17ECC321fD13a19e81cE82");

/// 토큰 메타데이터 lookup. 알려진 주소면 (`symbol`, `decimals`) 정확. 그 외는
/// `UNKNOWN` + decimals=18 fallback (DEX swap에서 흔한 ERC20 기본).
pub fn token_metadata(address: Address, chain_id: ChainId) -> Token {
    let is_native = address == ETH_NATIVE_SENTINEL || address == ETH_EEEE_SENTINEL;
    let (symbol, decimals) = match (chain_id, address) {
        // 네이티브 sentinel
        (_, addr) if addr == ETH_NATIVE_SENTINEL || addr == ETH_EEEE_SENTINEL => {
            // 체인별 native asset symbol
            match chain_id {
                1 | 8453 => ("ETH", 18u8),
                56 => ("BNB", 18),
                _ => ("NATIVE", 18),
            }
        }
        // mainnet
        (1, USDC_MAINNET) => ("USDC", 6),
        (1, USDT_MAINNET) => ("USDT", 6),
        (1, DAI_MAINNET) => ("DAI", 18),
        (1, WETH_MAINNET) => ("WETH", 18),
        (1, WBTC_MAINNET) => ("WBTC", 8),
        (1, STETH_MAINNET) => ("stETH", 18),
        (1, WSTETH_MAINNET) => ("wstETH", 18),
        // Base
        (8453, USDC_BASE) => ("USDC", 6),
        (8453, WETH_BASE) => ("WETH", 18),
        // BSC
        (56, BUSD_BSC) => ("BUSD", 18),
        (56, WBNB_BSC) => ("WBNB", 18),
        (56, CAKE_BSC) => ("CAKE", 18),
        // fallback
        _ => ("UNKNOWN", 18),
    };

    Token {
        chain_id,
        address,
        symbol: symbol.into(),
        decimals,
        is_native,
    }
}

// ===========================================================================
// Universal Router 주소 (v0.2의 UR registry는 49 entry / 23 chain)
// ===========================================================================

/// Universal Router family — opcode 마스킹 규칙이 다름.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrFamily {
    /// Uniswap UR — `command & 0x7f` (bit 7 = `FLAG_ALLOW_REVERT`).
    Uniswap,
    /// PancakeSwap UR — `command & 0x3f` (다른 opcode 공간).
    Pancakeswap,
}

/// 주소가 알려진 Universal Router면 family 반환.
pub fn ur_family_for(address: Address, _chain_id: ChainId) -> Option<UrFamily> {
    // mainnet Uniswap UR (v1.1)
    const UNI_UR_V11_MAINNET: Address = address!("66a9893cC07D91D95644AEDD05D03f95e1dBA8Af");
    // BSC PancakeSwap UR
    const PANCAKE_UR_BSC: Address = address!("1A0A18AC4BECDDbd6389559687d1A73d8927E416");

    match address {
        UNI_UR_V11_MAINNET => Some(UrFamily::Uniswap),
        PANCAKE_UR_BSC => Some(UrFamily::Pancakeswap),
        _ => None,
    }
}

/// UR family에 따른 opcode 마스킹.
pub fn mask_for(family: UrFamily) -> u8 {
    match family {
        UrFamily::Uniswap => 0x7f,
        UrFamily::Pancakeswap => 0x3f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_token_lookup() {
        let usdc = token_metadata(USDC_MAINNET, 1);
        assert_eq!(usdc.symbol, "USDC");
        assert_eq!(usdc.decimals, 6);
        assert!(!usdc.is_native);
    }

    #[test]
    fn native_eth_sentinel() {
        let eth = token_metadata(ETH_NATIVE_SENTINEL, 1);
        assert_eq!(eth.symbol, "ETH");
        assert!(eth.is_native);
    }

    #[test]
    fn unknown_token_fallback() {
        let unknown_addr: Address = "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let t = token_metadata(unknown_addr, 1);
        assert_eq!(t.symbol, "UNKNOWN");
        assert_eq!(t.decimals, 18);
    }

    #[test]
    fn ur_family_recognized() {
        let mainnet_ur: Address = "0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af"
            .parse()
            .unwrap();
        assert_eq!(ur_family_for(mainnet_ur, 1), Some(UrFamily::Uniswap));
        assert_eq!(mask_for(UrFamily::Uniswap), 0x7f);
        assert_eq!(mask_for(UrFamily::Pancakeswap), 0x3f);
    }
}
