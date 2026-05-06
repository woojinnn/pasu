//! Host-provided capabilities the engine consults during evaluation.
//!
//! This bag is intentionally tiny and opinionated: every policy run gets a
//! short-lived `HostCapabilities` value with required and optional services
//! attached by the caller (for example:
//! `HostCapabilities::new(&oracle).with_portfolio(&portfolio).with_stats(&stats)`).
//!
//! `oracle` is required and always present because USD valuation is used by
//! lowering paths that enrich amounts.
//!
//! `portfolio` and `approvals` are optional enrichment inputs; if absent, the
//! corresponding context fields are omitted so policy authors can guard with
//! `context has ...` and decide fail-open vs fail-closed behavior.
//!
//! `stats` is optional and only needed for window-key projection and
//! reservation-aware policy checks.

pub mod approvals;
pub mod oracle;
pub mod portfolio;
pub mod stat_windows;

use std::fmt;

/// Bag of host-provided capabilities the engine consults during
/// evaluation. Construct once per evaluation pass, freeze, pass by
/// reference into adapters and the pipeline.
#[derive(Clone, Copy)]
pub struct HostCapabilities<'a> {
    oracle: &'a dyn oracle::Oracle,
    portfolio: Option<&'a dyn portfolio::Portfolio>,
    approvals: Option<&'a dyn approvals::Approvals>,
    stats: Option<&'a dyn stat_windows::StatWindows>,
}

impl<'a> HostCapabilities<'a> {
    /// Construct host capabilities with the required oracle.
    #[must_use]
    pub fn new(oracle: &'a dyn oracle::Oracle) -> Self {
        Self {
            oracle,
            portfolio: None,
            approvals: None,
            stats: None,
        }
    }

    /// Attach a portfolio provider.
    #[must_use]
    pub fn with_portfolio(mut self, portfolio: &'a dyn portfolio::Portfolio) -> Self {
        self.portfolio = Some(portfolio);
        self
    }

    /// Attach an approvals provider.
    #[must_use]
    pub fn with_approvals(mut self, approvals: &'a dyn approvals::Approvals) -> Self {
        self.approvals = Some(approvals);
        self
    }

    /// Attach a stat-window provider.
    #[must_use]
    pub fn with_stats(mut self, stats: &'a dyn stat_windows::StatWindows) -> Self {
        self.stats = Some(stats);
        self
    }

    /// Required oracle provider.
    #[must_use]
    pub fn oracle(&self) -> &dyn oracle::Oracle {
        self.oracle
    }

    /// Optional portfolio provider.
    #[must_use]
    pub fn portfolio(&self) -> Option<&dyn portfolio::Portfolio> {
        self.portfolio
    }

    /// Optional approvals provider.
    #[must_use]
    pub fn approvals(&self) -> Option<&dyn approvals::Approvals> {
        self.approvals
    }

    /// Optional stat-window provider.
    #[must_use]
    pub fn stats(&self) -> Option<&dyn stat_windows::StatWindows> {
        self.stats
    }
}

impl fmt::Debug for HostCapabilities<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostCapabilities")
            .field("oracle", &"<oracle>")
            .field("portfolio", &self.portfolio.is_some())
            .field("approvals", &self.approvals.is_some())
            .field("stats", &self.stats.is_some())
            .finish()
    }
}

pub use self::{
    approvals::{Approvals, ApprovalsError, MockApprovals},
    oracle::{MockOracle, Oracle, OracleError},
    portfolio::{MockPortfolio, Portfolio, PortfolioError},
    stat_windows::{MockStatWindows, ReservationId, StatDelta, StatKey, StatValue, StatWindows},
};
