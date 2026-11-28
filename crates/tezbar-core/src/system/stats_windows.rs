//! Windows System Monitor facts read in-process (WMI over COM and Win32), never through a
//! PowerShell child. The JSON keys match the former `Get-CimInstance` scripts exactly so the
//! shared shaping in `stats.rs` is unchanged. Call from a blocking worker thread.
use super::windows_wmi::{wql_string, Com, Wmi};
use serde_json::{json, Map, Value};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;

type Row = Map<String, Value>;

fn first(rows: Result<Vec<Row>, String>) -> Row {
    rows.ok().and_then(|r| r.into_iter().next()).unwrap_or_default()
}
fn field(row: &Row, key: &str) -> Value {
    row.get(key).cloned().unwrap_or(Value::Null)
}
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}
fn text(row: &Row, key: &str) -> String {
    row.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}
/// `(rows | Measure-Object X -Sum).Sum`: null when nothing was reported.
fn sum(rows: &[Row], key: &str) -> Value {
    let values: Vec<f64> = rows.iter().filter_map(|r| number(&field(r, key))).collect();
    if values.is_empty() {
        Value::Null
    } else {
        json!(values.iter().sum::<f64>())
    }
}
fn system_drive() -> String {
    std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
}
/// MSFT_PhysicalDisk.HealthStatus, spelled the way Get-PhysicalDisk prints it.
fn disk_health(code: &Value) -> Value {
    match number(code).map(|n| n as u32) {
        Some(0) => json!("Healthy"),
        Some(1) => json!("Warning"),
        Some(2) => json!("Unhealthy"),
        Some(5) => json!("Unknown"),
        _ => Value::Null,
    }
}
fn by_largest(mut rows: Vec<Row>, key: &str) -> Row {
    rows.sort_by(|a, b| {
        let a = number(&field(a, key)).unwrap_or(0.0);
        let b = number(&field(b, key)).unwrap_or(0.0);
        b.total_cmp(&a)
    });
    rows.into_iter().next().unwrap_or_default()
}

/// Hardware facts read once per app run.
pub(super) fn fixed() -> Result<Value, String> {
    let _com = Com::init()?;
    let cim = Wmi::connect(r"ROOT\CIMV2")?;
    let computer = first(cim.query(
        "SELECT Manufacturer, Model, NumberOfLogicalProcessors FROM Win32_ComputerSystem",
        &["Manufacturer", "Model", "NumberOfLogicalProcessors"],
    ));
    let processor = first(cim.query("SELECT Name FROM Win32_Processor", &["Name"]));
    let gpu = by_largest(
        cim.query(
            "SELECT Name, AdapterRAM, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate FROM Win32_VideoController",
            &["Name", "AdapterRAM", "CurrentHorizontalResolution", "CurrentVerticalResolution", "CurrentRefreshRate"],
        )
        .unwrap_or_default(),
        "AdapterRAM",
    );
    let os = first(cim.query(
        "SELECT Caption, Version, BuildNumber FROM Win32_OperatingSystem",
        &["Caption", "Version", "BuildNumber"],
    ));
    let drive = first(cim.query(
        &format!(
            "SELECT VolumeName, DeviceID FROM Win32_LogicalDisk WHERE DeviceID = {}",
            wql_string(&system_drive())
        ),
        &["VolumeName", "DeviceID"],
    ));
    let fans: Vec<Value> = cim
        .query("SELECT DesiredSpeed FROM Win32_Fan", &["DesiredSpeed"])
        .unwrap_or_default()
        .iter()
        .filter_map(|r| number(&field(r, "DesiredSpeed")))
        .filter(|n| *n > 0.0)
        .map(|n| json!(n))
        .collect();
    let smart = Wmi::connect(r"ROOT\Microsoft\Windows\Storage")
        .and_then(|s| s.query("SELECT HealthStatus, Size FROM MSFT_PhysicalDisk", &["HealthStatus", "Size"]))
        .map(|rows| disk_health(&field(&by_largest(rows, "Size"), "HealthStatus")))
        .unwrap_or(Value::Null);
    let (design, full, cycles) = match Wmi::connect(r"ROOT\WMI") {
        Ok(w) => {
            let total = |class: &str, key: &str| {
                w.query(&format!("SELECT {key} FROM {class}"), &[key])
                    .map(|rows| sum(&rows, key))
                    .unwrap_or(Value::Null)
            };
            (
                total("BatteryStaticData", "DesignedCapacity"),
                total("BatteryFullChargedCapacity", "FullChargedCapacity"),
                total("BatteryCycleCount", "CycleCount"),
            )
        }
        Err(_) => (Value::Null, Value::Null, Value::Null),
    };
    Ok(json!({
        "name": std::env::var("COMPUTERNAME").ok(),
        "model": format!("{} {}", text(&computer, "Manufacturer"), text(&computer, "Model")),
        "cpu": field(&processor, "Name"),
        "cores": field(&computer, "NumberOfLogicalProcessors"),
        "gpu": field(&gpu, "Name"),
        "width": field(&gpu, "CurrentHorizontalResolution"),
        "height": field(&gpu, "CurrentVerticalResolution"),
        "refresh": field(&gpu, "CurrentRefreshRate"),
        "os": format!("{} · {} · build {}", text(&os, "Caption"), text(&os, "Version"), text(&os, "BuildNumber")),
        "storage": field(&drive, "VolumeName"),
        "device": field(&drive, "DeviceID"),
        "smart": smart,
        "fans": fans,
        "design": design,
        "full": full,
        "cycles": cycles,
    }))
}

