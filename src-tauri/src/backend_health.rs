use serde::Serialize;
use std::time::{Duration, Instant};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub phase: String,
    pub detail: String,
    pub revision: u64,
    pub generation: u64,
}

impl Default for BackendStatus {
    fn default() -> Self {
        Self {
            phase: "starting".into(),
            detail: "Starting the background service…".into(),
            revision: 0,
            generation: 0,
        }
    }
}

pub struct HealthCheck {
    last_ping: Instant,
    awaiting_pong: Option<Instant>,
}

impl HealthCheck {
    pub fn new(now: Instant) -> Self {
        Self { last_ping: now, awaiting_pong: None }
    }

    pub fn should_ping(&mut self, now: Instant) -> bool {
        if self.awaiting_pong.is_none() && now.duration_since(self.last_ping) >= PING_INTERVAL {
            self.last_ping = now;
            self.awaiting_pong = Some(now);
            return true;
        }
        false
    }

    pub fn pong(&mut self) { self.awaiting_pong = None; }

    pub fn expired(&self, now: Instant) -> bool {
        self.awaiting_pong.is_some_and(|sent| now.duration_since(sent) >= HEALTH_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresponsive_process_expires_without_postponing_its_deadline() {
        let now = Instant::now();
        let mut health = HealthCheck::new(now);
        assert!(!health.should_ping(now));
        assert!(health.should_ping(now + PING_INTERVAL));
        assert!(!health.should_ping(now + PING_INTERVAL * 2));
        assert!(health.expired(now + PING_INTERVAL + HEALTH_TIMEOUT));
    }

    #[test]
    fn responsive_process_stays_healthy() {
        let now = Instant::now();
        let mut health = HealthCheck::new(now);
        assert!(health.should_ping(now + PING_INTERVAL));
        health.pong();
        assert!(!health.expired(now + HEALTH_TIMEOUT * 2));
        assert!(health.should_ping(now + HEALTH_TIMEOUT * 2));
    }
}
