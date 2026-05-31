//! End-to-end 시나리오 — 사용자 1명이 wallet 1개에 USDC 잔고 + 라이브 borrow
//! delta 를 INSERT 한 후 다시 읽어 비교.
//!
//! Phase 1 schema 전체 (`user_profile` / `wallets` / `tokens` / `token_holdings` /
//! `state_deltas`) 가 한 트랜잭션에서 동작하는지 검증.

use alloy_primitives::{Address, U256};
use std::str::FromStr;

use simulation_db::repositories::deltas::{DeltaInsert, DeltaSource, DeltaStatus};
use simulation_db::repositories::execution_reports::{
    ExecutionReportInsert, OutcomeKind, ReportStage,
};
use simulation_db::repositories::profile::UserProfile;
use simulation_db::repositories::wallets::WalletInsert;
use simulation_db::repositories::{deltas, execution_reports, holdings, profile, tokens, wallets};
use simulation_db::{current_version, run_migrations, Pool};
use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
use simulation_state::primitives::{ChainId, Duration, Price, Time};
use simulation_state::token::{
    Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind,
};

fn usdc() -> TokenKey {
    TokenKey::Erc20 {
        chain: ChainId::ethereum_mainnet(),
        address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
    }
}

fn build_holding() -> TokenHolding {
    TokenHolding {
        key: usdc(),
        kind: TokenKind::Base {
            category: BaseCategory::Stable,
            peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
        },
        symbol: "USDC".into(),
        decimals: 6,
        balance: Balance::Fungible {
            amount: U256::from(2_500_000_000u64), // 2500 USDC
        },
        committed: Balance::Fungible { amount: U256::ZERO },
        approved_to: None,
        price_usd: Some(
            LiveField::new(
                Price::new("0.99955"),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "USDC/USD".into(),
                },
                Time::from_unix(1_738_000_000),
            )
            .with_ttl(Duration::from_secs(12)),
        ),
        metadata: None,
        value_usd: None,
        last_synced_at: Time::from_unix(1_738_000_000),
        primitives_source: DataSource::UserSupplied,
    }
}

