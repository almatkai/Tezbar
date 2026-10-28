//! Worker-local, bounded caches for expensive progress and empty-queue reads.
//! Never consume search dirty rows: the transactional journal is shared read-only.
use super::{err, Store};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const COUNT_INTERVAL: Duration = Duration::from_secs(10);
const EMPTY_RECHECK: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    pub roots: Vec<String>,
    pub model: String,
    // Effective depth AND selection are explicit, even if eligible IDs happen
    // to be identical. Root order/updated_at and unrelated settings are not keys.
    policy: String,
}
impl Scope {
    pub fn new(settings: &Value, roots: &[Value], model: &str) -> Self {
        let mut policy: Vec<_> = roots
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap_or("").to_owned(),
                    r["enabled"] == true,
                    super::super::depth(r, settings).to_owned(),
                )
            })
            .collect();
        policy.sort();
        Self {
            roots: policy
                .iter()
                .filter(|(_, enabled, depth)| {
                    *enabled && !matches!(depth.as_str(), "off" | "basic")
                })
                .map(|(id, _, _)| id.clone())
                .collect(),
            model: model.to_owned(),
            policy: json!([
                settings["embeddingModel"],
                settings["embeddingVariant"],
                settings["depth"],
                policy
            ])
            .to_string(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    generation: i64,
    data_version: i64,
}
struct Counts {
    scope: Scope,
    stamp: Stamp,
    value: (u64, u64),
    completed: Instant,
}
struct Empty {
    scope: Scope,
    stamp: Stamp,
    completed: Instant,
}
#[derive(Default)]
pub(crate) struct Work {
    counts: Option<Counts>,
    empty: Option<Empty>,
}
#[derive(Clone, Copy)]
pub(crate) struct CountObservation {
    pub value: Option<(u64, u64)>,
    pub fresh: bool,
    pub reconciled: bool,
}
impl Store {
    fn semantic_stamp(&self) -> Result<Stamp, String> {
        // Retry failed optional journal installation rather than trust an
        // untracked generation. Do not modify cache generation or dirty rows.
        self.search_cache.borrow_mut().journal(&self.db)?;
        Ok(Stamp {
            generation: self
                .db
                .query_row(
                    "SELECT generation FROM temp.knowledge_search_generation WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(err)?,
            data_version: self
                .db
                .query_row("PRAGMA data_version", [], |r| r.get(0))
                .map_err(err)?,
        })
    }
    fn semantic_scope_matches(&self, scope: &Scope) -> Result<bool, String> {
        // Also fence an external writer changing root/depth/settings. The
        // service can hydrate its small settings snapshot and retry, not count
        // an obsolete scope. Local metadata/queue writes do not dirty stamps.
        Ok(Scope::new(&self.settings()?, &self.roots()?, &scope.model) == *scope)
    }
    pub fn semantic_counts_if_due(
        &self,
        scope: &Scope,
        now: Instant,
    ) -> Result<CountObservation, String> {
        let started = Instant::now();
        self.search_checkpoint()?;
        if !self.semantic_scope_matches(scope)? {
            return Err("Knowledge semantic scope changed".into());
        }
        let stamp = self.semantic_stamp()?;
        {
            let work = self.semantic_work.borrow();
            if let Some(c) = work.counts.as_ref().filter(|c| c.scope == *scope) {
                if c.stamp == stamp {
                    return Ok(CountObservation {
                        value: Some(c.value),
                        fresh: true,
                        reconciled: false,
                    });
                }
                if now.saturating_duration_since(c.completed) < COUNT_INTERVAL {
                    return Ok(CountObservation {
                        value: Some(c.value),
                        fresh: false,
                        reconciled: false,
                    });
                }
            }
        }
        let value = self.gemma_counts_for_model(&scope.roots, &scope.model)?;
        self.search_checkpoint()?;
        if stamp != self.semantic_stamp()? || !self.semantic_scope_matches(scope)? {
            // Includes commits from other connections DURING the read. A
            // failed/cancelled/reordered result never marks this scope clean.
            return Err("Knowledge semantic data changed during count".into());
        }
        self.semantic_work.borrow_mut().counts = Some(Counts {
            scope: scope.clone(),
            stamp,
            value,
            // The interval is measured from completion, never query admission.
            completed: now + started.elapsed(),
        });
        Ok(CountObservation {
            value: Some(value),
            fresh: true,
            reconciled: true,
        })
    }
    pub fn semantic_pending(
        &self,
        scope: &Scope,
        limit: usize,
        now: Instant,
    ) -> Result<Vec<Value>, String> {
        let started = Instant::now();
        self.search_checkpoint()?;
        if !self.semantic_scope_matches(scope)? {
            return Err("Knowledge semantic scope changed".into());
        }
        let stamp = self.semantic_stamp()?;
        if self.semantic_work.borrow().empty.as_ref().is_some_and(|e| {
            e.scope == *scope
                && e.stamp == stamp
                && now.saturating_duration_since(e.completed) < EMPTY_RECHECK
        }) {
            return Ok(vec![]);
        }
        let rows = self.gemma_pending_for_model(&scope.roots, limit, &scope.model)?;
        self.search_checkpoint()?;
        if stamp != self.semantic_stamp()? || !self.semantic_scope_matches(scope)? {
            return Err("Knowledge semantic data changed during pending read".into());
        }
        self.semantic_work.borrow_mut().empty = if rows.is_empty() {
            Some(Empty {
                scope: scope.clone(),
                stamp,
                completed: now + started.elapsed(),
            })
        } else {
            None
        };
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "semantic_work_tests.rs"]
mod tests;
