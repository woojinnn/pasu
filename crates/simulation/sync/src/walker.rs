//! State walker — `WalletState` 를 traverse 하면서 LiveField 위치/소스를 모두 수집.
//!
//! 각 LiveField 는 _location_ (어디 박혀있는지) + _source_ (어디서 가져오나) + _staleness_
//! 정보를 들고 있다. orchestrator 가 이 walker 결과 → batcher 로 묶어 → fetcher 로 fetch.
//!
//! "stale" 판정은 `LiveField.is_stale(now)` 활용. ttl 없으면 안전하게 stale 로 봄
//! (한 번도 sync 안 된 거 포함).

use simulation_state::{DataSource, LiveField, Time, WalletState};

/// LiveField 가 어디에 있는지의 경로.
///
/// 두 종류:
/// * `Wallet*` — `WalletState` 안의 LiveField (sync 주기/event-trigger 로 갱신)
/// * `Action { ix, slot }` — `Action.body.*.live_inputs` 안의 LiveField
///   (정책 평가 직전 [`Orchestrator::refresh_action`] 으로 갱신)
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
    /// `action_index` 는 `Multicall` 안의 자식 위치 (단일 액션이면 0).
    Action {
        action_index: usize,
        slot: ActionSlot,
    },
}

/// `Action.body.*.live_inputs` 안의 슬롯 식별자.
///
/// 점진적으로 채워짐 — 지금은 lending borrow / supply 의 5+5 슬롯만.
/// 새 액션 wire-up 시 variant 추가 (컴파일러가 walk/apply 양쪽의 match 누락을 잡음).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionSlot {
    // ── Lending Borrow ──
    LendingBorrowReserveState,
    LendingBorrowUserState,
    LendingBorrowAssetPriceUsd,
    LendingBorrowCurrentRate,
    LendingBorrowAvailableLiquidity,

    // ── Lending Supply ──
    LendingSupplyReserveState,
    LendingSupplySupplyApy,
    LendingSupplyATokenPriceUsd,
    LendingSupplyEligibleAsCollat,
    LendingSupplyUserState,
}

#[derive(Clone, Debug)]
pub struct StaleField {
    pub location: FieldLocation,
    pub source: DataSource,
    /// 마지막 sync 시각 (디버깅용).
    pub synced_at: Time,
}

/// 한 walk 의 결과 통계.
#[derive(Debug, Default, Clone, Copy)]
pub struct WalkStats {
    pub total_live_fields: usize,
    pub stale_count: usize,
    pub fresh_count: usize,
}

/// `state` 안의 모든 LiveField 를 수집. 이 중 stale 한 것만 반환.
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
        use simulation_state::PositionKind;
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
            // Airdrop / Launchpad / Vesting 은 LiveField 없음
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

/// 가격 (price) 같은 구체 타입의 LiveField.
fn is_field_stale(f: &LiveField<simulation_state::Price>, now: Time) -> bool {
    f.is_stale(now)
}

/// generic.
fn is_field_stale_generic<T>(f: &LiveField<T>, now: Time) -> bool {
    f.is_stale(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::{
        Address, Balance, BaseCategory, ChainId, DataSource, Decimal, Duration, FiatCurrency,
        LiveField, OracleProvider, PegTarget, Time, TokenHolding, TokenKey, TokenKind, WalletId,
        WalletState,
    };
    use std::str::FromStr;

    fn mk_usdc_holding(synced_at: u64) -> (TokenKey, TokenHolding) {
        let usdc_addr =
            Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap();
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

        // 신선 (synced 30s 전, ttl 60s)
        let (k1, h1) = mk_usdc_holding(1_738_000_000);
        state.tokens.insert(k1, h1);

        // stale (synced 1000s 전, ttl 60s)
        let usdt_addr =
            Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap();
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
                // alloy Address 의 serde 직렬화 결과 (checksum or lower 대소문자) 모두 허용.
                let lower = token_key_json.to_lowercase();
                assert!(
                    lower.contains("dac17"),
                    "expected USDT address in {}",
                    token_key_json
                );
            }
            other => panic!("expected TokenPrice, got {:?}", other),
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
