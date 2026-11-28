//! Pure update-row policy, preserving the current renderer's twelve-hour boundaries.
use tezbar_update::{release_channels::ReleaseChannel, AppUpdateStatus as Status};
pub const INTERVAL: f64 = 12.0 * 60.0 * 60.0 * 1000.0;
pub fn fresh(last: Option<f64>, now: f64) -> bool {
    last.is_some_and(|t| t.is_finite() && now - t >= 0.0 && now - t < INTERVAL)
}
pub fn auto_check(last: Option<f64>, now: f64) -> bool {
    last.is_none_or(|t| !t.is_finite() || now - t > INTERVAL)
}
pub fn read_last(raw: Option<&str>) -> Option<f64> {
    let s = raw.filter(|s| !s.is_empty())?.trim();
    let n = if s.is_empty() {
        0.0
    } else if let Some((radix, digits)) = [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ]
    .into_iter()
    .find_map(|(prefix, radix)| s.strip_prefix(prefix).map(|digits| (radix, digits)))
    {
        if digits.is_empty() {
            return None;
        }
        digits.chars().try_fold(0.0, |n, c| {
            c.to_digit(radix)
                .map(|digit| n * radix as f64 + digit as f64)
        })?
    } else {
        s.parse().ok()?
    };
    if n.is_finite() {
        Some(n)
    } else {
        None
    }
}
pub fn record(now: f64) -> String {
    now.to_string()
}
pub fn read_auto(raw: Option<&str>) -> bool {
    raw == Some("true")
}
pub fn write_auto(enabled: bool) -> String {
    enabled.to_string()
}
pub fn effective(s: Status, last: Option<f64>, now: f64) -> Status {
    if matches!(s, Status::UpToDate { .. }) && !fresh(last, now) {
        Status::Idle
    } else {
        s
    }
}
pub fn urgent(s: &Status) -> bool {
    matches!(
        s,
        Status::Available { .. }
            | Status::ManualAvailable { .. }
            | Status::Ready { .. }
            | Status::Downloading { .. }
    )
}
pub fn matches(query: &str, s: &Status, checked: bool) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        urgent(s) || (matches!(s, Status::Checking) && checked)
    } else {
        [
            "update",
            "tezbar",
            "upgrade",
            "обновление",
            "обнова",
            "версия",
            "check for updates",
        ]
        .iter()
        .any(|v| v.contains(&q))
    }
}
pub fn promote(s: &Status, checked: bool) -> bool {
    urgent(s) || (checked && matches!(s, Status::Checking | Status::UpToDate { .. }))
}
pub fn score(s: &Status, base: Option<f64>, downgraded: bool) -> f64 {
    if matches!(s, Status::UpToDate { .. }) && downgraded {
        base.unwrap_or(50.0).min(100.0)
    } else if matches!(s, Status::Error { .. }) {
        base.unwrap_or(100.0)
    } else {
        base.unwrap_or(1_000_000.0)
    }
}
pub fn channel_description(channel: ReleaseChannel, version: &str) -> &'static str {
    match channel{ReleaseChannel::Alpha=>"Experimental Alpha releases, plus Beta and Stable updates. Early releases use your current settings and data; switching channels never downgrades the app.",ReleaseChannel::Beta if version.contains("-alpha.")=>"You’ll receive the next Beta or Stable release newer than this Alpha build. Your settings and data stay in place.",ReleaseChannel::Beta=>"Beta and Stable updates. Early releases use your current settings and data; Alpha releases are excluded.",ReleaseChannel::Stable if version.contains("-alpha.")||version.contains("-beta.")=>"You’ll receive the next Stable release newer than this prerelease build. Your settings and data stay in place.",_=>"Recommended for everyday use. Only Stable updates are offered."}
}
#[cfg(test)]
#[path = "update_policy_tests.rs"]
mod tests;
