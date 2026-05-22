//! Curve Finance sub-decoder tables + protocol address registry.
//!
//! Source (verified against `curvefi/curve-router-ng @ master`,
//! `contracts/Router.vy v1.2.0`):
//! Curve Router NG 의 `exchange` 는 `_route: address[11]` + `_swap_params:
//! uint256[5][5]` 로 multi-hop swap. `_swap_params[i][2] = swap_type` 이
//! per-hop enum (1..9).
//!
//! 본 module = Phase 12.3 의 Tier A bundle 의 reference table (per-hop 의
//! swap_type 의 의미 의 1:1 mapping). PoC scope 에서는 enum_tagged_dispatch 의
//! top-level tag 가 아닌 per-hop 의 정보 라 단순 single_emit + helper
//! BuiltinFn `curve_route_last_token` 로 simplified — 본 table 은 향후 spec
//! 확장의 anchor.
//!
//! crvUSD / veCRV / Gauge / GaugeController 는 단순 단일 함수 라 별 sub-decoder
//! 불필요 — Tier A bundle 의 `single_emit` 으로 직접 처리. Address registry 만
//! 본 module 에서 제공 (per-collateral controller / mainnet escrow / gauge
//! controller).
//!
//! Selectors (verified via `cast keccak`) 와 collateral mapping (verified via
//! on-chain `Controller.collateral_token()` static call) 모두 1차 출처 기반.

use alloy_primitives::Address;

use crate::subdecode::enum_tagged::{EnumEntry, EnumTable};

// ---------------------------------------------------------------------------
// Curve Router NG — Swap Type Enum (per-hop)
// ---------------------------------------------------------------------------
//
// Router.vy v1.2.0 docstring (verbatim):
//   The swap_type should be:
//   1. for `exchange`,
//   2. for `exchange_underlying`,
//   3. for underlying exchange via zap: factory stable metapools with lending
//      base pool `exchange_underlying` and factory crypto-meta pools
//      underlying exchange (`exchange` method in zap)
//   4. for coin -> LP token "exchange" (actually `add_liquidity`),
//   5. for lending pool underlying coin -> LP token "exchange" (actually
//      `add_liquidity`),
//   6. for LP token -> coin "exchange" (actually `remove_liquidity_one_coin`)
//   7. for LP token -> lending or fake pool underlying coin "exchange"
//      (actually `remove_liquidity_one_coin`)
//   8. for ETH <-> WETH, ETH -> stETH or ETH -> frxETH, stETH <-> wstETH,
//      ETH -> wBETH
//   9. for ERC4626 asset <-> share
//
// PoC scope 에서는 per-hop branch 의 inner expansion 미지원 — top-level
// `exchange` envelope 만 emit. 본 table 은 다음 phase 에서 per-hop expansion
// 시 reference.

const SWAP_TYPE_GENERIC_JSON: &str = r#"[
    {"name": "swap_type", "type": "uint256"}
]"#;

