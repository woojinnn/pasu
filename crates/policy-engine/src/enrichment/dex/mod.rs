//! Per-action enrichment handlers for DEX actions.
//!
//! Each submodule provides an `impl Enrich for <Action>` so the dispatcher in
//! [`crate::enrichment::dispatch`] can call `action.enrich(...)` uniformly.

mod add_liquidity;
mod burn_liquidity_nft;
mod decrease_liquidity;
mod increase_liquidity;
mod mint_liquidity_nft;
mod remove_liquidity;
mod swap;
