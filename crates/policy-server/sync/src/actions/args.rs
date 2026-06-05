//! * `LendingBorrowUserState`           → getUserAccountData(user) — submitter
//! * `LendingBorrowReserveState`        → getReserveData(asset) — borrow asset
//!
//! Remaining slots default to empty args, which covers no-arg views and slots
//! that are intentionally wired in a later pass.

use policy_state::WalletState;
use policy_transition::action::amm::{AmmAction, RemoveLiquidityParams};
use policy_transition::action::lending::LendingAction;
use policy_transition::action::liquid_staking::LiquidStakingAction;
use policy_transition::action::token::TokenAction;
use policy_transition::action::{Action, ActionBody};

use crate::fetchers::decoder::{encode_address, encode_u256};
use crate::walker::{ActionSlot, FieldLocation};

#[must_use]
pub fn resolve_args(slot: &ActionSlot, action: &Action, state: &WalletState) -> Vec<u8> {
    let Some(body) = body_at_index(&action.body, 0) else {
        return Vec::new();
    };
    resolve_args_for_body(slot, body, action, state)
}

/// Location-aware resolver for `refresh_action`.
///
/// `ActionSlot` alone is ambiguous for `ActionBody::Multicall`: the same slot can
/// appear on multiple child actions with different calldata-derived arguments.
#[must_use]
pub fn resolve_args_for_location(
    location: &FieldLocation,
    action: &Action,
    state: &WalletState,
) -> Vec<u8> {
    let FieldLocation::Action { action_index, slot } = location else {
        return Vec::new();
    };
    let Some(body) = body_at_index(&action.body, *action_index) else {
        return Vec::new();
    };
    resolve_args_for_body(slot, body, action, state)
}

