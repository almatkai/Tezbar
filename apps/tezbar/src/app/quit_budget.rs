//! One total quit budget. Reserve time before any irreversible cancellation.
use std::{cell::Cell, time::Duration};

pub const LIMIT: Duration = Duration::from_secs(8);
const COMMIT_RESERVE: Duration = Duration::from_secs(3);

pub fn commit<T>(
    elapsed: Duration,
    committed: &Cell<bool>,
    destructive: impl FnOnce() -> T,
) -> Result<T, String> {
    if LIMIT.saturating_sub(elapsed) < COMMIT_RESERVE {
        return Err("Too little quit time remains to stop runs safely; retry quit.".into());
    }
    committed.set(true);
    Ok(destructive())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_preflight_refuses_without_commit_dispose_or_cancel() {
        let committed = Cell::new(false);
        let destroyed = Cell::new(false);
        let result = commit(Duration::from_millis(7900), &committed, || {
            destroyed.set(true)
        });
        assert!(result.is_err());
        assert!(!committed.get());
        assert!(!destroyed.get());
        assert!(commit(Duration::from_secs(8), &committed, || panic!("must refuse")).is_err());
    }
    #[test]
    fn commit_uses_the_remaining_total_budget_with_three_seconds_reserved() {
        let committed = Cell::new(false);
        assert_eq!(
            commit(Duration::from_secs(5), &committed, || 42).unwrap(),
            42
        );
        assert!(committed.get());
        assert!(commit(Duration::from_millis(5001), &Cell::new(false), || ()).is_err());
    }
}
