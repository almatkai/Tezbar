//! Parent persistence acknowledgements share the Settings close/quit barrier.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct State {
    next: u64,
    active: BTreeSet<u64>,
    latest: BTreeMap<&'static str, (u64, bool)>,
}
#[derive(Clone, Default)]
pub struct EffectBarrier {
    state: Arc<Mutex<State>>,
    wake: Arc<tokio::sync::Notify>,
}
impl EffectBarrier {
    pub fn is_clean(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.active.is_empty() && state.latest.values().all(|(_, ok)| *ok)
    }
    pub fn begin(&self, key: &'static str) -> u64 {
        let mut s = self.state.lock().unwrap();
        s.next += 1;
        let id = s.next;
        s.active.insert(id);
        s.latest.insert(key, (id, false));
        id
    }
    pub fn finish(&self, key: &'static str, id: u64, ok: bool) -> bool {
        let mut s = self.state.lock().unwrap();
        s.active.remove(&id);
        let current = s.latest.get(&key).is_some_and(|(token, _)| *token == id);
        if current {
            s.latest.insert(key, (id, ok));
        }
        drop(s);
        self.wake.notify_waiters();
        current
    }
    pub fn current(&self, key: &'static str, id: u64) -> bool {
        self.state
            .lock()
            .unwrap()
            .latest
            .get(key)
            .is_some_and(|(token, _)| *token == id)
    }
    pub fn drain(&self) -> tezbar_core::host::HostFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            loop {
                let wait = this.wake.notified();
                tokio::pin!(wait);
                wait.as_mut().enable();
                {
                    let s = this.state.lock().unwrap();
                    if s.active.is_empty() {
                        return if s.latest.values().all(|(_, ok)| *ok) {
                            Ok(())
                        } else {
                            Err("Could not save Settings. Retry saved changes.".into())
                        };
                    }
                }
                wait.await;
            }
        })
    }
}