fn resolve_args_for_body(
    slot: &ActionSlot,
    body: &ActionBody,
    action: &Action,
    _state: &WalletState,
) -> Vec<u8> {
    match slot {
        // ─── Aave Borrow ───
        ActionSlot::LendingBorrowAvailableLiquidity => {
            // balanceOf(pool) — 자산 컨트랙트의 잔고를 pool 이 얼마나 들고 있나
            if let ActionBody::Lending(LendingAction::Borrow(b)) = body {
                if let Some(pool) = lending_venue_pool_address(&b.venue) {
                    return encode_address(pool).to_vec();
                }
            }
            Vec::new()
        }
        ActionSlot::LendingBorrowUserState => encode_address(action.meta.submitter).to_vec(),
        ActionSlot::LendingBorrowReserveState | ActionSlot::LendingBorrowCurrentRate => {
            if let ActionBody::Lending(LendingAction::Borrow(b)) = body {
                if let Some(addr) = token_ref_to_address(&b.asset) {
                    return encode_address(addr).to_vec();
                }
            }
            Vec::new()
        }

        // ─── Permit2 unordered nonce bitmap ───
        // nonceBitmap(owner, word) — the action stores the signed nonce as
        // bitmap coordinates, so only the word is an ABI arg. The bit is checked
        // after the bitmap is fetched.
        ActionSlot::TokenPermit2SignNonce => {
            if let ActionBody::Token(TokenAction::Permit2SignAllowance(p)) = body {
                let mut out = Vec::with_capacity(64);
                out.extend_from_slice(&encode_address(action.meta.submitter));
                out.extend_from_slice(&encode_u256(p.nonce.value.0));
                return out;
            }
            if let ActionBody::Token(TokenAction::Permit2SignTransfer(p)) = body {
                let mut out = Vec::with_capacity(64);
                out.extend_from_slice(&encode_address(p.owner));
                out.extend_from_slice(&encode_u256(p.nonce.value.0));
                return out;
            }
            if let ActionBody::Token(TokenAction::Permit2TransferFrom(p)) = body {
                let mut out = Vec::with_capacity(64);
                out.extend_from_slice(&encode_address(p.owner));
                out.extend_from_slice(&encode_u256(p.nonce.value.0));
                return out;
            }
            Vec::new()
        }

        // ─── Uniswap V3 position fees ───
        // positions(tokenId) — tokenId comes from the calldata-derived NFT key.
        ActionSlot::AmmRemoveLiquidityFeesOwed => {
            if let ActionBody::Amm(AmmAction::RemoveLiquidity(r)) = body {
                let token_id = match &r.params {
                    RemoveLiquidityParams::ConcentratedDecrease { nft_key, .. }
                    | RemoveLiquidityParams::ConcentratedBurn { nft_key } => nft_key.token_id(),
                    _ => None,
                };
                if let Some(token_id) = token_id {
                    return encode_u256(*token_id).to_vec();
                }
            }
            Vec::new()
        }
        ActionSlot::AmmCollectFeesOwed => {
            if let ActionBody::Amm(AmmAction::CollectFees(c)) = body {
                if let Some(token_id) = c.nft_key.token_id() {
                    return encode_u256(*token_id).to_vec();
                }
            }
            Vec::new()
        }

        // ─── Aave Supply (같은 패턴, 후속에서 wire-up) ───
        // ActionSlot::LendingSupplyReserveState  → getReserveData(asset)
        // ActionSlot::LendingSupplyUserState     → getUserAccountData(user)

        // Lido liquid staking conversion views.
        ActionSlot::LiquidStakingWrapExpectedWsteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Wrap(w)) = body {
                return encode_u256(w.amount).to_vec();
            }
            Vec::new()
        }
        ActionSlot::LiquidStakingUnwrapExpectedSteth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::Unwrap(u)) = body {
                return encode_u256(u.amount).to_vec();
            }
            Vec::new()
        }
        ActionSlot::LiquidStakingTransferSharesPooledEth => {
            if let ActionBody::LiquidStaking(LiquidStakingAction::TransferShares(t)) = body {
                return encode_u256(t.shares).to_vec();
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn body_at_index(body: &ActionBody, index: usize) -> Option<&ActionBody> {
    match body {
        ActionBody::Multicall { actions } => actions.get(index),
        single if index == 0 => Some(single),
        _ => None,
    }
}

/// `LendingVenue` 의 pool 주소 추출 (Aave/Compound/Morpho/...).
const fn lending_venue_pool_address(
    venue: &policy_transition::action::lending::LendingVenue,
) -> Option<policy_state::Address> {
    use policy_transition::action::lending::LendingVenue::{
        AaveV2, AaveV3, AaveV3Periphery, CompoundV2, CompoundV3, CrvUsd, Fluid, LlamaLend,
        MetaMorpho, MorphoBlue, MorphoOptimizer, Spark,
    };
    match venue {
        AaveV3 { pool, .. } | AaveV2 { pool, .. } | Spark { pool, .. } => Some(*pool),
        CompoundV3 { comet, .. } => Some(*comet),
        CompoundV2 { comptroller, .. } => Some(*comptroller),
        MorphoOptimizer { vault, .. } | Fluid { vault, .. } | MetaMorpho { vault, .. } => {
            Some(*vault)
        }
        // crvUSD and LlamaLend use one controller address per market.
        CrvUsd { controller, .. } | LlamaLend { controller, .. } => Some(*controller),
        AaveV3Periphery { adapter, .. } => Some(*adapter),
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
    use policy_transition::action::amm::{
        AmmAction, AmmVenue, CollectFeesAction, CollectFeesLiveInputs,
    };
    use policy_transition::action::lending::{
        BorrowAction, BorrowLiveInputs, LendingVenue, ReserveState, UserLendingState,
    };
    use policy_transition::action::token::TokenAction;
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

    fn mk_action(body: ActionBody, submitter: Address) -> Action {
        Action {
            meta: ActionMeta {
                submitted_at: Time::from_unix(0),
                submitter,
                nature: ActionNature::OnchainTx {
                    chain: ChainId::ethereum_mainnet(),
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
            body,
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

    #[test]
    fn args_for_permit2_nonce_bitmap_are_owner_and_signed_word() {
        let owner = Address::from_str("0xd8da6bf26964af9d7eed9e03e53415d37aa96045").unwrap();
        let nonce_word = U256::from(7u64);
        let action = mk_action(
            ActionBody::Token(TokenAction::Permit2SignAllowance(
                policy_transition::action::token::Permit2SignAction {
                    token: TokenRef::new(TokenKey::Erc20 {
                        chain: ChainId::ethereum_mainnet(),
                        address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
                            .unwrap(),
                    }),
                    spender: Address::from_str("0x00000000000000000000000000000000deadbeef")
                        .unwrap(),
                    amount: U256::from(1000u64),
                    expires_at: Time::from_unix(1_800_000_000),
                    sig_deadline: Time::from_unix(1_800_000_100),
                    nonce: LiveField::new(
                        (nonce_word, 42),
                        DataSource::UserSupplied,
                        Time::from_unix(0),
                    ),
                },
            )),
            owner,
        );

        let args = resolve_args_for_location(
            &FieldLocation::Action {
                action_index: 0,
                slot: ActionSlot::TokenPermit2SignNonce,
            },
            &action,
            &dummy_state(),
        );

        assert_eq!(args.len(), 64);
        assert_eq!(&args[12..32], owner.as_slice());
        assert_eq!(&args[32..64], &nonce_word.to_be_bytes::<32>());
    }

    #[test]
    fn args_for_collect_fees_are_position_token_id() {
        let token_id = U256::from(12345u64);
        let nfpm = Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap();
        let action = mk_action(
            ActionBody::Amm(AmmAction::CollectFees(CollectFeesAction {
                venue: AmmVenue::UniswapV3 {
                    chain: ChainId::ethereum_mainnet(),
                    pool: Address::from_str("0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8").unwrap(),
                    fee_tier_bp: 3000,
                },
                nft_key: TokenKey::Erc721 {
                    chain: ChainId::ethereum_mainnet(),
                    contract: nfpm,
                    token_id,
                },
                recipient: Address::ZERO,
                live_inputs: CollectFeesLiveInputs {
                    fees_owed: LiveField::new(
                        Vec::new(),
                        DataSource::UserSupplied,
                        Time::from_unix(0),
                    ),
                },
            })),
            Address::ZERO,
        );

        let args = resolve_args_for_location(
            &FieldLocation::Action {
                action_index: 0,
                slot: ActionSlot::AmmCollectFeesOwed,
            },
            &action,
            &dummy_state(),
        );

        assert_eq!(args.len(), 32);
        assert_eq!(&args[..], &token_id.to_be_bytes::<32>());
    }

    #[test]
    fn args_for_multicall_child_use_child_action_index() {
        let first_id = U256::from(1u64);
        let second_id = U256::from(2u64);
        let nfpm = Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap();
        let collect = |token_id| {
            ActionBody::Amm(AmmAction::CollectFees(CollectFeesAction {
                venue: AmmVenue::UniswapV3 {
                    chain: ChainId::ethereum_mainnet(),
                    pool: Address::ZERO,
                    fee_tier_bp: 3000,
                },
                nft_key: TokenKey::Erc721 {
                    chain: ChainId::ethereum_mainnet(),
                    contract: nfpm,
                    token_id,
                },
                recipient: Address::ZERO,
                live_inputs: CollectFeesLiveInputs {
                    fees_owed: LiveField::new(
                        Vec::new(),
                        DataSource::UserSupplied,
                        Time::from_unix(0),
                    ),
                },
            }))
        };
        let action = mk_action(
            ActionBody::Multicall {
                actions: vec![collect(first_id), collect(second_id)],
            },
            Address::ZERO,
        );

        let args = resolve_args_for_location(
            &FieldLocation::Action {
                action_index: 1,
                slot: ActionSlot::AmmCollectFeesOwed,
            },
            &action,
            &dummy_state(),
        );

        assert_eq!(&args[..], &second_id.to_be_bytes::<32>());
    }
}
