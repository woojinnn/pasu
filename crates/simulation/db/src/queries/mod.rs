//! 정책 평가 / Sync Orchestrator 가 자주 쓰는 read 쿼리.
//!
//! repository 의 단일 row CRUD 보다 위 레이어 — JOIN / WHERE 가 들어가는 view.

// 단계적 활성화:
// pub mod holdings;     // "이 토큰 보유한 모든 wallet", "USDC 잔고 > X"
// pub mod approvals;    // "unlimited approve 목록", "expired permit2"
// pub mod positions;    // "HF < 1.5", "perp leverage > 5x"
// pub mod pending;      // "active orders by market", "expiring within N hours"
// pub mod stale;        // Sync 가 stale LiveField walk — partial index 활용
