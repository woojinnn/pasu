//! DataSource 별 fetcher 구현.
//!
//! 공통 trait `Fetcher` 를 두고, 각 종류 (Onchain/Oracle/Venue) 마다 impl.
//! 같은 source 의 여러 LiveField 는 batcher 가 모아 한 번에 처리.

// 단계적 활성화:
// pub mod onchain;       // eth_call (alloy provider 기반)
// pub mod oracle;        // Chainlink, Pyth, Redstone
// pub mod venue;         // Hyperliquid, GMX, dYdX, UniswapX
// pub mod derived;       // DerivedFrom 라우팅 (실 계산은 calc/ 에)