fn ticks(time: FILETIME) -> f64 {
    ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)) as f64
}

/// One live sample. CPU values are cumulative 100 ns counters; `cpu_between` diffs them.
pub(super) fn live() -> Result<Value, String> {
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    let memory_ok = unsafe { GlobalMemoryStatusEx(&mut memory) } != 0;
    let (mut idle, mut kernel, mut user) = unsafe { std::mem::zeroed::<(FILETIME, FILETIME, FILETIME)>() };
    let cpu_ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0;
    let mut power: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    let power_ok = unsafe { GetSystemPowerStatus(&mut power) } != 0;
    let mut performance: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    performance.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let performance_ok = unsafe { GetPerformanceInfo(&mut performance, performance.cb) } != 0;
    let root: Vec<u16> = format!("{}\\", system_drive()).encode_utf16().chain(Some(0)).collect();
    let (mut available, mut disk_total, mut disk_free) = (0u64, 0u64, 0u64);
    let disk_ok = unsafe {
        GetDiskFreeSpaceExW(root.as_ptr(), &mut available, &mut disk_total, &mut disk_free)
    } != 0;

    // Battery: BatteryFlag 128 = no system battery, 255 = unknown (Win32_Battery returned none).
    let battery_present = power_ok && power.BatteryFlag != 128 && power.BatteryFlag != 255;
    let charging = power.BatteryFlag & 8 != 0;
    let percent = (power.BatteryLifePercent != 255).then_some(power.BatteryLifePercent);
    // Win32_Battery.BatteryStatus spelling consumed by stats.rs (6..=9 charging, 2|3 on AC).
    let status = if charging {
        6
    } else if power.ACLineStatus == 1 {
        if percent == Some(100) { 3 } else { 2 }
    } else {
        1
    };
    let minutes = (power.BatteryLifeTime != u32::MAX).then(|| power.BatteryLifeTime / 60);

    let _com = Com::init()?;
    let page_files = Wmi::connect(r"ROOT\CIMV2")
        .and_then(|w| w.query("SELECT AllocatedBaseSize, CurrentUsage FROM Win32_PageFileUsage", &["AllocatedBaseSize", "CurrentUsage"]))
        .unwrap_or_default();
    let megabytes = |key: &str| number(&sum(&page_files, key)).map(|n| json!(n * 1_048_576.0));
    let network = network_identity().unwrap_or_default();

    Ok(json!({
        "uptime": unsafe { GetTickCount64() } as f64 / 1000.0,
        "total": memory_ok.then_some(memory.ullTotalPhys),
        "free": memory_ok.then_some(memory.ullAvailPhys),
        "cpuUser": cpu_ok.then(|| ticks(user)),
        // Kernel time includes idle time.
        "cpuSystem": cpu_ok.then(|| (ticks(kernel) - ticks(idle)).max(0.0)),
        "cpuIdle": cpu_ok.then(|| ticks(idle)),
        "swapTotal": megabytes("AllocatedBaseSize"),
        "swapUsed": megabytes("CurrentUsage"),
        "batteryPresent": battery_present,
        "batteryCharge": battery_present.then_some(percent).flatten(),
        "batteryStatus": battery_present.then_some(status),
        "batteryMinutes": battery_present.then_some(minutes).flatten(),
        "interface": field(&network, "interface"),
        "ssid": field(&network, "ssid"),
        "ip": field(&network, "ip"),
        "medium": field(&network, "medium"),
        "rx": field(&network, "rx"),
        "tx": field(&network, "tx"),
        "processes": performance_ok.then_some(performance.ProcessCount),
        "diskTotal": disk_ok.then_some(disk_total),
        "diskFree": disk_ok.then_some(disk_free),
    }))
}

