//! Display-only command snapshot. This is NEVER a package verification cache.
//! Cheap file identities invalidate normal external edits on the next request.
//! A five-minute maximum age bounds missed/metadata-preserving mutations; the
//! next request after expiry rebuilds via the original fully verified registry.
use super::{registry, CoreContext};
use crate::native_core::common::CoreResult;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_AGE: Duration = Duration::from_secs(5 * 60);
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_INPUTS: usize = 16_384;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Inputs {
    pub roots: Vec<PathBuf>,
    pub files: Vec<(PathBuf, String)>,
}
struct Snapshot {
    inputs: Inputs,
    commands: Value,
    completed: Instant,
}
#[derive(Default)]
struct State {
    generation: u64,
    mutations: usize,
    snapshot: Option<Snapshot>,
}
#[derive(Default)]
pub(super) struct CommandList {
    state: Mutex<State>,
    // Only coalesces list requests, never excludes package writers or execution.
    refresh: std::sync::Arc<tokio::sync::Mutex<()>>,
    #[cfg(test)]
    before_publish: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
impl CommandList {
    pub fn mutation(self: &std::sync::Arc<Self>) -> Mutation {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.mutations += 1;
        state.snapshot = None;
        Mutation(self.clone())
    }
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.snapshot = None;
    }
    pub async fn commands(
        self: &std::sync::Arc<Self>,
        ctx: &CoreContext,
        #[cfg(test)] work: std::sync::Arc<super::process_install::hash_work::Work>,
    ) -> CoreResult {
        let refresh = self.refresh.clone().lock_owned().await;
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || {
            // Keep single-flight ownership even if the async caller is dropped.
            let _refresh = refresh;
            #[cfg(test)]
            let _measurement = super::process_install::hash_work::measure(work);
            this.load(&ctx)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    fn load(&self, ctx: &CoreContext) -> CoreResult {
        // No state or metadata mutation lock spans filesystem work. Fence both
        // publication AND cache hits against in-process writers, including a
        // writer that starts and finishes entirely during the blocking refresh.
        for _ in 0..2 {
            let generation = {
                let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.mutations != 0 {
                    return Err("Extension metadata is changing; retry command list".into());
                }
                state.generation
            };
            let inputs = match registry::command_inputs(ctx) {
                Ok(inputs) => inputs,
                Err(_) => {
                    // Unreadable/oversize input sets are not cached. Preserve
                    // the uncached registry's tolerant discovery behavior.
                    {
                        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        if state.generation != generation || state.mutations != 0 {
                            continue;
                        }
                        state.snapshot = None;
                    }
                    let result = registry::handle(ctx, "core:extensions:commands", &[]);
                    #[cfg(test)]
                    self.run_publish_hook();
                    // A failed input probe cannot fence external CLI writes.
                    // Re-read the authoritative descriptors instead: publish
                    // only matching observations, within the same two-attempt
                    // bound. This rare fallback does extra verification work;
                    // normal cache hits still never hash package contents.
                    let unchanged = result == registry::handle(ctx, "core:extensions:commands", &[]);
                    // Uncached output is still published to the caller. Fence
                    // it too: uninstall/update may finish during this read.
                    let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    if state.generation != generation || state.mutations != 0 || !unchanged {
                        continue;
                    }
                    return result;
                }
            };
            {
                let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.generation != generation || state.mutations != 0 {
                    continue;
                }
                if let Some(snapshot) = &state.snapshot {
                    if snapshot.inputs == inputs && snapshot.completed.elapsed() < MAX_AGE {
                        return Ok(snapshot.commands.clone());
                    }
                }
            }
            let commands = registry::handle(ctx, "core:extensions:commands", &[])?;
            // Do not publish a result observed across external CLI activation,
            // icon replacement, discovery changes, or an in-process mutation.
            #[cfg(test)]
            self.run_publish_hook();
            let after = registry::command_inputs(ctx);
            let cacheable = serde_json::to_vec(&commands)
                .map_err(|e| e.to_string())?
                .len()
                <= MAX_SNAPSHOT_BYTES;
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.generation != generation || state.mutations != 0 {
                continue;
            }
            state.snapshot = None;
            if after.as_ref().ok() != Some(&inputs) {
                continue;
            }
            if cacheable {
                state.snapshot = Some(Snapshot {
                    inputs,
                    commands: commands.clone(),
                    completed: Instant::now(),
                });
            }
            return Ok(commands);
        }
        Err("Extension metadata changed during command-list refresh; retry".into())
    }
    #[cfg(test)]
    fn run_publish_hook(&self) {
        let hook = self.before_publish.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
    }
    #[cfg(test)]
    pub fn has_snapshot(&self) -> bool {
        self.state.lock().unwrap().snapshot.is_some()
    }
    #[cfg(test)]
    pub fn before_publish(&self, hook: impl FnOnce() + Send + 'static) {
        *self.before_publish.lock().unwrap() = Some(Box::new(hook));
    }
    #[cfg(test)]
    pub fn expire(&self) {
        if let Some(snapshot) = self.state.lock().unwrap().snapshot.as_mut() {
            snapshot.completed = Instant::now() - MAX_AGE;
        }
    }
}
pub(super) struct Mutation(std::sync::Arc<CommandList>);
impl Drop for Mutation {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.mutations -= 1;
        state.generation = state.generation.wrapping_add(1);
        state.snapshot = None;
    }
}
