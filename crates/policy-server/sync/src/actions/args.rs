//! * `LendingBorrowUserState`           → getUserAccountData(user) — submitter
//! * `LendingBorrowReserveState`        → getReserveData(asset) — borrow asset
//!
//! Remaining slots default to empty args, which covers no-arg views and slots
//! that are intentionally wired in a later pass.

use policy_state::WalletState;
use policy_transition::action::lending::LendingAction;
use policy_transition::action::liquid_staking::LiquidStakingAction;
use policy_transition::action::{Action, ActionBody};

use crate::fetchers::decoder::{encode_address, encode_u256};
use crate::walker::ActionSlot;

#[must_use]
pub fn resolve_args(slot: &ActionSlot, action: &Action, _state: &WalletState) -> Vec<u8> {
    match slot {
        // ─── Aave Borrow ───
        ActionSlot::LendingBorrowAvailableLiquidity => {
            if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
                if let Some(pool) = lending_venue_pool_address(&b.venue) {
                    return encode_address(pool).to_vec();
                }
            }
            Vec::new()
        }
        ActionSlot::LendingBorrowUserState => encode_address(action.meta.submitter).to_vec(),
        ActionSlot::LendingBorrowReserveState | ActionSlot::LendingBorrowCurrentRate => {
            if let ActionBody::Lending(LendingAction::Borrow(b)) = &action.body {
                if let Some(addr) = token_ref_to_address(&b.asset) {
                    return encode_address(addr).to_vec();
                }
            }
            Vec::new()
        }

        // ActionSlot::LendingSupplyReserveState  → getReserveData(asset)
        // ActionSlot::LendingSupplyUserState     → getUserAccountData(user)

        // Lido liquid staking conversion views.
        ActionSlot::LiquidStakingWrapExpectedWsteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Wrap(w)) = &action.body {
                return encode_u256(w.amount).to_vec();
            }
            Vec::new()
        }
        ActionSlot::LiquidStakingUnwrapExpectedSteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Unwrap(u)) = &action.body {
                return encode_u256(u.amount).to_vec();
            }
            Vec::new()
        }
        ActionSlot::LiquidStakingTransferSharesPooledEth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(t)) = &action.body
            {
                return encode_u256(t.shares).to_vec();
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

const fn lending_venue_pool_address(
    venue: &policy_transition::action::lending::LendingVenue,
) -> Option<policy_state::Address> {
    use policy_transition::action::lending::LendingVenue::{
        AaveV2, AaveV3, CompoundV2, CompoundV3, CrvUsd, Fluid, LlamaLend, MorphoBlue,
        MorphoOptimizer, Spark,
    };
    match venue {
        AaveV3 { pool, .. } | AaveV2 { pool, .. } | Spark { pool, .. } => Some(*pool),
        CompoundV3 { comet, .. } => Some(*comet),
        CompoundV2 { comptroller, .. } => Some(*comptroller),
        MorphoOptimizer { vault, .. } | Fluid { vault, .. } => Some(*vault),
        // crvUSD and LlamaLend use one controller address per market.
        CrvUsd { controller, .. } | LlamaLend { controller, .. } => Some(*controller),
        MorphoBlue { .. } => None,
    }
}

const fn token_ref_to_address(token_ref: &policy_state::TokenRef) -> Option<policy_state::Address> {
    use policy_state::TokenKey;
    match &token_ref.key {
        TokenKey::Erc20 { address, .. } => Some(*address),
        TokenKey::Erc721 { contract, .. } | TokenKey::Erc1155 { contract, .. } => Some(*contract),
        TokenKey::Native { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use policy_state::{
        Address, ChainId, DataSource, Decimal, LiveField, RateMode, Time, TokenKey, TokenRef,
        WalletId, U256,
    };
    use policy_transition::action::lending::{
        BorrowAction, BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState,
    };
    use policy_transition::action::{ActionBody, ActionMeta, ActionNature};
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
                            provider: policy_state::OracleProvider::Chainlink,
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
