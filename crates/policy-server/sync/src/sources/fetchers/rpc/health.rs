use std::collections::HashMap;
use std::time::Instant;

use super::config::FailoverConfig;

#[derive(Debug)]
pub struct HealthTracker {
    states: HashMap<String, ProviderHealth>,
    config: FailoverConfig,
}

#[derive(Debug, Clone, Default)]
struct ProviderHealth {
    consecutive_failures: u32,
    unhealthy_until: Option<Instant>,
}

impl HealthTracker {
    #[must_use]
    pub fn new(config: FailoverConfig) -> Self {
        Self {
            states: HashMap::new(),
            config,
        }
    }

    #[must_use]
    pub fn is_unhealthy(&self, provider: &str) -> bool {
        let now = Instant::now();
        match self.states.get(provider) {
            Some(s) => s.unhealthy_until.is_some_and(|t| now < t),
            None => false,
        }
    }

    pub fn record_success(&mut self, provider: &str) {
        let entry = self.states.entry(provider.to_string()).or_default();
        entry.consecutive_failures = 0;
        entry.unhealthy_until = None;
    }

    pub fn record_failure(&mut self, provider: &str) {
        let cooldown = self.config.unhealthy_cooldown_sec;
        let threshold = self.config.mark_unhealthy_after;
        let entry = self.states.entry(provider.to_string()).or_default();
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        if entry.consecutive_failures >= threshold {
            entry.unhealthy_until = Some(Instant::now() + std::time::Duration::from_secs(cooldown));
        }
    }
}
