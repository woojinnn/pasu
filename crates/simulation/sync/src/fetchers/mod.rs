//! DataSource 별 fetcher 구현.
//!
//! 공통 trait `Fetcher` 를 두고, 각 종류 (Onchain/Oracle/Venue/Registry) 마다 impl.
//! 같은 source 의 여러 LiveField 는 batcher 가 모아 한 번에 처리.

pub mod decoder;
pub mod onchain;
pub mod oracle;
pub mod registry;
pub mod rpc;
pub mod venue;

pub use decoder::DecoderRegistry;
pub use onchain::{OnchainCall, OnchainOutcome, OnchainViewFetcher};
pub use oracle::{ChainlinkFeed, ChainlinkFeedRegistry, ChainlinkFetcher};
pub use registry::RegistryFetcher;
pub use venue::HyperliquidFetcher;
