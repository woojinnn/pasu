//! Action 의 `OnchainView` `LiveField` 들이 필요로 하는 ABI 인자를 동적으로 인코딩.
//!
//! 배경:
//! `DataSource::OnchainView { contract, function, decoder_id }` 자체에는 args 가
//! 없다. `balanceOf(address)` 같이 인자가 필요한 호출에서는 호출 시점에
//! 인자를 동적으로 채워야 한다. action 의 다른 필드 (venue.pool, asset, submitter)
//! 에서 그 인자를 추출.
//!
//! 설계: slot-keyed resolver — `(ActionSlot, Action, WalletState)` → `Vec<u8>` (ABI 인자).
//!
//! 현재 wired:
//! * `LendingBorrowAvailableLiquidity`  → balanceOf(pool) — pool 주소를 인자로
//! * `LendingBorrowUserState`           → getUserAccountData(user) — submitter
//! * `LendingBorrowReserveState`        → getReserveData(asset) — borrow asset
//!
//! 나머지는 후속 패스. 기본값 = 빈 args (인자 없는 함수).

use simulation_reducer::action::lending::LendingAction;
use simulation_reducer::action::liquid_staking::LiquidStakingAction;
use simulation_reducer::action::{Action, ActionBody};
use simulation_state::WalletState;

use crate::fetchers::decoder::{encode_address, encode_u256};
use crate::walker::ActionSlot;

/// 한 slot 이 필요로 하는 ABI 인자를 인코딩. 인자 없는 함수면 빈 벡터.
#[must_use]
pub fn resolve_args(slot: &ActionSlot, action: &Action, _state: &WalletState) -> Vec<u8> {
    match slot {
        // ─── Aave Borrow ───
        ActionSlot::LendingBorrowAvailableLiquidity => {
            // balanceOf(pool) — 자산 컨트랙트의 잔고를 pool 이 얼마나 들고 있나
            if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
                if let Some(pool) = lending_venue_pool_address(&b.venue) {
                    return encode_address(pool).to_vec();
                }
            }
            Vec::new()
        }
        ActionSlot::LendingBorrowUserState => {
            // getUserAccountData(user) — submitter 의 lending 상태
            encode_address(action.meta.submitter).to_vec()
        }
        // 둘 다 getReserveData(asset) 호출 — 같은 인자 (borrow 자산 주소)
        ActionSlot::LendingBorrowReserveState | ActionSlot::LendingBorrowCurrentRate => {
            if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
                if let Some(addr) = token_ref_to_address(&b.asset) {
                    return encode_address(addr).to_vec();
                }
            }
            Vec::new()
        }

        // ─── Aave Supply (같은 패턴, 후속에서 wire-up) ───
        // ActionSlot::LendingSupplyReserveState  → getReserveData(asset)
        // ActionSlot::LendingSupplyUserState     → getUserAccountData(user)

        // ─── Lido Liquid Staking (단일 uint256 환산 view) ───
        // wstETH getWstETHByStETH(amount) — wrap 이 받을 wstETH
        ActionSlot::LiquidStakingWrapExpectedWsteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Wrap(w)) = &action.body {
                return encode_u256(w.amount).to_vec();
            }
            Vec::new()
        }
        // wstETH getStETHByWstETH(amount) — unwrap 이 돌려줄 stETH
        ActionSlot::LiquidStakingUnwrapExpectedSteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Unwrap(u)) = &action.body {
                return encode_u256(u.amount).to_vec();
            }
            Vec::new()
        }
        // stETH getPooledEthByShares(shares) — 전송 shares 의 stETH 환산
        ActionSlot::LiquidStakingTransferSharesPooledEth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(t)) = &action.body
            {
                return encode_u256(t.shares).to_vec();
            }
            Vec::new()
        }

        // 그 외 slot 은 args 없음 (Chainlink, no-arg 함수 등)
        _ => Vec::new(),
    }
}

/// `LendingVenue` 의 pool 주소 추출 (Aave/Compound/Morpho/...).
const fn lending_venue_pool_address(
    venue: &simulation_reducer::action::lending::LendingVenue,
) -> Option<simulation_state::Address> {
    use simulation_reducer::action::lending::LendingVenue::{
        AaveV2, AaveV3, CompoundV2, CompoundV3, CrvUsd, Fluid, MorphoBlue, MorphoOptimizer, Spark,
    };
    match venue {
        AaveV3 { pool, .. } | AaveV2 { pool, .. } | Spark { pool, .. } => Some(*pool),
        CompoundV3 { comet, .. } => Some(*comet),
        CompoundV2 { comptroller, .. } => Some(*comptroller),
        MorphoOptimizer { vault, .. } | Fluid { vault, .. } => Some(*vault),
        // crvUSD: collateral market 당 Controller 1개 = pool.
        CrvUsd { controller, .. } => Some(*controller),
        // Morpho Blue 는 single market id 기반, pool address 없음
        MorphoBlue { .. } => None,
    }
}

