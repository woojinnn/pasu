//! Aggregator crate: turns the directory of internal adapter crates
//! (`crates/adapters/*`) into a single `MockTransactionActionAdapterRegistry` that pipeline
//! tests/examples can use as a drop-in "virtual registry".
//!
//! Adding a new internal adapter is a two-step:
//! 1. Create a new crate under `crates/adapters/<name>/` and add it to the
//!    workspace `members` list.
//! 2. Add a `policy-engine-adapter-<name>` dependency here and one
//!    `.with_adapter(...)` line in [`default_registry`].

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

use policy_engine::{
    DeclaredTransactionActionAdapter, MockSignatureActionAdapterRegistry,
    MockTransactionActionAdapterRegistry,
};
use std::sync::Arc;

// All first-party adapter crates are re-exported under their module name so
// downstream code (tests, examples) doesn't need to depend on each one.
pub mod eip2612;
pub mod permit2;
pub mod shim;
pub mod uniswap_v2;
pub mod uniswap_v3;
pub mod universal_router;

/// Build a `MockTransactionActionAdapterRegistry` populated with every first-party swap
/// adapter shipped in this workspace.
///
/// Tests or examples that want all known adapters installed should call this.
/// Tests that want a narrower setup should build the registry by hand.
#[must_use]
pub fn default_registry() -> MockTransactionActionAdapterRegistry {
    MockTransactionActionAdapterRegistry::new()
        // Uniswap Universal Router
        .with_factory(universal_router::Adapter_::factory())
        // Uniswap V3 SwapRouter
        .with_factory(uniswap_v3::UniswapV3ExactInputSingleAdapter::factory())
        .with_adapter(Arc::new(uniswap_v3::UniswapV3ExactInputAdapter::new()))
        .with_adapter(Arc::new(
            uniswap_v3::UniswapV3ExactOutputSingleAdapter::new(),
        ))
        .with_adapter(Arc::new(uniswap_v3::UniswapV3ExactOutputAdapter::new()))
        .with_adapter(Arc::new(uniswap_v3::UniswapV3MulticallAdapter::new()))
        // Uniswap V2 Router02
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapExactTokensForTokensAdapter::new(),
        ))
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapTokensForExactTokensAdapter::new(),
        ))
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapExactETHForTokensAdapter::new(),
        ))
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapETHForExactTokensAdapter::new(),
        ))
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapExactTokensForETHAdapter::new(),
        ))
        .with_adapter(Arc::new(
            uniswap_v2::UniswapV2SwapTokensForExactETHAdapter::new(),
        ))
}

/// Build a `MockSignatureActionAdapterRegistry` populated with first-party signature
/// adapters.
#[must_use]
pub fn default_signature_registry() -> MockSignatureActionAdapterRegistry {
    MockSignatureActionAdapterRegistry::new()
        .with_adapter(Arc::new(permit2::Permit2Adapter::new()))
        .with_adapter(Arc::new(eip2612::Eip2612Adapter::new()))
}
