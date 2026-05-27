//! `simulation-sync` — LiveField 갱신을 담당하는 Sync Orchestrator.
//!
//! state crate 의 LiveField 들을 외부 (RPC/Oracle/Venue API) 에서 가져와
//! `value` / `synced_at` / `confidence` 를 in-place 로 갱신한다.
//! `source` 와 `ttl` 은 불변.
//!
//! 동작 흐름:
//! 1. [`walker`] 가 state 를 traverse 하면서 모든 LiveField 수집
//! 2. [`batcher`] 가 같은 DataSource 끼리 묶음
//! 3. [`fetchers`] 가 batch 호출 → 결과 받음
//! 4. [`topo`] 가 DerivedFrom 의 의존 그래프 위상정렬
//! 5. [`calc`] 가 DerivedFrom 의 calc 함수로 계산
//! 6. simulation-db 로 결과 write back
//!
//! reducer 와 달리 외부 IO 가 있으므로 native only — wasm 빌드 안 됨.

pub mod error;

// 단계적 활성화:
// pub mod orchestrator;   // sync_wallet, sync_global, refresh_for_action
// pub mod walker;         // state walk → stale LiveField 수집
// pub mod batcher;        // 같은 DataSource 묶기
// pub mod topo;           // DerivedFrom 위상정렬
// pub mod scheduler;      // ttl 기반 주기적 refresh
// pub mod fetchers;       // OnchainView / OracleFeed / VenueApi / DerivedFrom
// pub mod calc;           // DerivedFrom 계산 함수 (HF, PnL, liq_price)