pub static CURVE_ROUTER_NG_SWAP_TYPES: EnumTable = EnumTable {
    name: "Curve Router NG swap_type",
    entries: &[
        EnumEntry {
            kind: 1,
            name: "STABLESWAP_EXCHANGE",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 2,
            name: "EXCHANGE_UNDERLYING",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 3,
            name: "ZAP_UNDERLYING_EXCHANGE",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 4,
            name: "COIN_TO_LP_ADD_LIQUIDITY",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 5,
            name: "LENDING_UNDERLYING_TO_LP",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 6,
            name: "LP_TO_COIN_REMOVE_LIQUIDITY_ONE_COIN",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 7,
            name: "LP_TO_LENDING_UNDERLYING",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 8,
            name: "WRAPPED_ASSET_CONVERT",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
        EnumEntry {
            kind: 9,
            name: "ERC4626_ASSET_SHARE",
            payload_json_abi: SWAP_TYPE_GENERIC_JSON,
        },
    ],
};

// ---------------------------------------------------------------------------
// Curve Router NG addresses per chain
// ---------------------------------------------------------------------------
//
// Source: `curvefi/curve-router-ng/README.md` @ commit
// `1014d3691bd9df935dc06fc5988484b0614d1fd5` — 14 chain. Mainnet = v1.2.0, 그
// 외 = v1.1.0. Fraxtal(252)/zkSync(324)/Mantle(5000)/X-Layer(196) 는 v1.0 으로
// `exchange` 의 `_swap_params` 가 `uint256[4][5]` 변종 (selector `0xaad348a2`);
// 나머지 10 chain 은 `uint256[5][5]` (selector `0xc872a3c5`) — Phase 13 검증.
// 본 table 은 (chain_id, address) 만 보유 — ABI 변종 구분은 Tier A bundle 의
// `match.selector` 책임.

/// Curve Router NG addresses per chain (14 chain). Verified via
/// `curvefi/curve-router-ng/README.md` @ `1014d369` + on-chain `version()`.
pub const CURVE_ROUTER_NG_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet (v1.2.0)
    (
        1,
        Address::new(*b"\x45\x31\x2e\xa0\xeF\xf7\xE0\x9C\x83\xCB\xE2\x49\xfa\x1d\x75\x98\xc4\xC8\xcd\x4e"),
    ),
    // Optimism
    (
        10,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // BSC
    (
        56,
        Address::new(*b"\xA7\x2C\x85\xC2\x58\xA8\x17\x61\x43\x3B\x4e\x8d\xa6\x05\x05\xFe\x3D\xd5\x51\xCC"),
    ),
    // Gnosis (xDai)
    (
        100,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // Polygon
    (
        137,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // Fantom
    (
        250,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // Fraxtal
    (
        252,
        Address::new(*b"\x56\xC5\x26\xb0\x15\x9a\x25\x88\x87\xe0\xd7\x9e\xc3\xa8\x0d\xfb\x94\x0d\x0c\xD7"),
    ),
    // zkSync Era
    (
        324,
        Address::new(*b"\x7C\x91\x53\x90\xe1\x09\xCA\x66\x93\x4f\x1e\xB2\x85\x85\x43\x75\xD1\xB1\x27\xFA"),
    ),
    // Base
    (
        8453,
        Address::new(*b"\x4f\x37\xA9\xd1\x77\x47\x04\x99\xA2\xdD\x08\x46\x21\x02\x0b\x02\x3f\xcf\xfc\x1F"),
    ),
    // Arbitrum One
    (
        42161,
        Address::new(*b"\x21\x91\x71\x8C\xD3\x2d\x02\xB8\xE6\x0B\xaD\xFF\xeA\x33\xE4\xB5\xDD\x9A\x0A\x0D"),
    ),
    // Avalanche
    (
        43114,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // Mantle
    (
        5000,
        Address::new(*b"\x4f\x37\xA9\xd1\x77\x47\x04\x99\xA2\xdD\x08\x46\x21\x02\x0b\x02\x3f\xcf\xfc\x1F"),
    ),
    // Kava (Phase 13 — README 14-chain reconcile; uint256[5][5] 변종)
    (
        2222,
        Address::new(*b"\x0D\xCD\xED\x35\x45\xD5\x65\xbA\x3B\x19\xE6\x83\x43\x13\x81\x00\x72\x45\xd9\x83"),
    ),
    // X-Layer (Phase 13 — README 14-chain reconcile; uint256[4][5] 변종)
    (
        196,
        Address::new(*b"\xBF\xab\x8e\xbc\x83\x6E\x1c\x4D\x81\x83\x77\x98\xFC\x07\x6D\x21\x9C\x9a\x18\x55"),
    ),
];

// ---------------------------------------------------------------------------
// crvUSD Controllers (collateral-specific)
// ---------------------------------------------------------------------------
//
// Per-collateral mapping verified via on-chain
// `Controller.collateral_token()` static call (Ethereum mainnet RPC). Top 3
// markets (by historical TVL — wstETH / sfrxETH / WBTC). Plan 의 initial
// prediction (wstETH 가 0xEC0820...) 은 on-chain query 의 결과 sfrxETH 였음 —
// 본 table 은 actual chain state 만 reflect.

/// One crvUSD Controller deployment.
#[derive(Debug, Clone, Copy)]
pub struct CrvusdControllerEntry {
    pub chain_id: u64,
    pub controller: Address,
    pub collateral_symbol: &'static str,
    pub collateral_token: Address,
}

/// crvUSD Controllers (collateral-specific). PoC = mainnet 만, top 3.
/// Verified via on-chain `collateral_token()` static call.
pub const CRVUSD_CONTROLLERS: &[CrvusdControllerEntry] = &[
    // wstETH market — controller 0x100daa78fc509db39ef7d04de0c1abd299f4c6ce
    // collateral 0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0 (wstETH)
    CrvusdControllerEntry {
        chain_id: 1,
        controller: Address::new(*b"\x10\x0d\xaa\x78\xfc\x50\x9d\xb3\x9e\xf7\xd0\x4d\xe0\xc1\xab\xd2\x99\xf4\xc6\xce"),
        collateral_symbol: "wstETH",
        collateral_token: Address::new(*b"\x7f\x39\xC5\x81\xF5\x95\xB5\x3c\x5c\xb1\x9b\xD0\xb3\xf8\xdA\x6c\x93\x5E\x2C\xa0"),
    },
    // sfrxETH market — controller 0xEC0820EfafC41D8943EE8dE495fC9Ba8495B15cf (v1)
    // collateral 0xac3E018457B222d93114458476f3E3416Abbe38F (sfrxETH)
    CrvusdControllerEntry {
        chain_id: 1,
        controller: Address::new(*b"\xEC\x08\x20\xEf\xaf\xC4\x1D\x89\x43\xEE\x8d\xE4\x95\xfC\x9B\xa8\x49\x5B\x15\xcf"),
        collateral_symbol: "sfrxETH",
        collateral_token: Address::new(*b"\xac\x3E\x01\x84\x57\xB2\x22\xd9\x31\x14\x45\x84\x76\xf3\xE3\x41\x6A\xbb\xe3\x8F"),
    },
    // WBTC market — controller 0x4e59541306910ad6dc1dac0ac9dfb29bd9f15c67
    // collateral 0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599 (WBTC)
    CrvusdControllerEntry {
        chain_id: 1,
        controller: Address::new(*b"\x4e\x59\x54\x13\x06\x91\x0a\xd6\xdc\x1d\xac\x0a\xc9\xdf\xb2\x9b\xd9\xf1\x5c\x67"),
        collateral_symbol: "WBTC",
        collateral_token: Address::new(*b"\x22\x60\xFA\xC5\xE5\x54\x2a\x77\x3A\xa4\x4f\xBC\xfe\xDf\x7C\x19\x3b\xc2\xC5\x99"),
    },
];

// ---------------------------------------------------------------------------
// veCRV (VotingEscrow) — mainnet only
// ---------------------------------------------------------------------------
//
// Source: `curvefi/curve-veCRV-api @ master / contracts/VotingEscrow.vy`,
// Etherscan name-tag "Curve: Voting Escrow", constructor name = "Vote-escrowed
// CRV", symbol = "veCRV" (verified via Etherscan).

/// veCRV VotingEscrow contract on Ethereum mainnet.
pub const VECRV_ADDRESS_MAINNET: Address =
    Address::new(*b"\x5f\x3b\x5D\xfE\xb7\xB2\x8C\xDb\xD7\xFA\xbA\x78\x96\x3E\xE2\x02\xa4\x94\xe2\xA2");

// ---------------------------------------------------------------------------
// GaugeController — mainnet only
// ---------------------------------------------------------------------------
//
// Source: `curvefi/curve-dao-contracts @ master / contracts/GaugeController.vy`,
// Etherscan name-tag "Curve: Gauge Controller" (verified via Etherscan).

/// GaugeController contract on Ethereum mainnet.
pub const GAUGE_CONTROLLER_ADDRESS_MAINNET: Address =
    Address::new(*b"\x2F\x50\xD5\x38\x60\x6F\xa9\xED\xD2\xB1\x1E\x24\x46\xBE\xb1\x8C\x9D\x58\x46\xbB");

// ---------------------------------------------------------------------------
// CRV token — mainnet only
// ---------------------------------------------------------------------------
//
// Source: Etherscan "Curve DAO Token (CRV)" — `Curve: CRV Token` name-tag.

/// CRV (Curve DAO Token) on Ethereum mainnet.
pub const CRV_TOKEN_MAINNET: Address =
    Address::new(*b"\xD5\x33\xa9\x49\x74\x0b\xb3\x30\x6d\x11\x9C\xC7\x77\xfa\x90\x0b\xA0\x34\xcd\x52");

// ---------------------------------------------------------------------------
// crvUSD token — mainnet only
// ---------------------------------------------------------------------------
//
// Source: Etherscan "crvUSD Stablecoin" — name "Curve.Fi USD Stablecoin",
// symbol "crvUSD" (Vyper 0.3.7).

/// crvUSD stablecoin on Ethereum mainnet.
pub const CRVUSD_TOKEN_MAINNET: Address =
    Address::new(*b"\xf9\x39\xe0\xa0\x3f\xb0\x7f\x59\xa7\x33\x14\xe7\x37\x94\xbe\x0e\x57\xac\x1b\x4e");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::enum_tagged::dispatch;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    fn encode(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
        let func = Function::parse(&format!("step{sig}")).unwrap();
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn router_ng_swap_type_stableswap_exchange() {
        let payload = encode("(uint256)", vec![DynSolValue::Uint(U256::from(1u64), 256)]);
        let d = dispatch(&payload, &CURVE_ROUTER_NG_SWAP_TYPES).unwrap();
        assert_eq!(d.kind, 1);
        assert_eq!(d.kind_name, "STABLESWAP_EXCHANGE");
    }

    #[test]
    fn router_ng_swap_type_erc4626_share() {
        // swap_type = 9 ⇒ ERC4626_ASSET_SHARE (upper bound of enum)
        let payload = encode("(uint256)", vec![DynSolValue::Uint(U256::from(9u64), 256)]);
        let d = dispatch(&payload, &CURVE_ROUTER_NG_SWAP_TYPES).unwrap();
        assert_eq!(d.kind, 9);
        assert_eq!(d.kind_name, "ERC4626_ASSET_SHARE");
    }

    #[test]
    fn router_ng_swap_type_unknown_returns_none() {
        // 0 and 10+ are not valid swap_type values per Router.vy
        let payload_zero = encode("(uint256)", vec![DynSolValue::Uint(U256::ZERO, 256)]);
        assert!(dispatch(&payload_zero, &CURVE_ROUTER_NG_SWAP_TYPES).is_none());

        let payload_ten = encode("(uint256)", vec![DynSolValue::Uint(U256::from(10u64), 256)]);
        assert!(dispatch(&payload_ten, &CURVE_ROUTER_NG_SWAP_TYPES).is_none());

        let payload_high = encode("(uint256)", vec![DynSolValue::Uint(U256::from(99u64), 256)]);
        assert!(dispatch(&payload_high, &CURVE_ROUTER_NG_SWAP_TYPES).is_none());
    }

    #[test]
    fn address_registry_lookup_mainnet_router() {
        let mainnet = CURVE_ROUTER_NG_ADDRESSES
            .iter()
            .find(|(c, _)| *c == 1)
            .map(|(_, a)| *a)
            .expect("mainnet entry");
        // 0x45312ea0eFf7E09C83CBE249fa1d7598c4C8cd4e (Router v1.2)
        assert_eq!(
            format!("{mainnet:#x}").to_lowercase(),
            "0x45312ea0eff7e09c83cbe249fa1d7598c4c8cd4e"
        );
    }

    #[test]
    fn crvusd_controllers_collateral_mapping_distinct() {
        // 3 distinct collateral tokens (wstETH / sfrxETH / WBTC) — guards
        // against copy-paste errors in the address byte literals.
        let collats: Vec<_> = CRVUSD_CONTROLLERS
            .iter()
            .map(|e| e.collateral_token)
            .collect();
        assert_eq!(collats.len(), 3);
        for i in 0..collats.len() {
            for j in (i + 1)..collats.len() {
                assert_ne!(collats[i], collats[j], "duplicate collateral token");
            }
        }
        // wstETH = 0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0
        let wsteth = CRVUSD_CONTROLLERS
            .iter()
            .find(|e| e.collateral_symbol == "wstETH")
            .expect("wstETH market");
        assert_eq!(
            format!("{:#x}", wsteth.collateral_token).to_lowercase(),
            "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
        );
    }

    #[test]
    fn curve_router_ng_table_covers_14_chains() {
        // Phase 13 — README @ 1014d369 lists 14 Router NG deployments.
        assert_eq!(CURVE_ROUTER_NG_ADDRESSES.len(), 14);
        let lookup = |chain: u64| {
            CURVE_ROUTER_NG_ADDRESSES
                .iter()
                .find(|(c, _)| *c == chain)
                .map(|(_, a)| format!("{a:#x}").to_lowercase())
        };
        // Kava (2222) shares the multi-chain CREATE2 address with OP/Gnosis/etc.
        assert_eq!(
            lookup(2222).as_deref(),
            Some("0x0dcded3545d565ba3b19e683431381007245d983")
        );
        // X-Layer (196) — distinct deployment.
        assert_eq!(
            lookup(196).as_deref(),
            Some("0xbfab8ebc836e1c4d81837798fc076d219c9a1855")
        );
    }
}
