use super::*;
#[derive(Default)]
pub(super) struct StatsState {
    fixed: Option<Value>,
    latest: Option<(u64, Value)>,
    network: Option<(String, u64, f64, f64)>,
    cpu: Option<[f64; 3]>,
}
fn capture(text: &str, pattern: &str) -> Vec<String> {
    regex::Regex::new(pattern)
        .unwrap()
        .captures_iter(text)
        .last()
        .map(|c| {
            c.iter()
                .skip(1)
                .map(|m| m.map(|s| s.as_str()).unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default()
}
// Parse named sysctl fields exactly: "usec" is not the boot Unix seconds.
fn mac_uptime_seconds(boot: &str, now_seconds: u64) -> Option<u64> {
    let seconds = boot.split(['{', '}', ',']).find_map(|field| {
        let (key, value) = field.split_once('=')?;
        (key.trim() == "sec")
            .then(|| value.trim().parse::<u64>().ok())
            .flatten()
    })?;
    Some(now_seconds.saturating_sub(seconds))
}
// getifaddr returns one address; never accept an arbitrary entry from a summary.
fn mac_local_ipv4(raw: &str) -> Option<String> {
    let address = raw.trim().parse::<std::net::Ipv4Addr>().ok()?;
    (!address.is_unspecified() && address.octets()[0] < 224).then(|| address.to_string())
}
fn n(s: Option<&String>) -> Value {
    s.and_then(|s| s.parse::<f64>().ok())
        .map_or(Value::Null, |n| json!(n))
}
fn num(v: &Value) -> Value {
    if let Some(n) = v.as_f64() {
        json!(n)
    } else if let Some(s) = v.as_str() {
        s.parse::<f64>().ok().map_or(Value::Null, |n| json!(n))
    } else {
        Value::Null
    }
}
fn binary(s: Option<&String>, unit: Option<&String>) -> Value {
    s.and_then(|s| s.parse::<f64>().ok())
        .map(|v| {
            json!(
                v * match unit.map(String::as_str) {
                    Some("G" | "g") => 1073741824.0,
                    Some("M" | "m") => 1048576.0,
                    Some("K" | "k") => 1024.0,
                    _ => 1.0,
                }
            )
        })
        .unwrap_or(Value::Null)
}
fn difference(total: &Value, free: &Value) -> Value {
    match (total.as_f64(), free.as_f64()) {
        (Some(t), Some(f)) => json!((t - f).max(0.0)),
        _ => Value::Null,
    }
}
fn nullable(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        json!(s)
    }
}
async fn optional(file: &str, args: &[&str]) -> String {
    run(file, args).await.unwrap_or_default()
}
fn ioreg(raw: &str, key: &str) -> Value {
    n(capture(raw, &format!(r#""{}"\s*=\s*(\d+)"#, regex::escape(key))).first())
}
fn boolean(raw: &str, key: &str) -> Value {
    let c = capture(raw, &format!(r#""{}"\s*=\s*(Yes|No)"#, regex::escape(key)));
    c.first().map(|s| json!(s == "Yes")).unwrap_or(Value::Null)
}
pub(super) fn parse_top(raw: &str) -> Value {
    let cpu = capture(
        raw,
        r"CPU usage:\s*([\d.]+)% user,\s*([\d.]+)% sys,\s*([\d.]+)% idle",
    );
    let load = capture(raw, r"Load Avg:\s*([\d.]+),\s*([\d.]+),\s*([\d.]+)");
    let processes = capture(raw, r"Processes:\s*(\d+) total,\s*(\d+) running");
    let mem = capture(
        raw,
        r"PhysMem:\s*([\d.]+)([KMG]) used.*?,\s*([\d.]+)([KMG]) unused",
    );
    json!({"cpu":{"userPercent":n(cpu.first()),"systemPercent":n(cpu.get(1)),"idlePercent":n(cpu.get(2)),"usagePercent":match(n(cpu.first()).as_f64(),n(cpu.get(1)).as_f64()){(Some(a),Some(b))=>json!((a+b).min(100.0)),_=>Value::Null},"loadAverage":if load.len()==3{json!([n(load.first()),n(load.get(1)),n(load.get(2))])}else{Value::Null}},"processes":{"total":n(processes.first()),"running":n(processes.get(1))},"memory":{"usedBytes":binary(mem.first(),mem.get(1)),"freeBytes":binary(mem.get(2),mem.get(3))}})
}
impl SystemCore {
    pub(super) async fn system_stats(&self) -> CoreResult {
        if !cfg!(target_os = "macos") && !cfg!(windows) {
            return Err("System Monitor is currently available on macOS and Windows only.".into());
        }
        let mut state = self.state.stats.lock().await;
        if let Some((at, v)) = &state.latest {
            if now_ms().saturating_sub(*at) < 900 {
                return Ok(v.clone());
            }
        }
        let v = if cfg!(windows) {
            self.windows_stats(&mut state).await?
        } else {
            self.mac_stats(&mut state).await?
        };
        state.latest = Some((now_ms(), v.clone()));
        Ok(v)
    }
    async fn mac_stats(&self, state: &mut StatsState) -> CoreResult {
        if state.fixed.is_none() {
            let raw = run(
                "system_profiler",
                &[
                    "SPHardwareDataType",
                    "SPDisplaysDataType",
                    "SPPowerDataType",
                    "SPStorageDataType",
                    "-json",
                ],
            )
            .await?;
            let p: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            state.fixed = Some(mac_fixed(&p));
        }
        let mut fixed = state.fixed.clone().unwrap();
        let (top, vm, total, swap, battery, route, network, version, fans, disk, boot) = tokio::join!(
            optional("top", &["-l", "2", "-s", "1", "-n", "0", "-stats", "cpu"]),
            optional("vm_stat", &[]),
            optional("sysctl", &["-n", "hw.memsize"]),
            optional("sysctl", &["-n", "vm.swapusage"]),
            optional("ioreg", &["-r", "-n", "AppleSmartBattery", "-d", "1"]),
            optional("route", &["-n", "get", "default"]),
            optional("netstat", &["-ibn"]),
            optional("sw_vers", &["-productVersion"]),
            optional("ioreg", &["-r", "-c", "AppleSMC", "-d", "1"]),
            optional("df", &["-k", "/"]),
            optional("sysctl", &["-n", "kern.boottime"])
        );
        let live = parse_top(&top);
        let total = total.parse::<f64>().ok().map_or(Value::Null, |n| json!(n));
        let free = live["memory"]["freeBytes"].clone();
        fixed["device"]["osVersion"] = nullable(&version);
        fixed["device"]["uptimeSeconds"] = mac_uptime_seconds(&boot, now_ms() / 1000)
            .map(|seconds| json!(seconds))
            .unwrap_or(Value::Null);
        for (key, v) in live["cpu"].as_object().unwrap() {
            fixed["cpu"][key] = v.clone();
        }
        let page = n(capture(&vm, r"page size of (\d+) bytes").first()).as_f64();
        let pages = |label: &str| -> Value {
            match (
                page,
                n(capture(&vm, &format!(r"{}:\s+(\d+)\.", regex::escape(label))).first()).as_f64(),
            ) {
                (Some(p), Some(n)) => json!(p * n),
                _ => Value::Null,
            }
        };
        let swaps = capture(&swap, r"total = ([\d.]+)([MG])\s+used = ([\d.]+)([MG])");
        fixed["memory"] = json!({"totalBytes":total,"freeBytes":free,"usedBytes":difference(&total,&free),"wiredBytes":pages("Pages wired down"),"compressedBytes":pages("Pages occupied by compressor"),"swapTotalBytes":binary(swaps.first(),swaps.get(1)),"swapUsedBytes":binary(swaps.get(2),swaps.get(3))});
        if let Some(row) = disk.lines().find(|s| s.starts_with("/dev/")) {
            let cols: Vec<_> = row.split_whitespace().collect();
            let t = cols
                .get(1)
                .and_then(|s| s.parse::<f64>().ok())
                .map(|n| json!(n * 1024.0))
                .unwrap_or(Value::Null);
            let f = cols
                .get(3)
                .and_then(|s| s.parse::<f64>().ok())
                .map(|n| json!(n * 1024.0))
                .unwrap_or(Value::Null);
            fixed["storage"]["totalBytes"] = t.clone();
            fixed["storage"]["freeBytes"] = f.clone();
            fixed["storage"]["usedBytes"] = difference(&t, &f);
        }
        fixed["battery"] = if battery.contains("AppleSmartBattery") {
            let health = &fixed["batteryHealth"];
            let mut charge = ioreg(&battery, "CurrentCapacity");
            let max = ioreg(&battery, "MaxCapacity");
            if let (Some(c), Some(m)) = (charge.as_f64(), max.as_f64()) {
                if m > 100.0 {
                    charge = json!((c / m * 100.0).clamp(0.0, 100.0));
                }
            }
            json!({"chargePercent":charge,"maximumCapacityPercent":health["maximumCapacityPercent"],"cycleCount":ioreg(&battery,"CycleCount"),"condition":health["condition"],"isCharging":boolean(&battery,"IsCharging"),"isPluggedIn":boolean(&battery,"ExternalConnected"),"timeRemainingMinutes":ioreg(&battery,"TimeRemaining").as_u64().filter(|n|*n<65535).map(|n|json!(n)).unwrap_or(Value::Null),"temperatureCelsius":ioreg(&battery,"Temperature").as_f64().map(|n|json!(n/100.0)).unwrap_or(Value::Null),"lowPowerMode":health["lowPowerMode"]})
        } else {
            Value::Null
        };
        let iface = capture(&route, r"interface:\s*(\S+)")
            .first()
            .cloned()
            .unwrap_or_default();
        let (summary, address) = if iface.is_empty() {
            (String::new(), String::new())
        } else {
            let summary_args = ["getsummary", iface.as_str()];
            let address_args = ["getifaddr", iface.as_str()];
            tokio::join!(
                optional("ipconfig", &summary_args),
                optional("ipconfig", &address_args),
            )
        };
        let summary_value = |key: &str| {
            let v = capture(
                &summary,
                &format!(r"(?m)^\s*{}\s*:\s*(.+)$", regex::escape(key)),
            )
            .first()
            .cloned()
            .unwrap_or_default();
            if v == "<redacted>" {
                Value::Null
            } else {
                nullable(v.trim())
            }
        };
        fixed["network"] = json!({"interface":nullable(&iface),"connectionType":summary_value("InterfaceType"),"ssid":summary_value("SSID"),"localIp":mac_local_ipv4(&address).map_or(Value::Null, |address| json!(address)),"downloadBytesPerSecond":null,"uploadBytesPerSecond":null});
        if let Some((rx, tx)) = mac_network(&network, &iface) {
            network_rates(state, &mut fixed["network"], iface, rx, tx);
        }
        let rpms: Vec<f64> = regex::Regex::new(r#""F\d+Ac"\s*=\s*(\d+)"#)
            .unwrap()
            .captures_iter(&fans)
            .filter_map(|c| c[1].parse::<f64>().ok())
            .filter(|n| *n > 0.0)
            .collect();
        fixed["fans"] = json!({"available":!rpms.is_empty(),"status":if rpms.is_empty(){"Managed automatically"}else{"Active"},"rpms":rpms,"note":if rpms.is_empty(){json!("Fan RPM is not exposed by macOS on this Mac without a privileged sensor helper.")}else{Value::Null}});
        fixed["processes"] = live["processes"].clone();
        fixed["collectedAt"] = json!(chrono::Utc::now().to_rfc3339());
        fixed.as_object_mut().unwrap().remove("batteryHealth");
        Ok(fixed)
    }
}
pub(super) fn mac_network(raw: &str, iface: &str) -> Option<(f64, f64)> {
    let row = raw
        .lines()
        .find(|r| r.split_whitespace().next() == Some(iface) && r.contains("<Link#"))?;
    let c: Vec<_> = row.split_whitespace().collect();
    let i = c.iter().position(|s| s.starts_with("<Link#"))?;
    Some((c.get(i + 4)?.parse().ok()?, c.get(i + 7)?.parse().ok()?))
}
fn network_rates(state: &mut StatsState, v: &mut Value, iface: String, rx: f64, tx: f64) {
    let now = now_ms();
    if let Some((prev, at, r, t)) = &state.network {
        let secs = now.saturating_sub(*at) as f64 / 1000.0;
        if prev == &iface && secs > 0.0 && rx >= *r && tx >= *t {
            v["downloadBytesPerSecond"] = json!((rx - r) / secs);
            v["uploadBytesPerSecond"] = json!((tx - t) / secs);
        }
    }
    state.network = Some((iface, now, rx, tx));
}
pub(super) fn mac_fixed(p: &Value) -> Value {
    let h = &p["SPHardwareDataType"][0];
    let g = &p["SPDisplaysDataType"][0];
    let display = &g["spdisplays_ndrvs"][0];
    let storage = p["SPStorageDataType"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|s| s["mount_point"] == "/")
                .or_else(|| a.first())
        })
        .cloned()
        .unwrap_or(Value::Null);
    let physical = &storage["physical_drive"];
    let cores = capture(
        h["number_processors"].as_str().unwrap_or(""),
        r"proc\s+(\d+):(\d+):(\d+)",
    );
    let rows = p["SPPowerDataType"].as_array();
    let health = rows
        .and_then(|a| a.iter().find(|r| r["_name"] == "spbattery_information"))
        .map(|r| r["sppower_battery_health_info"].clone())
        .unwrap_or(Value::Null);
    let settings = rows.and_then(|a| a.iter().find(|r| r["_name"] == "sppower_information"));
    let cap = capture(
        health["sppower_battery_health_maximum_capacity"]
            .as_str()
            .unwrap_or(""),
        r"([\d.]+)\s*%",
    );
    let total = num(&storage["size_in_bytes"]);
    let free = num(&storage["free_space_in_bytes"]);
    json!({"device":{"platform":"macOS","name":h["machine_name"],"model":h["machine_model"],"chip":h["chip_type"],"osVersion":null,"uptimeSeconds":null},"cpu":{"model":h["chip_type"],"totalCores":n(cores.first()),"performanceCores":n(cores.get(1)),"efficiencyCores":n(cores.get(2))},"gpu":{"model":g["sppci_model"].as_str().or_else(||g["_name"].as_str()),"cores":num(&g["sppci_cores"]),"display":display["_spdisplays_pixels"]},"storage":{"name":storage["_name"],"device":physical["device_name"],"totalBytes":total,"freeBytes":free,"usedBytes":difference(&total,&free),"smartStatus":physical["smart_status"]},"batteryHealth":{"maximumCapacityPercent":n(cap.first()),"cycleCount":num(&health["sppower_battery_cycle_count"]),"condition":health["sppower_battery_health"].as_str().map(|s|if s=="Good"||s=="sppower_battery_health_good"{"Normal"}else{s}),"lowPowerMode":settings.and_then(|v| match v["Battery Power"]["LowPowerMode"].as_str() {Some("Yes")=>Some(true),Some("No")=>Some(false),_=>None})}})
}
impl SystemCore {
    async fn windows_stats(&self, state: &mut StatsState) -> CoreResult {
        // In-process WMI/Win32 (no PowerShell child per refresh).
        #[cfg(windows)]
        {
            if state.fixed.is_none() {
                state.fixed = Some(blocking(super::stats_windows::fixed).await?);
            }
        }
        #[cfg(windows)]
        let live: Value = blocking(super::stats_windows::live).await?;
        #[cfg(not(windows))]
        let live: Value = return Err("Windows statistics are only collected on Windows".into());
        let p = state.fixed.clone().unwrap();
        let mut cpu = json!({"model":p["cpu"],"totalCores":num(&p["cores"]),"performanceCores":null,"efficiencyCores":null,"usagePercent":null,"userPercent":null,"systemPercent":null,"idlePercent":null,"loadAverage":null});
        if let (Some(user), Some(system), Some(idle)) = (
            num(&live["cpuUser"]).as_f64(),
            num(&live["cpuSystem"]).as_f64(),
            num(&live["cpuIdle"]).as_f64(),
        ) {
            if let Some(previous) = state.cpu {
                if let Some(percentages) = cpu_between(previous, [user, system, idle]) {
                    cpu["userPercent"] = json!(percentages[0]);
                    cpu["systemPercent"] = json!(percentages[1]);
                    cpu["idlePercent"] = json!(percentages[2]);
                    cpu["usagePercent"] = json!(percentages[0] + percentages[1]);
                }
            }
            state.cpu = Some([user, system, idle]);
        }
        let mut network = json!({"interface":live["interface"],"connectionType":if live["medium"].as_i64()==Some(9){json!("Wi-Fi")}else if live["medium"].is_number(){json!("Ethernet")}else{Value::Null},"localIp":live["ip"],"ssid":live["ssid"],"downloadBytesPerSecond":null,"uploadBytesPerSecond":null});
        if let (Some(rx), Some(tx), Some(iface)) = (
            num(&live["rx"]).as_f64(),
            num(&live["tx"]).as_f64(),
            live["interface"].as_str(),
        ) {
            network_rates(state, &mut network, iface.to_owned(), rx, tx);
        }
        let health = match (num(&p["design"]).as_f64(), num(&p["full"]).as_f64()) {
            (Some(d), Some(f)) if d > 0.0 => json!((f / d * 100.0).min(100.0).round()),
            _ => Value::Null,
        };
        let battery = if live["batteryPresent"] == true {
            let status = live["batteryStatus"].as_u64();
            json!({"chargePercent":num(&live["batteryCharge"]),"maximumCapacityPercent":health,"cycleCount":num(&p["cycles"]),"condition":health.as_f64().map(|n|if n>=80.0{"Normal"}else{"Service recommended"}),"isCharging":status.map(|n|matches!(n,6..=9)),"isPluggedIn":status.map(|n|matches!(n,2|3|6..=9)),"timeRemainingMinutes":num(&live["batteryMinutes"]).as_f64().filter(|n|*n>0.0&&*n<715827882.0),"temperatureCelsius":null,"lowPowerMode":null})
        } else {
            Value::Null
        };
        let rpms = p["fans"].as_array().cloned().unwrap_or_default();
        let display = match (num(&p["width"]).as_f64(), num(&p["height"]).as_f64()) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => json!(format!(
                "{w} × {h}{}",
                num(&p["refresh"])
                    .as_f64()
                    .filter(|n| *n > 0.0)
                    .map(|n| format!(" @ {n} Hz"))
                    .unwrap_or_default()
            )),
            _ => Value::Null,
        };
        Ok(
            json!({"collectedAt":chrono::Utc::now().to_rfc3339(),"device":{"platform":"Windows","name":p["name"],"model":p["model"],"chip":p["cpu"],"osVersion":p["os"],"uptimeSeconds":num(&live["uptime"])},"cpu":cpu,"gpu":{"model":p["gpu"],"cores":null,"display":display},"memory":{"totalBytes":num(&live["total"]),"freeBytes":num(&live["free"]),"usedBytes":difference(&num(&live["total"]),&num(&live["free"])),"wiredBytes":null,"compressedBytes":null,"swapTotalBytes":num(&live["swapTotal"]),"swapUsedBytes":num(&live["swapUsed"])},"storage":{"name":p["storage"],"device":p["device"],"totalBytes":num(&live["diskTotal"]),"freeBytes":num(&live["diskFree"]),"usedBytes":difference(&num(&live["diskTotal"]),&num(&live["diskFree"])),"smartStatus":p["smart"]},"battery":battery,"network":network,"fans":{"available":!rpms.is_empty(),"status":if rpms.is_empty(){"Managed by firmware"}else{"Active"},"rpms":rpms,"note":if rpms.is_empty(){json!("Fan RPM is not exposed by standard Windows hardware APIs on this PC.")}else{Value::Null}},"processes":{"total":num(&live["processes"]),"running":null}}),
        )
    }
}
pub(super) fn cpu_between(previous: [f64; 3], current: [f64; 3]) -> Option<[f64; 3]> {
    let delta = [
        current[0] - previous[0],
        current[1] - previous[1],
        current[2] - previous[2],
    ];
    if delta.iter().any(|n| !n.is_finite() || *n < 0.0) {
        return None;
    }
    let sum = delta.iter().sum::<f64>();
    if sum <= 0.0 {
        return None;
    }
    Some(delta.map(|n| n / sum * 100.0))
}

#[cfg(test)]
#[path = "stats_parser_tests.rs"]
mod parser_tests;
