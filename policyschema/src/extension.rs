//! `Extension` — `Action`에 부착되는 프로토콜 특수 데이터를 `(namespace, data)`
//! 페어로 표현. 코어 `ActionFields`를 가볍게 유지.
//!
//! v0.1 보강에서 13 → **38종** namespace로 확장. 사용자 결정에 따라 Bridge·Perp는
//! 제외했지만 그 외 schema_v260508의 32종 + Governance·NFT·Vault 관련을 모두 포함.
//!
//! 두 namespace는 *통합*되어 `data` 안에 `component` 식별자를 둠:
//! - `pancakeswap` — V2 / V3 / SmartRouter / UniversalRouter / Infinity
//! - `lido`       — stETH / wstETH / WithdrawalQueue
//!
//! namespace별 `data` 구조는 `docs/extensions/<namespace>.md` 참조.

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extension {
    /// 안정 id (예: `e#uniswap-v2`).
    pub id: String,
    pub scope: ExtensionScope,
    pub namespace: ExtensionNamespace,
    /// `data` 페이로드의 스키마 버전 (namespace별).
    pub version: String,
    /// namespace 특수 페이로드.
    pub data: serde_json::Value,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtensionScope {
    /// 특정 Action에 결합.
    Action {
        #[serde(rename = "actionId")]
        action_id: String,
    },
    /// 전체 Request에 결합 (드물다 — 최상위 메타데이터용).
    Request,
}

/// Extension namespace — schema_v260508 32종 + Governance/NFT/Vault 관련 확장.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtensionNamespace {
    // ─── DEX (10) ───
    #[serde(rename = "uniswap.v2")]
    UniswapV2,
    #[serde(rename = "uniswap.v3")]
    UniswapV3,
    #[serde(rename = "uniswap.v4")]
    UniswapV4,
    #[serde(rename = "uniswap.universalRouter")]
    UniswapUniversalRouter,
    /// 통합 — `data.component` ∈ {`v2`, `v3`, `smartRouter`, `universalRouter`, `infinity`}.
    #[serde(rename = "pancakeswap")]
    Pancakeswap,
    #[serde(rename = "aerodrome.v1")]
    AerodromeV1,
    #[serde(rename = "aerodrome.slipstream")]
    AerodromeSlipstream,
    #[serde(rename = "balancer.vault")]
    BalancerVault,
    #[serde(rename = "curve.stableswap")]
    CurveStableswap,
    #[serde(rename = "1inch.aggregator")]
    OneInchAggregator,

    // ─── Lending (4) ───
    #[serde(rename = "aave.v3")]
    AaveV3,
    #[serde(rename = "morpho.blue")]
    MorphoBlue,
    #[serde(rename = "spark.lend")]
    SparkLend,
    #[serde(rename = "compound.v3")]
    CompoundV3,

    // ─── Liquid Staking (3) ───
    /// 통합 — `data.component` ∈ {`stETH`, `wstETH`, `withdrawalQueue`}.
    #[serde(rename = "lido")]
    Lido,
    #[serde(rename = "rocketPool")]
    RocketPool,
    #[serde(rename = "mantle.meth")]
    MantleMeth,

    // ─── Restaking / LRT (5) ───
    #[serde(rename = "eigenlayer.core")]
    EigenlayerCore,
    #[serde(rename = "eigenlayer.eigenpod")]
    EigenlayerEigenpod,
    #[serde(rename = "etherfi")]
    Etherfi,
    #[serde(rename = "kelp")]
    Kelp,
    #[serde(rename = "renzo")]
    Renzo,

    // ─── RWA (4) ───
    #[serde(rename = "centrifuge.erc7540")]
    CentrifugeErc7540,
    #[serde(rename = "ondo.usdy")]
    OndoUsdy,
    #[serde(rename = "securitize.dsProtocol")]
    SecuritizeDsProtocol,
    #[serde(rename = "blackrock.buidl")]
    BlackrockBuidl,

    // ─── Governance (3) ⭐ 신규 ───
    #[serde(rename = "governance.governorBravo")]
    GovernanceGovernorBravo,
    #[serde(rename = "governance.openzeppelin")]
    GovernanceOpenzeppelin,
    #[serde(rename = "governance.snapshot")]
    GovernanceSnapshot,

    // ─── NFT (3) ⭐ 신규 ───
    #[serde(rename = "nft.seaport")]
    NftSeaport,
    #[serde(rename = "nft.blur")]
    NftBlur,
    #[serde(rename = "nft.x2y2")]
    NftX2y2,

    // ─── Vault (2) ⭐ 신규 ───
    #[serde(rename = "vault.erc4626")]
    VaultErc4626,
    #[serde(rename = "vault.yearn")]
    VaultYearn,

    // ─── Sign (3) ───
    #[serde(rename = "permit2")]
    Permit2,
    #[serde(rename = "eip2612")]
    Eip2612,
    #[serde(rename = "eip712")]
    Eip712,

    // ─── 토큰 표준 (2) ───
    #[serde(rename = "erc20")]
    Erc20,
    #[serde(rename = "weth")]
    Weth,

    // ─── Account Abstraction (1) ⭐ 신규 ───
    #[serde(rename = "safe")]
    Safe,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_namespace_round_trip() {
        // 38 namespace 모두 round-trip
        let all = [
            ExtensionNamespace::UniswapV2,
            ExtensionNamespace::UniswapV3,
            ExtensionNamespace::UniswapV4,
            ExtensionNamespace::UniswapUniversalRouter,
            ExtensionNamespace::Pancakeswap,
            ExtensionNamespace::AerodromeV1,
            ExtensionNamespace::AerodromeSlipstream,
            ExtensionNamespace::BalancerVault,
            ExtensionNamespace::CurveStableswap,
            ExtensionNamespace::OneInchAggregator,
            ExtensionNamespace::AaveV3,
            ExtensionNamespace::MorphoBlue,
            ExtensionNamespace::SparkLend,
            ExtensionNamespace::CompoundV3,
            ExtensionNamespace::Lido,
            ExtensionNamespace::RocketPool,
            ExtensionNamespace::MantleMeth,
            ExtensionNamespace::EigenlayerCore,
            ExtensionNamespace::EigenlayerEigenpod,
            ExtensionNamespace::Etherfi,
            ExtensionNamespace::Kelp,
            ExtensionNamespace::Renzo,
            ExtensionNamespace::CentrifugeErc7540,
            ExtensionNamespace::OndoUsdy,
            ExtensionNamespace::SecuritizeDsProtocol,
            ExtensionNamespace::BlackrockBuidl,
            ExtensionNamespace::GovernanceGovernorBravo,
            ExtensionNamespace::GovernanceOpenzeppelin,
            ExtensionNamespace::GovernanceSnapshot,
            ExtensionNamespace::NftSeaport,
            ExtensionNamespace::NftBlur,
            ExtensionNamespace::NftX2y2,
            ExtensionNamespace::VaultErc4626,
            ExtensionNamespace::VaultYearn,
            ExtensionNamespace::Permit2,
            ExtensionNamespace::Eip2612,
            ExtensionNamespace::Eip712,
            ExtensionNamespace::Erc20,
            ExtensionNamespace::Weth,
            ExtensionNamespace::Safe,
        ];
        assert_eq!(all.len(), 40, "ExtensionNamespace 40종 (DEX 10 + Lending 4 + LST 3 + Restaking 5 + RWA 4 + Governance 3 + NFT 3 + Vault 2 + Sign 3 + 토큰 2 + AA 1)");
        for ns in all {
            let s = serde_json::to_string(&ns).unwrap();
            let back: ExtensionNamespace = serde_json::from_str(&s).unwrap();
            assert_eq!(ns, back);
        }
    }

    #[test]
    fn lido_namespace_renames_to_lower_lido() {
        assert_eq!(
            serde_json::to_string(&ExtensionNamespace::Lido).unwrap(),
            "\"lido\""
        );
    }
}
