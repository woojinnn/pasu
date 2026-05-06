//! Host-provided capability bag passed into adapters and the Pipeline.
//!
//! Construct once per evaluation pass via `HostCapabilities::new(oracle)`
//! for oracle-only flows, or `HostCapabilities::builder(oracle)
//! .with_portfolio(...).with_approvals(...).build()` for richer setups.
//! Optional capabilities default to `None`; lowering enrichment skips
//! fields that aren't supplied (fail-open).

use crate::approvals::Approvals;
use crate::oracle::Oracle;
use crate::portfolio::Portfolio;
use crate::stat_windows::StatWindows;

/// Bag of host-provided capabilities the engine consults during
/// evaluation. Construct once per evaluation pass, freeze, pass by
/// reference into adapters and the pipeline.
#[derive(Clone, Copy)]
pub struct HostCapabilities<'a> {
    oracle: &'a dyn Oracle,
    portfolio: Option<&'a dyn Portfolio>,
    approvals: Option<&'a dyn Approvals>,
    stats: Option<&'a dyn StatWindows>,
}

impl<'a> HostCapabilities<'a> {
    pub fn new(oracle: &'a dyn Oracle) -> Self {
        Self {
            oracle,
            portfolio: None,
            approvals: None,
            stats: None,
        }
    }

    pub fn builder(oracle: &'a dyn Oracle) -> HostCapabilitiesBuilder<'a> {
        HostCapabilitiesBuilder {
            oracle,
            portfolio: None,
            approvals: None,
            stats: None,
        }
    }

    pub fn oracle(&self) -> &dyn Oracle {
        self.oracle
    }

    pub fn portfolio(&self) -> Option<&dyn Portfolio> {
        self.portfolio
    }

    pub fn approvals(&self) -> Option<&dyn Approvals> {
        self.approvals
    }

    pub fn stats(&self) -> Option<&dyn StatWindows> {
        self.stats
    }
}

#[derive(Clone, Copy)]
pub struct HostCapabilitiesBuilder<'a> {
    oracle: &'a dyn Oracle,
    portfolio: Option<&'a dyn Portfolio>,
    approvals: Option<&'a dyn Approvals>,
    stats: Option<&'a dyn StatWindows>,
}

impl<'a> HostCapabilitiesBuilder<'a> {
    pub fn with_portfolio(mut self, portfolio: &'a dyn Portfolio) -> Self {
        self.portfolio = Some(portfolio);
        self
    }

    pub fn with_approvals(mut self, approvals: &'a dyn Approvals) -> Self {
        self.approvals = Some(approvals);
        self
    }

    pub fn with_stats(mut self, stats: &'a dyn StatWindows) -> Self {
        self.stats = Some(stats);
        self
    }

    pub fn build(self) -> HostCapabilities<'a> {
        HostCapabilities {
            oracle: self.oracle,
            portfolio: self.portfolio,
            approvals: self.approvals,
            stats: self.stats,
        }
    }
}
