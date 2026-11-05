//! Active run time excludes time waiting for a person's approval. Drop guards
//! resume the clock even when an approval future is cancelled.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::{sync::Notify, time::Instant};
pub struct Clock {
    started: Instant,
    paused: Mutex<(Duration, Option<Instant>)>,
    changed: Notify,
    expired: AtomicBool,
}
pub struct Pause(Arc<Clock>);
impl Clock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            paused: Mutex::new((Duration::ZERO, None)),
            changed: Notify::new(),
            expired: AtomicBool::new(false),
        })
    }
    pub fn pause(self: &Arc<Self>) -> Pause {
        self.paused.lock().unwrap().1 = Some(Instant::now());
        self.changed.notify_waiters();
        Pause(self.clone())
    }
    pub fn expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
    }
    pub async fn deadline(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (elapsed, paused) = {
                let state = self.paused.lock().unwrap();
                (
                    self.started
                        .elapsed()
                        .saturating_sub(state.0)
                        .saturating_sub(state.1.map(|t| t.elapsed()).unwrap_or_default()),
                    state.1.is_some(),
                )
            };
            let remaining = Duration::from_secs(1800).saturating_sub(elapsed);
            if remaining.is_zero() {
                self.expired.store(true, Ordering::Release);
                return;
            }
            if paused {
                notified.await
            } else {
                tokio::select! {_=notified=>{},_=tokio::time::sleep(remaining)=>{}}
            }
        }
    }
}
impl Drop for Pause {
    fn drop(&mut self) {
        let mut state = self.0.paused.lock().unwrap();
        if let Some(at) = state.1.take() {
            state.0 += at.elapsed();
        }
        drop(state);
        self.0.changed.notify_waiters();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn approval_time_is_excluded_and_drop_resumes_deadline() {
        let clock = Clock::new();
        let owner = clock.clone();
        let task = tokio::spawn(async move { owner.deadline().await });
        tokio::time::advance(Duration::from_secs(1700)).await;
        let pause = clock.pause();
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert!(!clock.expired());
        drop(pause);
        tokio::time::advance(Duration::from_secs(101)).await;
        task.await.unwrap();
        assert!(clock.expired());
    }
}
