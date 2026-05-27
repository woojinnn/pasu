//! Provider 별 health 추적. 실패 누적 시 일정 시간 unhealthy 로 마킹.

use std::collections::HashMap;
use std::time::Instant;

use super::config::FailoverConfig;

#[derive(Debug)]
pub struct HealthTracker {
    states: HashMap<String, ProviderHealth>,
    config: FailoverConfig,
}

#[derive(Debug, Clone)]
struct ProviderHealth {
    consecutive_failures: u32,
    /// 이 시각 이전에는 unhealthy 로 본다.
    unhealthy_until: Option<Instant>,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            unhealthy_until: None,
        }
    }
}

impl HealthTracker {
    pub fn new(config: FailoverConfig) -> Self {
        Self {
            states: HashMap::new(),
            config,
        }
    }

    pub fn is_unhealthy(&self, provider: &str) -> bool {
        let now = Instant::now();
        match self.states.get(provider) {
            Some(s) => s.unhealthy_until.map_or(false, |t| now < t),
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
            entry.unhealthy_until =
                Some(Instant::now() + std::time::Duration::from_secs(cooldown));
        }
    }
}