/// `TokenRef` → 그 토큰의 ERC20 address. Native/NFT 면 None.
const fn token_ref_to_address(
    token_ref: &simulation_state::TokenRef,
) -> Option<simulation_state::Address> {
    use simulation_state::TokenKey;
    match &token_ref.key {
        TokenKey::Erc20 { address, .. } => Some(*address),
        TokenKey::Erc721 { contract, .. } | TokenKey::Erc1155 { contract, .. } => Some(*contract),
        TokenKey::Native { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use simulation_reducer::action::lending::{
        BorrowAction, BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState,
    };
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature};
    use simulation_state::{
        Address, ChainId, DataSource, Decimal, LiveField, RateMode, Time, TokenKey, TokenRef,
        WalletId, U256,
    };
    use std::str::FromStr;

    fn empty_reserve() -> ReserveState {
        ReserveState {
            total_supply: U256::ZERO,
            total_borrow: U256::ZERO,
            utilization_bp: 0,
            supply_cap: None,
            borrow_cap: None,
            ltv_bp: 0,
            liquidation_threshold_bp: 0,
            liquidation_bonus_bp: 0,
            reserve_factor_bp: 0,
            is_frozen: false,
            is_paused: false,
        }
    }
    fn empty_user() -> UserLendingState {
        UserLendingState {
            health_factor: Decimal::from("0"),
            total_collat_usd: U256::ZERO,
            total_debt_usd: U256::ZERO,
            available_borrow_usd: U256::ZERO,
        }
    }

    fn mk_borrow_action(pool: Address, asset: Address, submitter: Address) -> Action {
        let chain = ChainId::ethereum_mainnet();
        let src = DataSource::OnchainView {
            chain: chain.clone(),
            contract: pool,
            function: "x".into(),
            decoder_id: "x".into(),
        };
        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(0),
                submitter,
                nature: ActionNature::OnchainTx {
                    chain: chain.clone(),
                    nonce: 0,
                    gas_limit: U256::from(200_000u64),
                    gas_price: LiveField::new(
                        U256::ZERO,
                        DataSource::UserSupplied,
                        Time::from_unix(0),
                    ),
                    value: U256::ZERO,
                },
            },
            body: ActionBody::Lending(LendingAction::Borrow(BorrowAction {
                venue: LendingVenue::AaveV3 {
                    chain: chain.clone(),
                    pool,
                    market_id: None,
                },
                asset: TokenRef {
                    key: TokenKey::Erc20 {
                        chain,
                        address: asset,
                    },
                },
                amount: U256::from(500u64),
                rate_mode: RateMode::Variable,
                on_behalf_of: None,
                live_inputs: BorrowLiveInputs {
                    reserve_state: LiveField::new(empty_reserve(), src.clone(), Time::from_unix(0)),
                    user_state_before: LiveField::new(
                        empty_user(),
                        src.clone(),
                        Time::from_unix(0),
                    ),
                    asset_price_usd: LiveField::new(
                        Decimal::from("0"),
                        DataSource::OracleFeed {
                            provider: simulation_state::OracleProvider::Chainlink,
                            feed_id: "USDC/USD".into(),
                        },
                        Time::from_unix(0),
                    ),
                    current_borrow_rate: LiveField::new(
                        Decimal::from("0"),
                        src.clone(),
                        Time::from_unix(0),
                    ),
                    available_liquidity: LiveField::new(U256::ZERO, src, Time::from_unix(0)),
                },
            })),
        }
    }

    fn dummy_state() -> WalletState {
        WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]))
    }

    #[test]
    fn args_for_available_liquidity_is_pool_address() {
        let pool = Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap();
        let asset = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let submitter = Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();
        let action = mk_borrow_action(pool, asset, submitter);

        let args = resolve_args(
            &ActionSlot::LendingBorrowAvailableLiquidity,
            &action,
            &dummy_state(),
        );

        assert_eq!(args.len(), 32);
        // 마지막 20 bytes = pool address
        assert_eq!(&args[12..], pool.as_slice());
    }

    #[test]
    fn args_for_user_state_is_submitter() {
        let pool = Address::ZERO;
        let asset = Address::ZERO;
        let submitter = Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();
        let action = mk_borrow_action(pool, asset, submitter);

        let args = resolve_args(&ActionSlot::LendingBorrowUserState, &action, &dummy_state());

        assert_eq!(args.len(), 32);
        assert_eq!(&args[12..], submitter.as_slice());
    }

    #[test]
    fn args_for_reserve_state_is_asset() {
        let pool = Address::ZERO;
        let asset = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let submitter = Address::ZERO;
        let action = mk_borrow_action(pool, asset, submitter);

        let args = resolve_args(
            &ActionSlot::LendingBorrowReserveState,
            &action,
            &dummy_state(),
        );

        assert_eq!(args.len(), 32);
        assert_eq!(&args[12..], asset.as_slice());
    }

    #[test]
    fn args_for_no_arg_slot_is_empty() {
        let pool = Address::ZERO;
        let asset = Address::ZERO;
        let action = mk_borrow_action(pool, asset, Address::ZERO);
        let args = resolve_args(
            &ActionSlot::LendingBorrowAssetPriceUsd,
            &action,
            &dummy_state(),
        );
        assert!(args.is_empty());
    }
}
