//! Category 별 reducer 구현 디렉토리.
//!
//! 각 sub-디렉토리는 한 카테고리의 action 들을 담는다. action 당 1 파일,
//! protocol 별 strategy impl 은 같은 카테고리 폴더 안에 protocol 명 파일로.

// pub mod dex;        // swap, mint_lp, add/remove/increase/decrease_liquidity + V2/V3/V4/Curve/Balancer/LB
// pub mod lending;    // supply, withdraw, borrow, repay, liquidate, flash_loan + Aave/Compound/Morpho
// pub mod perp;       // open/close/modify, place_order + GMX/Hyperliquid bridge/dYdX bridge
// pub mod airdrop;    // claim, delegate_claim, stake_claim
// pub mod launchpad;  // commit, claim, refund, vest
// pub mod misc;       // approve, transfer, permit, wrap_unwrap, eip7702
