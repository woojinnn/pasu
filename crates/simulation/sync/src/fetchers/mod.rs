//! DataSource 별 fetcher 구현.
//!
//! 공통 trait `Fetcher` 를 두고, 각 종류 (Onchain/Oracle/Venue/Registry) 마다 impl.
//! 같은 source 의 여러 LiveField 는 batcher 가 모아 한 번에 처리.

pub mod rpc;

// 단계적 활성화:
// pub mod onchain;       // OnchainView — RPC eth_call (Phase 2)
// pub mod oracle;        // Chainlink, Pyth, Redstone   (Phase 5)
// pub mod venue;         // Hyperliquid, GMX, dYdX      (Phase 8)
// pub mod registry;      // scopeball registry server   (Phase 6)
// pub mod derived;       // DerivedFrom 라우팅          (Phase 7)
