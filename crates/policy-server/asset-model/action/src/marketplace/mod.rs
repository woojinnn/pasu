//! `MarketplaceAction` — NFT-marketplace orders (Seaport).
//!
//! Three sub-actions cover the two user surfaces ScopeBall must analyze:
//! - [`SignOrderAction`] — off-chain EIP-712 order signature (the maker lists /
//!   offers; the key drainer surface — signing away NFTs for ~0 or, via
//!   `*_criteria` items, ANY NFT in a collection).
//! - [`FulfillOrderAction`] — on-chain fulfillment (the taker buys / sells;
//!   Seaport `fulfill*` / `match*`).
//! - [`CancelOrderAction`] — on-chain revocation (`cancel` / `incrementCounter`).
//!
//! New domain (extension-guide axis 1). The `MarketplaceVenue` discriminator
//! keeps it reusable for Blur / LooksRare / X2Y2. Actions carry **no**
//! `LiveField` inputs — the `ActionBody` is a faithful static decode of the
//! order / calldata.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod cancel_order;
pub mod fulfill_order;
pub mod item;
pub mod sign_order;
pub mod venue;

pub use self::cancel_order::*;
pub use self::fulfill_order::*;
pub use self::item::*;
pub use self::sign_order::*;
pub use self::venue::*;

/// User-level NFT-marketplace actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MarketplaceAction {
    /// Sign an off-chain order (maker lists / offers). Seaport `OrderComponents`.
    SignOrder(SignOrderAction),
    /// Fulfill order(s) on-chain (taker). Seaport `fulfill*` / `match*`.
    FulfillOrder(FulfillOrderAction),
    /// Cancel own order(s) on-chain (Seaport `cancel` / `incrementCounter`).
    CancelOrder(CancelOrderAction),
}

impl MarketplaceAction {
    /// The action's serde `action` tag (e.g. `"sign_order"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::SignOrder(_) => "sign_order",
            Self::FulfillOrder(_) => "fulfill_order",
            Self::CancelOrder(_) => "cancel_order",
        }
    }

    /// The venue `name` of the wrapped action. Every marketplace action carries
    /// a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::SignOrder(a) => a.venue.name(),
            Self::FulfillOrder(a) => a.venue.name(),
            Self::CancelOrder(a) => a.venue.name(),
        })
    }
}
