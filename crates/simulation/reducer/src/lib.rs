//! `simulation-reducer` — Action 을 받아 WalletState 를 갱신하는 pure function.
//!
//! 외부 IO 없음 (DB / RPC / 시계 안 부름). 입력은 `state` + `action` + `eval`,
//! 출력은 `(newState, StateDelta)`. wasm 빌드 가능.
//!
//! 구조:
//! * [`apply`] / [`dispatch`]  — 진입점
//! * [`helpers`]               — debit/credit/upsert_position 같은 변경 헬퍼
//! * [`strategy`]              — protocol 별로 갈아끼우는 trait (SwapStrategy 등)
//! * [`reducers`]              — action × category × protocol 별 구현
//!
//! 시작 권장 순서:
//! 1. `reducers/misc/approve.rs`          (가장 단순 — 패턴 잡기)
//! 2. `reducers/dex/swap.rs` + UniswapV3   (strategy 패턴 확정)
//! 3. `reducers/lending/supply.rs` + AaveV3 (token + position + DerivedFrom 다층 케이스)

pub mod error;

// 단계적 활성화. 각 모듈은 비어있는 상태로 시작.
// pub mod apply;
// pub mod dispatch;
// pub mod helpers;
// pub mod strategy;
// pub mod reducers;
