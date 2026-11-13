use super::*;

#[test]
fn mac_boot_time_seconds_never_reads_the_usec_field() {
    let boot = "{ sec = 1793547147, usec = 219673 } Sat Nov  1 00:00:00 2026";
    let six_days = 6 * 24 * 60 * 60;
    assert_eq!(
        mac_uptime_seconds(boot, 1793547147 + six_days),
        Some(six_days)
    );
    assert_eq!(mac_uptime_seconds("{ usec = 219673 }", 1793547147), None);
    assert_eq!(
        mac_uptime_seconds("{ sec = invalid, usec = 12 }", 100),
        None
    );
    assert_eq!(mac_uptime_seconds("unavailable", 100), None);
    assert_eq!(mac_uptime_seconds("{ sec = 101, usec = 0 }", 100), Some(0));
}

#[test]
fn mac_local_ipv4_requires_an_explicit_address_not_an_ipconfig_summary() {
    let summary = "IPv4 : <dictionary> {\n  Addresses : <array> {\n    0 : 192.0.2.17\n  }\n  SubnetMasks : <array> {\n    0 : 255.255.255.0\n  }\n}\n";
    assert_eq!(mac_local_ipv4(summary), None);
    assert_eq!(mac_local_ipv4("192.0.2.17\n"), Some("192.0.2.17".into()));
    for invalid in [
        "",
        "999.1.2.3",
        "192.0.2.17 extra",
        "::1",
        "0.0.0.0",
        "255.255.255.255",
        "224.0.0.1",
    ] {
        assert_eq!(mac_local_ipv4(invalid), None);
    }
}
