//! Primitive wallet-state model shared by policy-server crates.
//!
//! This crate intentionally contains data types only. It has no database, RPC,
//! clock, or async runtime dependency, which keeps the model portable across
//! native tests and the browser extension's WASM boundary.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]

/// Approval and allowance state.
pub mod approval;
/// State deltas emitted by transition rules.
pub mod delta;
/// Evaluation context carried with each policy request.
pub mod eval_context;
/// Freshness-aware fields backed by external data sources.
pub mod live_field;
/// Pending transaction and off-chain intent tracking.
pub mod pending;
/// Lending, perp, launchpad, and Hyperliquid position state.
pub mod position;
/// Shared primitive value types.
pub mod primitives;
/// Serde helpers for wire-compatible primitive encodings.
pub mod serde_helpers;
/// Wallet state persistence abstraction.
pub mod store;
/// Token identity, metadata, and balance state.
pub mod token;
/// Top-level wallet state.
pub mod wallet;

pub use approval::{AllowanceSpec, ApprovalSet, Permit2Allowance};
pub use delta::{
    ApprovalScope, PendingChange, PendingRemoveReason, PositionChange, PositionPatch, StateDelta,
    TokenChange,
};
pub use eval_context::{EvalContext, RequestKind, SimulationMode};
pub use live_field::{
    AuthSpec, Confidence, DataSource, FieldRef, LiveField, OracleProvider, PendingFieldName,
    PositionFieldName, RegistryResource, TokenFieldName,
};
pub use pending::{
    AssetCommitment, NonceKey, OrderKind, PendingId, PendingKind, PendingLifecycle, PendingStatus,
    PendingTx, PerpOrderKind,
};
pub use position::{
    AirdropClaim, ClaimStatus, CoreFresh, EModeCategory, EquityAnchor, HlAccount, HlAgentApproval,
    HlBorrowLendAccount, HlBorrowLendBalance, HlBorrowLendTokenState, HlFillSummary,
    HlLeverageSetting, HlOpenOrder, HlPosition, HlSpotBalance, HlStakingAccount,
    HlStakingDelegation, HlVaultEquity, LaunchpadAllocation, LendingAccount, LongtailFresh,
    MarginMode, MerkleProof, PerpPosition, PerpSide, Position, PositionId, PositionKind, VestCurve,
    VestSchedule, VestingSchedule,
};
pub use primitives::{
    Address, BasisPoints, BlockHeight, ChainId, Decimal, Duration, MarketRef, PoolRef, Price,
    ProtocolRef, SignedI256, Spender, Time, VenueRef, Weight, U128, U256,
};
pub use store::{StoreError, WalletStore};
pub use token::{
    Balance, BaseCategory, FiatCurrency, LpShape, NoteKind, PegKind, PegTarget, RangeSpec,
    RateMode, RebaseForm, ShareForm, TokenHolding, TokenId, TokenKey, TokenKind, TokenRef,
    UnlockSchedule,
};
pub use wallet::{ApprovalEntry, WalletId, WalletState};

#[cfg(test)]
mod smoke {
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
            metadata: None,
            value_usd: None,
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

        assert_eq!(
            state.available_balance(&usdc_key),
            Some(U256::from(10_000_000_000u64))
        );

        // LiveField freshness
        let p = state.tokens[&usdc_key].price_usd.as_ref().unwrap();
        assert!(p.fresh_within(Time::from_unix(1_738_000_010), Duration::from_secs(60)));
        assert!(p.is_stale(Time::from_unix(1_738_001_000)));
    }

    #[test]
    fn state_delta_default_is_empty() {
        let d = StateDelta::default();
        assert!(d.is_empty());
    }

    #[test]
    fn registry_api_data_source_round_trip() {
        use std::str::FromStr;

        let usdc_addr = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();

        let source = DataSource::RegistryApi {
            endpoint: "http://localhost:8080".into(),
            resource: RegistryResource::TokenMeta {
                chain: ChainId::ethereum_mainnet(),
                address: usdc_addr,
            },
            version: Some("v1".into()),
        };

        let json = serde_json::to_string(&source).unwrap();
        let back: DataSource = serde_json::from_str(&json).unwrap();
        assert_eq!(source, back);
    }
}
