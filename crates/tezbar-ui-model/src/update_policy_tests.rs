use super::*;
const NOW: f64 = 100_000_000_000.0;
fn current() -> Status {
    Status::UpToDate {
        version: "1.0".into(),
    }
}
fn available() -> Status {
    Status::Available {
        version: "1.0".into(),
        notes: "".into(),
        release_url: "".into(),
    }
}
fn ready() -> Status {
    Status::Ready {
        version: "1.0".into(),
    }
}
// src/shared/updater.test.ts: uses Stable as the recommended channel and explains shared Alpha data
#[test]
fn uses_stable_as_the_recommended_channel_and_explains_shared_alpha_data() {
    assert!(channel_description(ReleaseChannel::Stable, "0.2.7").contains("Recommended"));
    assert!(
        channel_description(ReleaseChannel::Alpha, "0.2.7").contains("current settings and data")
    );
}
// src/shared/updater.test.ts: explains waiting for a newer release after opting out of a prerelease
#[test]
fn explains_waiting_for_a_newer_release_after_opting_out_of_a_prerelease() {
    for v in ["0.3.0-alpha.1", "0.3.0-beta.1"] {
        assert!(
            channel_description(ReleaseChannel::Stable, v).contains("next Stable release newer")
        );
    }
    assert!(channel_description(ReleaseChannel::Beta, "0.3.0-alpha.1")
        .contains("next Beta or Stable release newer"));
}
// src/shared/updater.test.ts: returns false when never checked
#[test]
fn returns_false_when_never_checked() {
    assert!(!fresh(None, 1_000_000.0));
}
// src/shared/updater.test.ts: returns false when timestamp is NaN
#[test]
fn returns_false_when_timestamp_is_nan() {
    assert!(!fresh(Some(f64::NAN), 1_000_000.0));
}
// src/shared/updater.test.ts: returns true when checked within 12 hours
#[test]
fn returns_true_when_checked_within_12_hours() {
    for age in [1000.0, INTERVAL - 1000.0] {
        assert!(fresh(Some(NOW - age), NOW));
    }
}
// src/shared/updater.test.ts: returns false when 12 hours have elapsed
#[test]
fn returns_false_when_12_hours_have_elapsed() {
    for age in [INTERVAL, INTERVAL + 1.0] {
        assert!(!fresh(Some(NOW - age), NOW));
    }
}
// src/shared/updater.test.ts: returns false when timestamp is in the future
#[test]
fn returns_false_when_timestamp_is_in_the_future() {
    assert!(!fresh(Some(NOW + 5000.0), NOW));
}
// src/shared/updater.test.ts: checks when never checked before
#[test]
fn checks_when_never_checked_before() {
    assert!(auto_check(None, NOW));
}
// src/shared/updater.test.ts: checks when the previous timestamp is invalid
#[test]
fn checks_when_the_previous_timestamp_is_invalid() {
    assert!(auto_check(Some(f64::NAN), NOW));
}
// src/shared/updater.test.ts: skips the check when the interval has not elapsed
#[test]
fn skips_the_check_when_the_interval_has_not_elapsed() {
    for age in [1000.0, INTERVAL - 1.0, INTERVAL] {
        assert!(!auto_check(Some(NOW - age), NOW));
    }
}
// src/shared/updater.test.ts: checks again once the interval has elapsed
#[test]
fn checks_again_once_the_interval_has_elapsed() {
    assert!(auto_check(Some(NOW - INTERVAL - 1.0), NOW));
}
// src/shared/updater.test.ts: round-trips the timestamp
#[test]
fn round_trips_the_timestamp() {
    assert_eq!(read_last(Some(&record(42.0))), Some(42.0));
    for raw in ["0x2a", "0b101010", "0o52", " 42 "] {
        assert_eq!(read_last(Some(raw)), Some(42.0));
    }
}
// src/shared/updater.test.ts: returns null when unset or corrupt
#[test]
fn returns_null_when_unset_or_corrupt() {
    assert_eq!(read_last(None), None);
    assert_eq!(read_last(Some("not-a-number")), None);
}
// src/shared/updater.test.ts: reads and writes auto-update preference
#[test]
fn reads_and_writes_auto_update_preference() {
    assert!(!read_auto(None));
    assert!(read_auto(Some(&write_auto(true))));
    assert!(!read_auto(Some(&write_auto(false))));
}
// src/shared/updater.test.ts: preserves non-upToDate statuses unchanged
#[test]
fn preserves_non_uptodate_statuses_unchanged() {
    for s in [
        Status::Idle,
        available(),
        ready(),
        Status::Error {
            message: "err".into(),
        },
    ] {
        assert_eq!(effective(s.clone(), None, NOW), s);
    }
}
// src/shared/updater.test.ts: keeps upToDate status when checked within 12 hours
#[test]
fn keeps_uptodate_status_when_checked_within_12_hours() {
    assert_eq!(
        effective(current(), Some(NOW - 3_600_000.0), NOW),
        current()
    );
}
// src/shared/updater.test.ts: reverts upToDate status to idle after 12 hours have elapsed
#[test]
fn reverts_uptodate_status_to_idle_after_12_hours_have_elapsed() {
    assert_eq!(
        effective(current(), Some(NOW - INTERVAL - 1.0), NOW),
        Status::Idle
    );
    assert_eq!(effective(current(), None, NOW), Status::Idle);
}
// src/shared/updater.test.ts: returns true only for available, ready, and downloading states
#[test]
fn returns_true_only_for_available_ready_and_downloading_states() {
    for s in [
        available(),
        ready(),
        Status::Downloading {
            version: "1".into(),
            downloaded: 10,
            total: Some(100),
        },
        Status::ManualAvailable {
            version: "1".into(),
            notes: "".into(),
            release_url: "".into(),
            installer_url: "".into(),
        },
    ] {
        assert!(urgent(&s));
    }
    for s in [
        current(),
        Status::Idle,
        Status::Checking,
        Status::Error {
            message: "fail".into(),
        },
    ] {
        assert!(!urgent(&s));
    }
}
// src/shared/updater.test.ts: does not match empty query for upToDate when not checked in current window
#[test]
fn does_not_match_empty_query_for_uptodate_when_not_checked_in_current_window() {
    for q in ["", "   "] {
        assert!(!matches(q, &current(), false));
    }
}
// src/shared/updater.test.ts: matches empty query for urgent update notifications
#[test]
fn matches_empty_query_for_urgent_update_notifications() {
    for s in [available(), ready()] {
        assert!(matches("", &s, false));
    }
}
// src/shared/updater.test.ts: matches when user types update keywords for upToDate
#[test]
fn matches_when_user_types_update_keywords_for_uptodate() {
    for q in ["update", "upd", "tezbar", "обновление"] {
        assert!(matches(q, &current(), false));
    }
}
// src/shared/updater.test.ts: does not match unrelated queries
#[test]
fn does_not_match_unrelated_queries() {
    for q in ["safari", "terminal"] {
        assert!(!matches(q, &current(), false));
    }
}
// src/shared/updater.test.ts: does not promote upToDate once the window has reopened (not in active check window)
#[test]
fn does_not_promote_uptodate_once_the_window_has_reopened_not_in_active_check_window() {
    assert!(!promote(&current(), false));
}
// src/shared/updater.test.ts: promotes upToDate immediately after checking in the current window
#[test]
fn promotes_uptodate_immediately_after_checking_in_the_current_window() {
    assert!(promote(&current(), true));
}
// src/shared/updater.test.ts: promotes urgent update notifications regardless of window session
#[test]
fn promotes_urgent_update_notifications_regardless_of_window_session() {
    for s in [available(), ready()] {
        assert!(promote(&s, false));
    }
}
// src/shared/updater.test.ts: does not promote idle status
#[test]
fn does_not_promote_idle_status() {
    assert!(!promote(&Status::Idle, false));
}
// src/shared/updater.test.ts: downgrades score for upToDate when downgraded
#[test]
fn downgrades_score_for_uptodate_when_downgraded() {
    for (base, expected) in [(Some(200.0), 100.0), (Some(30.0), 30.0), (None, 50.0)] {
        assert_eq!(score(&current(), base, true), expected);
    }
}
// src/shared/updater.test.ts: preserves top score when not downgraded or for urgent updates
#[test]
fn preserves_top_score_when_not_downgraded_or_for_urgent_updates() {
    for s in [current(), available()] {
        assert_eq!(score(&s, None, false), 1_000_000.0);
    }
}
