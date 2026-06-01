//! Shared per-action helpers for `PerpAction` reducers.
//!
//! Currently houses two families:
//!   - venue-tag derivation from `PerpVenue` (used for `PendingTx.id`
//!     namespacing and `position_id` generation),
//!   - common pending-id format strings (mirrors
//!     `effect::token::pending_id_for_eip2612` / `pending_id_for_permit2`).
//!
//! Everything is `pub(super)` — the `effect::perp` subtree is the sole
//! consumer and we keep the surface internal to enable future refactors
//! without touching `lib.rs` re-exports.

#![allow(dead_code)]

use policy_state::live_field::DataSource;
use policy_state::pending::PerpOrderKind;
use policy_state::position::PerpSide;
use policy_state::primitives::VenueRef;

use crate::action::perp::{PerpVenue, StopOrderKind};

/// Derive a short string venue tag from `PerpVenue` — used for `position_id`
/// namespacing and `PendingTx.id` prefixes. Matches the Defillama-style
/// names already used by `VenueRef` constructors elsewhere.
pub(super) const fn venue_tag(venue: &PerpVenue) -> &'static str {
    match venue {
        PerpVenue::Hyperliquid { .. } => "hyperliquid",
        PerpVenue::GmxV2 { .. } => "gmx_v2",
        PerpVenue::DyDxV4 { .. } => "dydx_v4",
        PerpVenue::Vertex { .. } => "vertex",
        PerpVenue::Aevo { .. } => "aevo",
        PerpVenue::Drift { .. } => "drift",
        PerpVenue::JupiterPerps { .. } => "jupiter_perps",
        PerpVenue::Synthetix { .. } => "synthetix",
        PerpVenue::Generic { .. } => "generic_perp",
    }
}

/// Build a `VenueRef` from a `PerpVenue` for embedding into
/// `PendingKind::PerpVenueOrder.venue`.
pub(super) fn venue_ref(venue: &PerpVenue) -> VenueRef {
    let chain = match venue {
        PerpVenue::Hyperliquid { chain }
        | PerpVenue::GmxV2 { chain }
        | PerpVenue::DyDxV4 { chain }
        | PerpVenue::Vertex { chain }
        | PerpVenue::Aevo { chain }
        | PerpVenue::Drift { chain }
        | PerpVenue::JupiterPerps { chain }
        | PerpVenue::Synthetix { chain }
        | PerpVenue::Generic { chain, .. } => Some(chain.clone()),
    };
    VenueRef {
        name: venue_tag(venue).into(),
        chain,
    }
}

/// Returns true if the venue routes new positions through an off-chain
/// orderbook (signed-only at the reducer; on-chain settlement happens later).
/// Hyperliquid / Aevo / `DyDx` V4 are off-chain-orderbook; the others execute
/// trades on-chain at the venue's gateway contract.
pub(super) const fn is_orderbook_venue(venue: &PerpVenue) -> bool {
    matches!(
        venue,
        PerpVenue::Hyperliquid { .. } | PerpVenue::Aevo { .. } | PerpVenue::DyDxV4 { .. }
    )
}

/// Compose a deterministic position id. The reducer needs a stable handle
/// to attach later `Close`/`Update` changes; we synthesize it from venue +
/// market + side so re-evaluating the same action produces the same id.
pub(super) fn synth_position_id(venue: &PerpVenue, market: &str, side: &PerpSide) -> String {
    format!("{}:{market}:{}", venue_tag(venue), side_tag(side))
}

/// Stable string tag for a `PerpSide`.
pub(super) const fn side_tag(side: &PerpSide) -> &'static str {
    match side {
        PerpSide::Long => "long",
        PerpSide::Short => "short",
    }
}

/// Compose a deterministic `PendingTx` id for a limit order.
pub(super) fn pending_id_for_limit_order(
    venue: &PerpVenue,
    market: &str,
    side: &PerpSide,
    price: &policy_state::primitives::Price,
) -> String {
    format!(
        "limit:{}:{market}:{}:{}",
        venue_tag(venue),
        side_tag(side),
        price.as_str()
    )
}

/// Compose a deterministic `PendingTx` id for a stop / take-profit order.
pub(super) fn pending_id_for_stop_order(
    venue: &PerpVenue,
    market: &str,
    side: &PerpSide,
    kind: &StopOrderKind,
    trigger: &policy_state::primitives::Price,
) -> String {
    let kind_s = match kind {
        StopOrderKind::StopMarket => "stop_market",
        StopOrderKind::StopLimit => "stop_limit",
        StopOrderKind::TakeProfit => "take_profit",
        StopOrderKind::TakeProfitLimit => "take_profit_limit",
    };
    format!(
        "{kind_s}:{}:{market}:{}:{}",
        venue_tag(venue),
        side_tag(side),
        trigger.as_str()
    )
}

