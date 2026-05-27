//! 테이블별 CRUD repository.
//!
//! 각 모듈은 한 테이블 (또는 같은 family 묶음) 의 insert/update/select/delete 와
//! WalletState 의 일부분을 hydrate/dehydrate 하는 함수를 제공.

// 단계적 활성화:
// pub mod wallets;       // wallets + block_heights
// pub mod tokens;        // tokens 글로벌 카탈로그
// pub mod holdings;      // token_holdings
// pub mod approvals;     // approvals_erc20, _set_for_all, _permit2
// pub mod positions;     // positions_lending, _perp, _misc
// pub mod pending;       // pending_txs
// pub mod deltas;        // state_deltas (append-only)
// pub mod global_live;   // global_live_fields
