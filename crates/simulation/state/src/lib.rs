//! `simulation-state` — Scopeball 시뮬레이터의 코어 타입 정의.
//!
//! 이 crate 는 **순수 타입만** 정의한다. DB / RPC / async runtime 같은 외부 IO 는
//! 일절 가지지 않는다. 그 결과:
//!
//! * wasm 빌드 가능 (정책 엔진 wasm 이 그대로 사용)
//! * `serde` 로 JSON round-trip 가능 (fixture, snapshot, 디버깅)
//! * `simulation-db` / `simulation-reducer` / `simulation-sync` 가 모두 이 crate
//!   를 import 해서 같은 모양을 공유
//!
//! 모듈 구성은 spec §3–§9 의 섹션 구조를 그대로 반영한다.

pub mod approval;
pub mod delta;
pub mod eval_context;
pub mod live_field;
pub mod pending;
pub mod position;
pub mod primitives;
pub mod serde_helpers;
pub mod token;
pub mod wallet;

// === 자주 쓰는 타입의 짧은 경로 re-export ===

pub use approval::{AllowanceSpec, ApprovalSet, Permit2Allowance};
pub use delta::{
    ApprovalScope, PendingChange, PendingRemoveReason, PositionChange, PositionPatch, StateDelta,
    TokenChange,
};
pub use eval_context::{EvalContext, RequestKind, SimulationMode};
pub use live_field::{
    AuthSpec, Confidence, DataSource, FieldRef, LiveField, OracleProvider, PendingFieldName,
    PositionFieldName, TokenFieldName,
};
pub use pending::{
    AssetCommitment, NonceKey, OrderKind, PendingId, PendingKind, PendingLifecycle, PendingStatus,
    PendingTx, PerpOrderKind,
};
pub use position::{
    AirdropClaim, ClaimStatus, EModeCategory, LaunchpadAllocation, LendingAccount, MarginMode,
    MerkleProof, PerpPosition, PerpSide, Position, PositionId, PositionKind, VestCurve,
    VestSchedule, VestingSchedule,
};
pub use primitives::{
    Address, BasisPoints, BlockHeight, ChainId, Decimal, Duration, MarketRef, PoolRef, Price,
    ProtocolRef, SignedI256, Spender, Time, U128, U256, VenueRef, Weight,
};
pub use token::{
    Balance, BaseCategory, FiatCurrency, LpShape, NoteKind, PegKind, PegTarget, RangeSpec,
    RateMode, RebaseForm, ShareForm, TokenHolding, TokenId, TokenKey, TokenKind, TokenRef,
    UnlockSchedule,
};
pub use wallet::{ApprovalEntry, WalletId, WalletState};

#[cfg(test)]
mod smoke {
    //! 가장 단순한 round-trip 테스트 — crate 전체가 한 덩어리로 컴파일·직렬화·역직렬화
    //! 되는지 확인.

    use super::*;
    use std::str::FromStr;

    #[test]
    fn empty_wallet_state_round_trip() {
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let state = WalletState::new(WalletId::new(addr, [ChainId::ethereum_mainnet()]));

        let json = serde_json::to_string(&state).unwrap();
        let back: WalletState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);
    }

    #[test]
    fn usdc_holding_with_live_price() {
        use std::str::FromStr;

        let addr = Address::from_str("0x000000000000000000000000000000000000a01c").unwrap();
        let usdc_contract =
            Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();

        let usdc_key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc_contract,
        };

        let holding = TokenHolding {
            key: usdc_key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(10_000_000_000u64)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Decimal::new("1.0001"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1_738_000_000),
                )
                .with_ttl(Duration::from_secs(60)),
            ),
            last_synced_at: Time::from_unix(1_738_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: usdc_contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        };

        let mut state = WalletState::new(WalletId::new(addr, [ChainId::ethereum_mainnet()]));
        state.tokens.insert(usdc_key.clone(), holding);

        let json = serde_json::to_string_pretty(&state).unwrap();
        let back: WalletState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, back);

        // 헬퍼 동작 확인
        assert_eq!(
            state.available_balance(&usdc_key),
            Some(U256::from(10_000_000_000u64))
        );

        // LiveField freshness
        let p = state.tokens[&usdc_key].price_usd.as_ref().unwrap();
        assert!(p.fresh_within(Time::from_unix(1_738_000_010), Duration::from_secs(60)));
        assert!(p.is_stale(Time::from_unix(1_738_001_000))); // 1000s 후라 ttl 초과
    }

    #[test]
    fn state_delta_default_is_empty() {
        let d = StateDelta::default();
        assert!(d.is_empty());
    }
}
