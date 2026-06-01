use policy_state::{DataSource, LiveField, Time, WalletState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldLocation {
    // ───── Wallet side ─────
    /// `tokens[K].price_usd`
    TokenPrice {
        token_key_json: String,
    },
    /// `positions[i].kind.LendingAccount.health_factor`
    LendingHealthFactor {
        position_id: String,
    },
    LendingLtv {
        position_id: String,
    },
    LendingLiquidationThreshold {
        position_id: String,
    },
    /// `positions[i].kind.PerpPosition.mark_price`
    PerpMarkPrice {
        position_id: String,
    },
    PerpLiqPrice {
        position_id: String,
    },
    PerpUnrealizedPnl {
        position_id: String,
    },
    PerpFundingOwed {
        position_id: String,
    },
    PerpLeverage {
        position_id: String,
    },

    // ───── Action side ─────
    /// `Action.body.*.live_inputs.<slot>`
    Action {
        action_index: usize,
        slot: ActionSlot,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionSlot {
    // ───────── Token ─────────
    TokenErc20PermitNonce,
    TokenPermit2SignNonce,

    // ───────── AMM ─────────
    AmmSwapRoute,
    AmmSwapExpectedAmountOut,
    AmmSwapPriceImpactBp,
    AmmSwapGasEstimate,
    AmmAddLiquidityPoolState,
    AmmAddLiquidityCurrentPrice,
    AmmRemoveLiquidityPoolState,
    AmmRemoveLiquidityFeesOwed,
    AmmCollectFeesOwed,
    AmmSignIntentExpectedFillPrice,
    AmmSignIntentCompetingOrders,

    // ───────── Lending ─────────
    LendingSupplyReserveState,
    LendingSupplySupplyApy,
    LendingSupplyATokenPriceUsd,
    LendingSupplyEligibleAsCollat,
    LendingSupplyUserState,
    LendingWithdrawReserveState,
    LendingWithdrawAvailableToWithdraw,
    LendingWithdrawUserState,
    LendingBorrowReserveState,
    LendingBorrowUserState,
    LendingBorrowAssetPriceUsd,
    LendingBorrowCurrentRate,
    LendingBorrowAvailableLiquidity,
    LendingRepayReserveState,
    LendingRepayCurrentDebt,
    LendingRepayUserState,
    LendingSwapRateModeCurrentDebts,
    LendingSwapRateModeRates,
    LendingSetEModeCategoryConfig,
    LendingSetEModeUserState,
    LendingSetCollateralReserveState,
    LendingSetCollateralUserState,
    LendingLiquidateVictimState,
    LendingLiquidateBonus,
    LendingLiquidateDebtAssetPrice,
    LendingLiquidateCollatAssetPrice,

    // ───────── Airdrop ─────────
    AirdropClaimIsStillClaimable,
    AirdropClaimActualAmount,
    AirdropClaimToken,
    AirdropClaimWindow,
    AirdropDelegateCurrentDelegate,
    AirdropDelegateVotingPower,

    // ───────── Launchpad ─────────
    LaunchpadCommitSaleState,
    LaunchpadCommitUserCap,
    LaunchpadCommitUserCommitted,
    LaunchpadCommitExpectedTokenPrice,
    LaunchpadClaimAllocationAllocated,
    LaunchpadClaimAllocationRefundDue,
    LaunchpadClaimAllocationIsClaimable,
    LaunchpadClaimVestedClaimableNow,
    LaunchpadClaimVestedNextUnlock,
    LaunchpadRefundAmount,
    LaunchpadRefundToken,
    LaunchpadWithdrawCommitWithdrawable,
    LaunchpadWithdrawCommitSaleState,

    // ───────── Perp ─────────
    PerpOpenMarkPrice,
    PerpOpenOraclePrice,
    PerpOpenFundingRate,
    PerpOpenAvailableOi,
    PerpOpenMaxLeverage,
    PerpOpenInitialMarginBp,
    PerpOpenMaintenanceBp,
    PerpOpenFeeTakerBp,
    PerpOpenFeeMakerBp,
    PerpOpenUserAccountState,
    PerpCloseMarkPrice,
    PerpCloseUnrealizedPnl,
    PerpCloseFundingAccrued,
    PerpCloseFeeBp,
    PerpIncreaseMarkPrice,
    PerpIncreaseOraclePrice,
    PerpIncreaseFundingRate,
    PerpIncreaseAvailableOi,
    PerpIncreaseMaxLeverage,
    PerpIncreaseInitialMarginBp,
    PerpIncreaseMaintenanceBp,
    PerpIncreaseFeeTakerBp,
    PerpIncreaseFeeMakerBp,
    PerpIncreaseUserAccountState,
    PerpDecreaseMarkPrice,
    PerpDecreaseUnrealizedPnl,
    PerpDecreaseFundingAccrued,
    PerpDecreaseFeeBp,
    PerpAdjustMarginPositionState,
    PerpAdjustMarginFreeMarginAfter,
    PerpChangeLeverageMaxLeverage,
    PerpChangeLeverageAffectedPositions,
    PerpChangeLeverageNewLiqPrices,
    PerpChangeMarginModeAffectedPositions,
    PerpChangeMarginModeReallocation,
    PerpPlaceLimitMarkPrice,
    PerpPlaceLimitBestBidAsk,
    PerpPlaceLimitOpenOrdersCount,
    PerpPlaceLimitUserAccountState,
    PerpPlaceStopMarkPrice,
    PerpPlaceStopUserAccountState,
    PerpClaimFundingClaimable,

    // ───────── Liquid Staking ─────────
    LiquidStakingWrapExpectedWsteth,
    LiquidStakingUnwrapExpectedSteth,
    LiquidStakingTransferSharesPooledEth,
}

#[derive(Clone, Debug)]
pub struct StaleField {
    pub location: FieldLocation,
    pub source: DataSource,
    pub synced_at: Time,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WalkStats {
    pub total_live_fields: usize,
    pub stale_count: usize,
    pub fresh_count: usize,
}

#[must_use]
pub fn walk_stale(state: &WalletState, now: Time) -> (Vec<StaleField>, WalkStats) {
    let mut stale = Vec::new();
    let mut stats = WalkStats::default();

    // 1. tokens — price_usd
    for (key, holding) in &state.tokens {
        if let Some(price) = holding.price_usd.as_ref() {
            stats.total_live_fields += 1;
            if is_field_stale(price, now) {
                stats.stale_count += 1;
                stale.push(StaleField {
                    location: FieldLocation::TokenPrice {
                        token_key_json: serde_json::to_string(key).unwrap_or_default(),
                    },
                    source: price.source.clone(),
                    synced_at: price.synced_at,
                });
            } else {
                stats.fresh_count += 1;
            }
        }
    }

    // 2. positions
    for pos in &state.positions {
        use policy_state::PositionKind;
        match &pos.kind {
            PositionKind::LendingAccount(la) => {
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &la.health_factor,
                    now,
                    FieldLocation::LendingHealthFactor {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &la.ltv,
                    now,
                    FieldLocation::LendingLtv {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &la.liquidation_threshold,
                    now,
                    FieldLocation::LendingLiquidationThreshold {
                        position_id: pos.id.clone(),
                    },
                );
            }
            PositionKind::PerpPosition(p) => {
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &p.mark_price,
                    now,
                    FieldLocation::PerpMarkPrice {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &p.liq_price,
                    now,
                    FieldLocation::PerpLiqPrice {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &p.unrealized_pnl,
                    now,
                    FieldLocation::PerpUnrealizedPnl {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &p.funding_owed,
                    now,
                    FieldLocation::PerpFundingOwed {
                        position_id: pos.id.clone(),
                    },
                );
                check_and_push(
                    &mut stale,
                    &mut stats,
                    &p.leverage,
                    now,
                    FieldLocation::PerpLeverage {
                        position_id: pos.id.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    (stale, stats)
}

fn check_and_push<T>(
    stale: &mut Vec<StaleField>,
    stats: &mut WalkStats,
    field: &LiveField<T>,
    now: Time,
    location: FieldLocation,
) {
    stats.total_live_fields += 1;
    if is_field_stale_generic(field, now) {
        stats.stale_count += 1;
        stale.push(StaleField {
            location,
            source: field.source.clone(),
            synced_at: field.synced_at,
        });
    } else {
        stats.fresh_count += 1;
    }
}

fn is_field_stale(f: &LiveField<policy_state::Price>, now: Time) -> bool {
    f.is_stale(now)
}

/// generic.
fn is_field_stale_generic<T>(f: &LiveField<T>, now: Time) -> bool {
    f.is_stale(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{
        Address, Balance, BaseCategory, ChainId, DataSource, Decimal, Duration, FiatCurrency,
        LiveField, OracleProvider, PegTarget, Time, TokenHolding, TokenKey, TokenKind, WalletId,
        WalletState,
    };
    use std::str::FromStr;

    fn mk_usdc_holding(synced_at: u64) -> (TokenKey, TokenHolding) {
        let usdc_addr = Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
        let key = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdc_addr,
        };
        let holding = TokenHolding {
            key: key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Decimal::new("1.0001"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(synced_at),
                )
                .with_ttl(Duration::from_secs(60)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(synced_at),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: usdc_addr,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        };
        (key, holding)
    }

    #[test]
    fn walks_token_prices() {
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let mut state = WalletState::new(WalletId::new(addr, [ChainId::ethereum_mainnet()]));

        let (k1, h1) = mk_usdc_holding(1_738_000_000);
        state.tokens.insert(k1, h1);

        let usdt_addr = Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
        let k2 = TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: usdt_addr,
        };
        let (_, mut h2) = mk_usdc_holding(1_737_999_000);
        h2.key = k2.clone();
        h2.symbol = "USDT".into();
        state.tokens.insert(k2, h2);

        let (stale, stats) = walk_stale(&state, Time::from_unix(1_738_000_030));
        assert_eq!(stats.total_live_fields, 2);
        assert_eq!(stats.stale_count, 1);
        assert_eq!(stats.fresh_count, 1);
        assert_eq!(stale.len(), 1);

        match &stale[0].location {
            FieldLocation::TokenPrice { token_key_json } => {
                let lower = token_key_json.to_lowercase();
                assert!(
                    lower.contains("dac17"),
                    "expected USDT address in {token_key_json}"
                );
            }
            other => panic!("expected TokenPrice, got {other:?}"),
        }
    }

    #[test]
    fn empty_state_has_no_stale() {
        let addr = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        let state = WalletState::new(WalletId::new(addr, [ChainId::ethereum_mainnet()]));
        let (stale, stats) = walk_stale(&state, Time::from_unix(1_738_000_000));
        assert!(stale.is_empty());
        assert_eq!(stats.total_live_fields, 0);
    }
}
