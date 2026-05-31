//! `OrderIntent` — the read-only "order intent" shared by two order models.
//!
//! ## Why this exists (design note)
//!
//! Two order-carrying action models coexist in this crate:
//!
//! - [`PlaceLimitOrderAction`](crate::action::perp::PlaceLimitOrderAction) — the
//!   on-chain-ish perp-`DEX` model. It carries venue-live inputs (mark price,
//!   book, account state) that the `Sync` layer fetches before evaluation, and
//!   which the reducer reads for safety checks. Those `live_inputs` are
//!   **required**.
//! - [`HlOrderAction`](crate::action::hyperliquid_core::HlOrderAction) — the thin
//!   off-chain-`L1` model. A `Hyperliquid` `/exchange` order is a self-describing
//!   signed payload that physically lacks mark price / account state, so it has
//!   **no** live inputs and is evaluated with no network fetch.
//!
//! The two models are deliberately **kept separate** (the perp model's required
//! live-inputs safety invariant must not be weakened to "optional" just to host
//! the off-chain case). This trait is the small, read-only seam that lets callers
//! — chiefly the policy layer — treat the *order-intrinsic* fields the two models
//! genuinely share uniformly, WITHOUT collapsing the models, touching their
//! `Cedar` schemas, or changing the `Sync`/reducer pipelines.
//!
//! It captures only the fields that are present, type-compatible, and meaningful
//! on **both** models:
//!   - direction (`is_buy`),
//!   - limit price (a fractional-safe [`Decimal`]),
//!   - reduce-only flag,
//!   - a normalized time-in-force tag.
//!
//! Deliberately NOT included (they diverge between the two models, so a shared
//! accessor would have to lie or lossily convert):
//!   - size — perp uses [`SizeSpec`](crate::action::perp::SizeSpec) (a `U256`
//!     base/quote/leverage discriminant); `HL` carries a plain [`Decimal`];
//!   - market identity — perp carries a `MarketRef` symbol, `HL` carries a
//!     numeric `asset_index` (+ optionally a resolved symbol).
//!
//! Non-order actions (`HL` `withdraw` / `usd_send` / `approve_agent`, and every
//! non-`PlaceLimitOrder` perp action) are NOT order intents and do not implement
//! this trait.

use simulation_state::primitives::Decimal;

/// The order-intrinsic intent shared by the perp and `Hyperliquid` `CORE` order
/// models.
///
/// Read-only; implementors expose their existing fields, so this adds **zero**
/// runtime state and is purely a unification seam for callers.
pub trait OrderIntent {
    /// Direction: `true` ⇒ long / buy, `false` ⇒ short / sell.
    fn is_buy(&self) -> bool;

    /// Limit price as a fractional-safe decimal (denomination is venue-defined).
    fn price(&self) -> &Decimal;

    /// Whether the order may only reduce existing exposure.
    fn reduce_only(&self) -> bool;

    /// Normalized time-in-force tag — one of `"gtc"`, `"ioc"`, `"fok"`,
    /// `"post_only"`, `"gtd"`. (Perp maps its [`TimeInForce`] enum to this
    /// spelling; `HL` already carries the normalized string.)
    ///
    /// [`TimeInForce`]: crate::action::perp::TimeInForce
    fn time_in_force_tag(&self) -> &str;
}

impl OrderIntent for crate::action::perp::PlaceLimitOrderAction {
    fn is_buy(&self) -> bool {
        matches!(self.side, simulation_state::position::PerpSide::Long)
    }

    fn price(&self) -> &Decimal {
        // `Price` is a type alias for `Decimal`, so this is `&Decimal`.
        &self.price
    }

    fn reduce_only(&self) -> bool {
        self.reduce_only
    }

    fn time_in_force_tag(&self) -> &str {
        use crate::action::perp::TimeInForce;
        match self.time_in_force {
            TimeInForce::Gtc => "gtc",
            TimeInForce::Ioc => "ioc",
            TimeInForce::Fok => "fok",
            TimeInForce::PostOnly => "post_only",
            TimeInForce::Gtd { .. } => "gtd",
        }
    }
}