#[test]
fn full_user_journey_in_memory() {
    let pool = Pool::open_in_memory();
    run_migrations(&pool).unwrap();

    // 모든 작업을 한 트랜잭션 안에서 진행 — Cedar 평가 atomicity 시뮬레이션.
    pool.with_tx(|tx| {
        // [1] 사용자 프로필 등록 (OAuth 로그인 시점에 발생할 작업).
        profile::upsert(
            tx,
            &UserProfile {
                user_id: "github:alice".into(),
                email: Some("alice@example.com".into()),
                display_name: Some("Alice".into()),
                settings: serde_json::json!({"retention_days": 90}),
                created_at: 1_700_000_000,
            },
        )?;

        // [2] 본인 지갑 추가.
        let wallet_id = wallets::insert(
            tx,
            &WalletInsert {
                address: "0xowner".into(),
                label: Some("main".into()),
                is_owned: true,
                created_at: 1_700_000_000,
                chains: vec![ChainId::ethereum_mainnet()],
            },
        )?;

        // [3] 토큰 카탈로그 등록 + 잔고 기록.
        tokens::upsert(tx, &usdc(), Some("USDC"), Some(6), 1_700_000_000)?;
        let holding = build_holding();
        holdings::upsert(tx, wallet_id, &holding)?;

        // [4] 라이브 borrow tx 의 시뮬레이션 (predicted) 단계 기록.
        let pred_id = deltas::insert(
            tx,
            &DeltaInsert {
                wallet_id,
                source: DeltaSource::Live,
                status: DeltaStatus::Predicted,
                created_at: 1_738_390_400,
                signed_at: None,
                confirmed_at: None,
                action_domain: "lending".into(),
                action_kind: "borrow".into(),
                submitter: "0xowner".into(),
                nature_kind: "onchain_tx".into(),
                chain: Some("eip155:1".into()),
                nonce: Some(47),
                action_json: serde_json::json!({
                    "meta": {"submitted_at": 1738390400, "submitter": "0xowner"},
                    "body": {"domain": "lending", "kind": "borrow", "amount": "500000000"}
                }),
                predicted_delta_json: Some(serde_json::json!({
                    "tokens": {"USDC": "+500000000"},
                    "positions": {"aave_v3_usdc": {"hf": "2.4→1.92"}}
                })),
                predicted_verdict: Some("allow".into()),
                predicted_verdict_reasons_json: None,
                tx_hash: None,
                sig_hash: None,
                realized_block_number: None,
                realized_delta_json: None,
            },
        )?;

        // [5] 사용자가 서명 → 멤풀에.
        deltas::mark_pending(tx, pred_id, 1_738_390_410, "0xabc123")?;

        // [6] 블록 확정 → confirmed + state 테이블 변경.
        deltas::mark_confirmed(
            tx,
            pred_id,
            1_738_390_430,
            25_197_950,
            &serde_json::json!({"tokens": {"USDC": "+500000000"}}),
        )?;
        // 동시에 holdings.balance 갱신 — 실제 paipeline 에서는 reducer 의 delta
        // 적용 결과를 그대로 UPDATE.
        let mut new_holding = holding.clone();
        new_holding.balance = Balance::Fungible {
            amount: U256::from(3_000_000_000u64), // 2500 + 500
        };
        holdings::upsert(tx, wallet_id, &new_holding)?;

        Ok(())
    })
    .unwrap();

    // 두 번째 트랜잭션에서 모두 다시 읽어 검증.
    pool.with_tx(|tx| {
        let p = profile::get(tx)?.expect("profile");
        assert_eq!(p.user_id, "github:alice");

        let walls = wallets::list_active(tx)?;
        assert_eq!(walls.len(), 1);
        let wallet_id = walls[0].id;

        let raw = holdings::raw_list_for_wallet(tx, wallet_id)?;
        assert_eq!(raw.len(), 1);
        let restored = raw
            .into_iter()
            .next()
            .unwrap()
            .into_holding(TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            })?;
        // 잔고가 새 값으로 갱신됐는지.
        assert_eq!(
            restored.balance,
            Balance::Fungible {
                amount: U256::from(3_000_000_000u64)
            }
        );

        let rows = deltas::list_recent(tx, wallet_id, 10)?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "confirmed");
        assert_eq!(rows[0].tx_hash.as_deref(), Some("0xabc123"));
        assert!(rows[0].realized_delta_json.is_some());

        let counts = deltas::count_by_status(tx, wallet_id)?;
        let confirmed = counts
            .iter()
            .find(|(s, _)| s == "confirmed")
            .map(|(_, n)| *n);
        assert_eq!(confirmed, Some(1));
        Ok(())
    })
    .unwrap();
}

#[test]
fn separate_users_have_separate_files() {
    // 사용자당 1 DB 파일 모델 검증 — 두 user 의 DB 가 서로 안 보임.
    let alice_dir = tempfile::tempdir().unwrap();
    let bob_dir = tempfile::tempdir().unwrap();

    let alice = Pool::open(alice_dir.path().join("scopeball.db")).unwrap();
    let bob = Pool::open(bob_dir.path().join("scopeball.db")).unwrap();
    run_migrations(&alice).unwrap();
    run_migrations(&bob).unwrap();

    alice
        .with_tx(|tx| {
            profile::upsert(
                tx,
                &UserProfile {
                    user_id: "github:alice".into(),
                    email: None,
                    display_name: None,
                    settings: serde_json::json!({}),
                    created_at: 1,
                },
            )?;
            wallets::insert(
                tx,
                &WalletInsert {
                    address: "0xalice".into(),
                    label: None,
                    is_owned: true,
                    created_at: 1,
                    chains: vec![ChainId::ethereum_mainnet()],
                },
            )?;
            Ok(())
        })
        .unwrap();

    bob.with_tx(|tx| {
        profile::upsert(
            tx,
            &UserProfile {
                user_id: "github:bob".into(),
                email: None,
                display_name: None,
                settings: serde_json::json!({}),
                created_at: 1,
            },
        )?;
        wallets::insert(
            tx,
            &WalletInsert {
                address: "0xbob".into(),
                label: None,
                is_owned: true,
                created_at: 1,
                chains: vec![ChainId::ethereum_mainnet()],
            },
        )?;
        Ok(())
    })
    .unwrap();

    // 각자 자기 wallet 만 보임.
    alice
        .with_tx(|tx| {
            let w = wallets::list_active(tx)?;
            assert_eq!(w.len(), 1);
            assert_eq!(w[0].address, "0xalice");
            Ok(())
        })
        .unwrap();
    bob.with_tx(|tx| {
        let w = wallets::list_active(tx)?;
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].address, "0xbob");
        Ok(())
    })
    .unwrap();
}