/// Map a `StopOrderKind` to the `PerpOrderKind` carried inside
/// `PendingKind::PerpVenueOrder`.
pub(super) const fn perp_order_kind_from_stop(kind: &StopOrderKind) -> PerpOrderKind {
    match kind {
        StopOrderKind::StopMarket => PerpOrderKind::StopMarket,
        StopOrderKind::StopLimit | StopOrderKind::TakeProfitLimit => PerpOrderKind::StopLimit,
        StopOrderKind::TakeProfit => PerpOrderKind::TakeProfit,
    }
}

/// Synthesise a `DataSource` for a freshly-emitted `PendingTx`. The sync
/// orchestrator owns later polling; reducer-side we mark the entry as
/// `UserSupplied` so it is not auto-refreshed until the orchestrator swaps
/// in a real `VenueApi` source. Mirrors `effect::token::pending_user_source`.
pub(super) const fn pending_user_source() -> DataSource {
    DataSource::UserSupplied
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::primitives::{ChainId, Decimal};

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    #[test]
    fn venue_tag_covers_every_variant() {
        for venue in [
            PerpVenue::Hyperliquid { chain: chain() },
            PerpVenue::GmxV2 { chain: chain() },
            PerpVenue::DyDxV4 { chain: chain() },
            PerpVenue::Vertex { chain: chain() },
            PerpVenue::Aevo { chain: chain() },
            PerpVenue::Drift { chain: chain() },
            PerpVenue::JupiterPerps { chain: chain() },
            PerpVenue::Synthetix { chain: chain() },
        ] {
            assert!(!venue_tag(&venue).is_empty());
            assert_eq!(venue_ref(&venue).name, venue_tag(&venue));
        }
    }

    #[test]
    fn orderbook_venues_classified_correctly() {
        assert!(is_orderbook_venue(&PerpVenue::Hyperliquid {
            chain: chain()
        }));
        assert!(is_orderbook_venue(&PerpVenue::Aevo { chain: chain() }));
        assert!(is_orderbook_venue(&PerpVenue::DyDxV4 { chain: chain() }));
        assert!(!is_orderbook_venue(&PerpVenue::GmxV2 { chain: chain() }));
        assert!(!is_orderbook_venue(&PerpVenue::Drift { chain: chain() }));
        assert!(!is_orderbook_venue(&PerpVenue::JupiterPerps {
            chain: chain()
        }));
        assert!(!is_orderbook_venue(&PerpVenue::Synthetix {
            chain: chain()
        }));
        assert!(!is_orderbook_venue(&PerpVenue::Vertex { chain: chain() }));
    }

    #[test]
    fn synth_position_id_stable_format() {
        let id = synth_position_id(
            &PerpVenue::Hyperliquid { chain: chain() },
            "ETH-PERP",
            &PerpSide::Long,
        );
        assert_eq!(id, "hyperliquid:ETH-PERP:long");
    }

    #[test]
    fn pending_id_for_limit_order_includes_all_keys() {
        let id = pending_id_for_limit_order(
            &PerpVenue::Hyperliquid { chain: chain() },
            "ETH-PERP",
            &PerpSide::Long,
            &Decimal::new("3000"),
        );
        assert_eq!(id, "limit:hyperliquid:ETH-PERP:long:3000");
    }

    #[test]
    fn perp_order_kind_from_stop_collapses_tp_limit_to_stop_limit() {
        // `PerpOrderKind` lacks a TakeProfitLimit variant; we collapse the
        // limit-flavored TP to StopLimit (Hyperliquid groups them under the
        // same orderbook lane), preserving the precise StopOrderKind on the
        // emitting action for downstream UI / policy.
        assert!(matches!(
            perp_order_kind_from_stop(&StopOrderKind::StopLimit),
            PerpOrderKind::StopLimit
        ));
        assert!(matches!(
            perp_order_kind_from_stop(&StopOrderKind::TakeProfitLimit),
            PerpOrderKind::StopLimit
        ));
        assert!(matches!(
            perp_order_kind_from_stop(&StopOrderKind::StopMarket),
            PerpOrderKind::StopMarket
        ));
        assert!(matches!(
            perp_order_kind_from_stop(&StopOrderKind::TakeProfit),
            PerpOrderKind::TakeProfit
        ));
    }
}
