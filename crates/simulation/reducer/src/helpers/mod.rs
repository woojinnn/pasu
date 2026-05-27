//! Reducer 들이 공유하는 state 변경 헬퍼.
//!
//! 모든 reducer 는 직접 state 필드를 수정하지 않고 이 헬퍼를 통해 변경한다 —
//! 변경 사실이 자동으로 StateDelta 에 누적되어, 호출자가 "무엇이 어떻게 바뀌었는지"
//! 일관되게 받아볼 수 있다.

// 단계적 활성화:
// pub mod amount;       // AmountConstraint → 정확량/Range 변환
// pub mod approval;     // set_allowance, revoke, upsert_permit2
// pub mod balance;      // debit, credit
// pub mod derived;      // recompute_hf, recompute_perp_pnl (DerivedFrom LiveField)
// pub mod pending;      // add_pending, remove_pending, recompute_committed
// pub mod position;     // upsert_position, remove_position