impl OrderIntent for crate::action::hyperliquid_core::HlOrderAction {
    fn is_buy(&self) -> bool {
        self.is_buy
    }

    fn price(&self) -> &Decimal {
        &self.price
    }

    fn reduce_only(&self) -> bool {
        self.reduce_only
    }

    fn time_in_force_tag(&self) -> &str {
        // HL already normalizes the wire `t` to "gtc" / "ioc" / "post_only".
        &self.tif
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::OrderIntent;
    use crate::action::hyperliquid_core::HlOrderAction;
    use crate::action::perp::{
        PerpAccountState, PerpVenue, PlaceLimitLiveInputs, PlaceLimitOrderAction, SizeSpec,
        TimeInForce,
    };
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::position::PerpSide;
    use simulation_state::primitives::{ChainId, Decimal, MarketRef, Price, Time, VenueRef, U256};

    fn live<T>(v: T) -> LiveField<T> {
        LiveField::new(v, DataSource::UserSupplied, Time::from_unix(0))
    }

    /// A perp short limit order, GTC, reduce-only, @ 60000.
    fn perp_order() -> PlaceLimitOrderAction {
        PlaceLimitOrderAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::arbitrum(),
            },
            market: MarketRef {
                symbol: "BTC-USD".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side: PerpSide::Short,
            size: SizeSpec::BaseAmount {
                amount: U256::from(1u64),
            },
            price: Price::new("60000"),
            time_in_force: TimeInForce::PostOnly,
            reduce_only: true,
            live_inputs: PlaceLimitLiveInputs {
                mark_price: live(Price::new("60000")),
                best_bid_ask: live((Price::new("0"), Price::new("0"))),
                open_orders_count: live(0u32),
                user_account_state: live(PerpAccountState {
                    total_collateral_usd: U256::ZERO,
                    used_margin_usd: U256::ZERO,
                    free_margin_usd: U256::ZERO,
                    open_positions: vec![],
                }),
            },
        }
    }

    /// The equivalent HL order: short, `post_only`, reduce-only, @ 60000.
    fn hl_order() -> HlOrderAction {
        HlOrderAction {
            asset_index: 0,
            symbol: Some("BTC".into()),
            is_buy: false,
            price: Decimal::new("60000"),
            size: Decimal::new("0.1"),
            reduce_only: true,
            tif: "post_only".into(),
        }
    }

    /// Both models expose the same order intent through the trait, despite their
    /// different internal field types (`PerpSide` vs bool, `TimeInForce` vs
    /// `String`).
    #[test]
    fn both_models_agree_via_trait() {
        let p = perp_order();
        let h = hl_order();
        for o in [&p as &dyn OrderIntent, &h as &dyn OrderIntent] {
            assert!(!o.is_buy(), "short ⇒ is_buy=false");
            assert!(o.reduce_only());
            assert_eq!(o.price().as_str(), "60000");
            assert_eq!(o.time_in_force_tag(), "post_only");
        }
    }

    /// Perp's `TimeInForce` enum maps to the same normalized tags `HL` uses.
    #[test]
    fn perp_tif_tags_match_hl_spelling() {
        let mut p = perp_order();
        for (tif, tag) in [
            (TimeInForce::Gtc, "gtc"),
            (TimeInForce::Ioc, "ioc"),
            (TimeInForce::Fok, "fok"),
            (TimeInForce::PostOnly, "post_only"),
        ] {
            p.time_in_force = tif;
            assert_eq!(p.time_in_force_tag(), tag);
        }
        p.time_in_force = TimeInForce::Gtd {
            until: Time::from_unix(1),
        };
        assert_eq!(p.time_in_force_tag(), "gtd");
    }

    /// A long `HL` order reads through as `is_buy=true`.
    #[test]
    fn hl_long_is_buy_true() {
        let mut h = hl_order();
        h.is_buy = true;
        assert!((&h as &dyn OrderIntent).is_buy());
    }
}