#[test]
fn execution_reports_round_trip_and_reconcile_by_wallet() {
    let pool = Pool::open_in_memory();
    run_migrations(&pool).unwrap();
    assert_eq!(current_version(&pool).unwrap(), Some(8));

    pool.with_tx(|tx| {
        let wallet_id = wallets::insert(
            tx,
            &WalletInsert {
                address: "0x362e7e9e630481631d7c804dfe50e24b53250925".into(),
                label: Some("hl".into()),
                is_owned: true,
                created_at: 1_700_000_000,
                chains: vec![ChainId::ethereum_mainnet()],
            },
        )?;

        let report_id = execution_reports::insert(
            tx,
            &ExecutionReportInsert {
                wallet_id: Some(wallet_id),
                evaluation_id: Some("eval-hl-1".into()),
                action_index: Some(0),
                stage: ReportStage::Venue,
                outcome_kind: OutcomeKind::VenueAccepted,
                chain: None,
                tx_hash: None,
                signature: None,
                venue: Some("hyperliquid".into()),
                venue_order_id: Some("987654321".into()),
                client_order_id: Some("0xcloid".into()),
                reason: None,
                raw_json: serde_json::json!({"kind":"venue_accepted"}),
                metadata_json: serde_json::json!({"source":"fetch-hook"}),
                created_at: 1_738_000_000,
            },
        )?;
        assert!(report_id > 0);

        // Hyperliquid agent-key requests can be observed before the extension
        // knows the master wallet. Keep those audit facts, but do not reconcile
        // them against a wallet snapshot until they are attributed.
        execution_reports::insert(
            tx,
            &ExecutionReportInsert {
                wallet_id: None,
                evaluation_id: Some("eval-hl-unattributed".into()),
                action_index: None,
                stage: ReportStage::Venue,
                outcome_kind: OutcomeKind::VenueRejected,
                chain: None,
                tx_hash: None,
                signature: None,
                venue: Some("hyperliquid".into()),
                venue_order_id: None,
                client_order_id: None,
                reason: Some("bad signature".into()),
                raw_json: serde_json::json!({"kind":"venue_rejected"}),
                metadata_json: serde_json::json!({}),
                created_at: 1_738_000_001,
            },
        )?;
        // Simulates a report that arrives during the same scheduler tick after
        // the authoritative snapshot time. It must remain unreconciled until a
        // later sync can actually observe it.
        execution_reports::insert(
            tx,
            &ExecutionReportInsert {
                wallet_id: Some(wallet_id),
                evaluation_id: Some("eval-hl-raced".into()),
                action_index: Some(1),
                stage: ReportStage::Venue,
                outcome_kind: OutcomeKind::VenueAccepted,
                chain: None,
                tx_hash: None,
                signature: None,
                venue: Some("hyperliquid".into()),
                venue_order_id: Some("987654322".into()),
                client_order_id: None,
                reason: None,
                raw_json: serde_json::json!({"kind":"venue_accepted"}),
                metadata_json: serde_json::json!({"source":"fetch-hook"}),
                created_at: 1_738_000_030,
            },
        )?;

        let pending = execution_reports::list_unreconciled_for_wallet(tx, wallet_id)?;
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].outcome_kind, "venue_accepted");
        assert_eq!(pending[0].venue.as_deref(), Some("hyperliquid"));

        let reconciled =
            execution_reports::mark_reconciled_for_wallet(tx, wallet_id, 1_738_000_030)?;
        assert_eq!(reconciled, 1);
        let still_pending = execution_reports::list_unreconciled_for_wallet(tx, wallet_id)?;
        assert_eq!(still_pending.len(), 1);
        assert_eq!(
            still_pending[0].evaluation_id.as_deref(),
            Some("eval-hl-raced")
        );
        assert_eq!(execution_reports::count_unreconciled(tx)?, 2);
        Ok(())
    })
    .unwrap();
}