/// The connected profile's adapter, as Get-NetConnectionProfile/Get-NetAdapter reported it.
fn network_identity() -> Result<Row, String> {
    let net = Wmi::connect(r"ROOT\StandardCimv2")?;
    let profile = net
        .query(
            "SELECT InterfaceAlias, Name, IPv4Connectivity, IPv6Connectivity FROM MSFT_NetConnectionProfile",
            &["InterfaceAlias", "Name", "IPv4Connectivity", "IPv6Connectivity"],
        )?
        .into_iter()
        // 0 = Disconnected.
        .find(|p| {
            number(&field(p, "IPv4Connectivity")).unwrap_or(0.0) != 0.0
                || number(&field(p, "IPv6Connectivity")).unwrap_or(0.0) != 0.0
        })
        .ok_or("no connected network profile")?;
    let alias = text(&profile, "InterfaceAlias");
    let quoted = wql_string(&alias);
    let adapter = first(net.query(
        &format!("SELECT InterfaceIndex, NdisPhysicalMedium FROM MSFT_NetAdapter WHERE Name = {quoted}"),
        &["InterfaceIndex", "NdisPhysicalMedium"],
    ));
    let counters = first(net.query(
        &format!("SELECT ReceivedBytes, SentBytes FROM MSFT_NetAdapterStatisticsSettingData WHERE Name = {quoted}"),
        &["ReceivedBytes", "SentBytes"],
    ));
    let ip = number(&field(&adapter, "InterfaceIndex"))
        .map(|index| {
            first(net.query(
                &format!("SELECT IPAddress FROM MSFT_NetIPAddress WHERE AddressFamily = 2 AND InterfaceIndex = {}", index as u32),
                &["IPAddress"],
            ))
        })
        .map(|row| field(&row, "IPAddress"))
        .unwrap_or(Value::Null);
    let mut row = Row::new();
    row.insert("interface".into(), json!(alias));
    row.insert("ssid".into(), field(&profile, "Name"));
    row.insert("medium".into(), field(&adapter, "NdisPhysicalMedium"));
    row.insert("rx".into(), field(&counters, "ReceivedBytes"));
    row.insert("tx".into(), field(&counters, "SentBytes"));
    row.insert("ip".into(), ip);
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_health_matches_get_physical_disk_spelling() {
        assert_eq!(disk_health(&json!(0)), json!("Healthy"));
        assert_eq!(disk_health(&json!("2")), json!("Unhealthy"));
        assert_eq!(disk_health(&json!(9)), Value::Null);
    }
    #[test]
    fn native_samples_report_this_machine() {
        let fixed = fixed().unwrap();
        assert!(fixed["cpu"].as_str().is_some_and(|s| !s.is_empty()), "{fixed}");
        assert!(fixed["os"].as_str().is_some_and(|s| s.contains("build")), "{fixed}");
        let live = live().unwrap();
        assert!(live["total"].as_f64().is_some_and(|n| n > 0.0), "{live}");
        assert!(live["cpuIdle"].as_f64().is_some_and(|n| n > 0.0), "{live}");
        assert!(live["diskTotal"].as_f64().is_some_and(|n| n > 0.0), "{live}");
        assert!(live["processes"].as_f64().is_some_and(|n| n > 0.0), "{live}");
        assert!(live["uptime"].as_f64().is_some_and(|n| n > 0.0), "{live}");
    }
}
