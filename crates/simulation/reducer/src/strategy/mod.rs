//! Strategy trait 들 — protocol 별로 갈아끼우는 작은 인터페이스.
//!
//! 한 action (예: swap) 의 skeleton 은 1개지만, protocol (V2/V3/V4/Curve/...)
//! 별로 fee 계산·output 추정·extras 처리가 다르다. 그 차이만 trait 으로 추출.

// 단계적 활성화:
// pub mod swap;        // SwapStrategy — fee_bps, estimate_output, apply_extras
// pub mod supply;      // SupplyStrategy — aToken mint, share/asset ratio
// pub mod borrow;      // BorrowStrategy
// pub mod lp_mint;     // LpMintStrategy — V3/V4 NFT mint, V2 fungible mint
// pub mod perp;        // PerpStrategy — open/close/modify
