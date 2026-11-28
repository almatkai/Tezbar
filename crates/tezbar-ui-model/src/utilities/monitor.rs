//! Structured system-stat presentation, matching the former SystemMonitorView helpers.
use serde_json::Value;
use tezbar_core::instant::transform::PresentationFormatter;
pub fn updated_at(snapshot: &Value) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(snapshot["collectedAt"].as_str()?)
        .ok()
        .map(|stamp| {
            stamp
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
}
pub struct Row {
    pub id: String,
    pub section: &'static str,
    pub label: &'static str,
    pub value: String,
    pub percent: Option<f64>,
}
pub fn decimal_bytes(n: f64) -> String {
    if !n.is_finite() {
        "—".into()
    } else if n >= 1e9 {
        format!("{:.2} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else {
        format!("{:.0} KB", n / 1000.)
    }
}
pub fn rate(n: Option<f64>) -> String {
    match n.filter(|n| n.is_finite()) {
        None => "Measuring…".into(),
        Some(n) if n >= 1e6 => format!("{:.1} MB/s", n / 1e6),
        Some(n) if n >= 1000. => format!("{:.0} KB/s", n / 1000.),
        Some(n) => format!("{n:.0} B/s"),
    }
}
pub fn uptime(n: f64) -> String {
    if !n.is_finite() || n < 0. {
        return "—".into();
    }
    let n = n.floor() as u64;
    let mut parts = vec![];
    if n / 86400 > 0 {
        parts.push(format!("{}d", n / 86400));
    }
    if (n % 86400) / 3600 > 0 {
        parts.push(format!("{}h", (n % 86400) / 3600));
    }
    parts.push(format!("{}m", (n % 3600) / 60));
    parts.join(" ")
}
pub fn rows(v: &Value, tab: &str, formatter: &dyn PresentationFormatter) -> Vec<Row> {
    let mut rows = vec![];
    let mut add = |section: &'static str,
                   key: &str,
                   label: &'static str,
                   value: String,
                   percent: Option<f64>| {
        if tab == "overview" || tab == section {
            rows.push(Row {
                id: format!("{section}.{key}"),
                section,
                label,
                value,
                percent: percent.filter(|n| n.is_finite()).map(|n| n.clamp(0., 100.)),
            });
        }
    };
    for (section, key, label) in [
        ("device", "name", "Computer"),
        ("device", "model", "Model"),
        ("device", "chip", "Chip"),
        ("device", "osVersion", "Operating system"),
        ("cpu", "model", "Processor"),
        ("gpu", "model", "Graphics"),
        ("gpu", "display", "Display"),
        ("storage", "name", "Drive"),
        ("storage", "device", "Device"),
        ("storage", "smartStatus", "Drive health"),
        ("network", "connectionType", "Connection"),
        ("network", "interface", "Interface"),
        ("network", "localIp", "Local IP"),
        ("network", "ssid", "Wi-Fi"),
    ] {
        add(
            section,
            key,
            label,
            v[section][key].as_str().unwrap_or("—").into(),
            None,
        );
    }
    let cpu = v["cpu"]["usagePercent"].as_f64();
    add(
        "cpu",
        "usagePercent",
        "CPU load",
        cpu.map(|n| format!("{n:.0}%"))
            .unwrap_or_else(|| "—".into()),
        cpu,
    );
    for (key, label) in [
        ("userPercent", "User"),
        ("systemPercent", "System"),
        ("idlePercent", "Idle"),
    ] {
        add(
            "cpu",
            key,
            label,
            v["cpu"][key]
                .as_f64()
                .map(|n| format!("{n:.1}%"))
                .unwrap_or_else(|| "—".into()),
            None,
        );
    }
    let load = if v["device"]["platform"] == "Windows" {
        "Not reported by Windows".into()
    } else {
        v["cpu"]["loadAverage"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_f64)
                    .map(|n| format!("{n:.2}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into())
    };
    add("cpu", "loadAverage", "Load average", load, None);
    for (section, key, label) in [
        ("cpu", "totalCores", "CPU cores"),
        ("cpu", "performanceCores", "Performance cores"),
        ("cpu", "efficiencyCores", "Efficiency cores"),
        ("gpu", "cores", "GPU cores"),
        ("processes", "total", "Processes"),
        ("processes", "running", "Running processes"),
    ] {
        add(
            section,
            key,
            label,
            v[section][key]
                .as_u64()
                .map(|n| formatter.count(n as usize))
                .unwrap_or_else(|| "—".into()),
            None,
        );
    }
    for section in ["memory", "storage"] {
        let used = v[section]["usedBytes"].as_f64();
        let total = v[section]["totalBytes"].as_f64();
        let percent = used
            .zip(total)
            .filter(|(_, n)| *n > 0.)
            .map(|(u, t)| u / t * 100.);
        for (key, label) in [
            ("usedBytes", "Used"),
            ("totalBytes", "Capacity"),
            ("freeBytes", "Available"),
            ("wiredBytes", "Wired"),
            ("compressedBytes", "Compressed"),
            ("swapUsedBytes", "Swap used"),
            ("swapTotalBytes", "Swap capacity"),
        ] {
            if section == "storage" && !matches!(key, "usedBytes" | "totalBytes" | "freeBytes") {
                continue;
            }
            if v[section].get(key).is_none()
                && matches!(
                    key,
                    "wiredBytes" | "compressedBytes" | "swapUsedBytes" | "swapTotalBytes"
                )
            {
                continue;
            }
            let value = v[section][key]
                .as_f64()
                .map(decimal_bytes)
                .unwrap_or_else(|| "—".into());
            add(
                section,
                key,
                label,
                value,
                if key == "usedBytes" { percent } else { None },
            );
        }
    }
    if v["battery"].is_null() {
        add(
            "battery",
            "status",
            "Battery",
            "No internal battery detected".into(),
            None,
        );
    } else {
        for (key, label) in [
            ("chargePercent", "Charge"),
            ("maximumCapacityPercent", "Maximum capacity"),
        ] {
            let n = v["battery"][key].as_f64();
            add(
                "battery",
                key,
                label,
                n.map(|n| format!("{n:.0}%")).unwrap_or_else(|| "—".into()),
                if key == "chargePercent" { n } else { None },
            );
        }
        let battery = &v["battery"];
        add(
            "battery",
            "power",
            "Power",
            if battery["isCharging"] == true {
                "Charging"
            } else if battery["isPluggedIn"] == true {
                "On power"
            } else {
                "On battery"
            }
            .into(),
            None,
        );
        add(
            "battery",
            "condition",
            "Condition",
            battery["condition"].as_str().unwrap_or("—").into(),
            None,
        );
        add(
            "battery",
            "cycleCount",
            "Cycle count",
            battery["cycleCount"]
                .as_u64()
                .map(|n| formatter.count(n as usize))
                .unwrap_or_else(|| "—".into()),
            None,
        );
        add(
            "battery",
            "temperature",
            "Temperature",
            battery["temperatureCelsius"]
                .as_f64()
                .map(|n| format!("{n:.1} °C"))
                .unwrap_or_else(|| "—".into()),
            None,
        );
        add(
            "battery",
            "remaining",
            "Remaining",
            battery["timeRemainingMinutes"]
                .as_u64()
                .filter(|n| *n > 0)
                .map(|n| format!("{}h {}m", n / 60, n % 60))
                .unwrap_or_else(|| "Calculating…".into()),
            None,
        );
        add(
            "battery",
            "lowPowerMode",
            "Low Power Mode",
            if battery["lowPowerMode"] == true {
                "On"
            } else {
                "Off"
            }
            .into(),
            None,
        );
    }
    for (key, label) in [
        ("downloadBytesPerSecond", "Download"),
        ("uploadBytesPerSecond", "Upload"),
    ] {
        add(
            "network",
            key,
            label,
            rate(v["network"][key].as_f64()),
            None,
        );
    }
    let fans = v["fans"]["rpms"]
        .as_array()
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.iter()
                .filter_map(Value::as_u64)
                .map(|n| format!("{} RPM", formatter.count(n as usize)))
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_else(|| v["fans"]["status"].as_str().unwrap_or("Unavailable").into());
    add("fans", "rpms", "Fans", fans, None);
    add(
        "device",
        "uptime",
        "Uptime",
        v["device"]["uptimeSeconds"]
            .as_f64()
            .map(uptime)
            .unwrap_or_else(|| "—".into()),
        None,
    );
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_rate_uptime_and_missing_data_match_the_source_display() {
        assert_eq!(decimal_bytes(17_179_869_184.), "17.18 GB");
        assert_eq!(decimal_bytes(1_500_000.), "1.5 MB");
        assert_eq!(rate(None), "Measuring…");
        assert_eq!(rate(Some(1234.)), "1 KB/s");
        assert_eq!(uptime(90061.), "1d 1h 1m");
        let rows = rows(
            &serde_json::json!({"memory":{"totalBytes":16_000_000_000u64,"usedBytes":8_000_000_000u64},"battery":null}),
            "overview",
            &tezbar_core::instant::transform::EnglishFormatter,
        );
        assert!(
            rows.iter().any(|r| r.id == "memory.usedBytes"
                && r.value == "8.00 GB"
                && r.percent == Some(50.))
        );
        assert!(
            rows.iter()
                .any(|r| r.section == "battery" && r.value == "No internal battery detected")
        );
        assert!(!rows.iter().any(|r| r.value.contains("totalBytes")));
    }
}
