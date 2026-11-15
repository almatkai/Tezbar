use super::*;
pub(super) fn descriptors() -> Value {
    serde_json::from_str(if cfg!(windows) {
        include_str!("native-windows.json")
    } else {
        include_str!("native-macos.json")
    })
    .unwrap()
}
fn descriptor(id: &str) -> Option<Value> {
    descriptors()
        .as_array()?
        .iter()
        .find(|d| d["id"] == id)
        .cloned()
}
fn kind(id: &str) -> Option<&'static str> {
    match id {
        "show-network-info"
        | "show-public-ip"
        | "show-macos-version"
        | "show-cpu-info"
        | "show-system-monitor"
        | "show-memory-info"
        | "show-disk-usage"
        | "show-battery-status" => Some("info"),
        "copy-current-path" | "git-root" => Some("copied"),
        "generate-password" => Some("password"),
        "start-keep-awake" | "stop-keep-awake" => Some("toggle"),
        _ => None,
    }
}
fn state(message: impl Into<String>, was: bool, on: bool) -> Value {
    json!({"ok":true,"message":message.into(),"state":{"wasOn":was,"isOn":on}})
}
impl SystemCore {
    pub(super) async fn execute_native(&self, action: &Value) -> CoreResult {
        let shell_action = action["type"] == "run-shell";
        let id = action
            .as_str()
            .or_else(|| action["commandId"].as_str())
            .unwrap_or("");
        let d = if shell_action {
            json!({"title":"Run shell command"})
        } else {
            match descriptor(id) {
                Some(d) => d,
                None => return Ok(result(false, format!("Unknown command: {id}"))),
            }
        };
        if d["macOnly"] == true && !cfg!(target_os = "macos") {
            return Ok(
                json!({"ok":false,"message":format!("{} is only available on macOS.",d["title"].as_str().unwrap_or(id)),"kind":"error"}),
            );
        }
        let safety = if shell_action {
            "shell.run"
        } else {
            match id {
                "empty-trash" => "trash.empty",
                "sleep-system" => "system.sleep",
                "quit-tezbar" => "app.quit",
                _ => "native.command",
            }
        };
        let context = if shell_action {
            json!({"command":action["command"]})
        } else {
            json!({"commandId":id})
        };
        let authorization = self.authorize(safety, context.clone()).await?;
        if authorization["accepted"] != true {
            return Ok(result(false, "Cancelled"));
        }
        if authorization["dryRun"] == true {
            return Ok(result(
                true,
                format!(
                    "Dry run: would have {}.",
                    d["title"].as_str().unwrap_or(id).to_lowercase()
                ),
            ));
        }
        let executed = if shell_action {
            let cmd = action["command"].as_str().unwrap_or("").trim();
            if cmd.is_empty() || cmd.chars().any(|c| ";|&`$(){}[]\n\r<>\\".contains(c)) {
                Ok(result(
                    false,
                    "Shell metacharacters are not allowed in run-shell commands.",
                ))
            } else if cfg!(windows) {
                ps(cmd).await.map(|s| {
                    result(
                        true,
                        if s.is_empty() {
                            "Command completed".into()
                        } else {
                            s
                        },
                    )
                })
            } else {
                shell(cmd).await.map(|s| {
                    result(
                        true,
                        if s.is_empty() {
                            "Command completed".into()
                        } else {
                            s
                        },
                    )
                })
            }
        } else {
            self.native_inner(id).await
        };
        let mut r = executed.unwrap_or_else(|e| {
            result(
                false,
                format!("{} failed: {e}", d["title"].as_str().unwrap_or(id)),
            )
        });
        if r["ok"] != true {
            r["kind"] = json!("error");
        } else if let Some(kind) = kind(id) {
            r["kind"] = json!(kind);
        }
        self.record(safety, context, r.clone()).await?;
        Ok(r)
    }
    async fn native_inner(&self, id: &str) -> CoreResult {
        match id {
            "start-keep-awake" | "stop-keep-awake" => {
                let mut guard = self.state.keep_awake.lock().await;
                let was = match guard.as_mut() {
                    Some(c) => c.is_active()?,
                    None => false,
                };
                if id == "stop-keep-awake" {
                    if let Some(c) = guard.take() {
                        c.stop().await;
                    }
                    return Ok(state("Keep Awake turned off.", was, false));
                }
                if !was {
                    *guard = Some(KeepAwake::start()?);
                }
                return Ok(state("Keep Awake is on.", was, true));
            }
            "generate-password" => {
                let password = password();
                self.ctx
                    .request("clipboard:write", vec![json!(password)])
                    .await?;
                return Ok(result(true, password));
            }
            "show-public-ip" => {
                let ip = reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .map_err(|e| e.to_string())?
                    .get("https://api.ipify.org")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .text()
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(result(true, format!("Public IP: {}", ip.trim())));
            }
            "open-downloads" | "open-applications" | "reveal-library" => {
                let path = match id {
                    "open-downloads" => self
                        .ctx
                        .paths
                        .home_dir
                        .join("Downloads")
                        .to_string_lossy()
                        .into_owned(),
                    "open-applications" => {
                        if cfg!(windows) {
                            "shell:AppsFolder".into()
                        } else {
                            "/Applications".into()
                        }
                    }
                    _ => {
                        if cfg!(windows) {
                            std::env::var("APPDATA").unwrap_or_else(|_| {
                                self.ctx
                                    .paths
                                    .home_dir
                                    .join("AppData/Roaming")
                                    .to_string_lossy()
                                    .into_owned()
                            })
                        } else {
                            self.ctx
                                .paths
                                .home_dir
                                .join("Library")
                                .to_string_lossy()
                                .into_owned()
                        }
                    }
                };
                self.ctx
                    .request("core:platform:open-file", vec![json!(path)])
                    .await?;
                return Ok(result(true, "Opened folder"));
            }
            "toggle-vpn-menu" => {
                self.ctx
                    .request(
                        "core:platform:open-url",
                        vec![json!(if cfg!(windows) {
                            "ms-settings:network-vpn"
                        } else {
                            "x-apple.systempreferences:com.apple.preference.network"
                        })],
                    )
                    .await?;
                return Ok(result(true, "Opened VPN settings"));
            }
            "show-system-monitor" => {
                return Ok(result(
                    true,
                    "Open System Monitor from the launcher to view live hardware information.",
                ))
            }
            "open-clipboard-history"
            | "open-snippets"
            | "open-notes"
            | "open-quick-notes"
            | "open-vault"
            | "open-emoji-picker" => {
                return Ok(result(
                    false,
                    format!(
                        "{} is a UI navigation — open the launcher to browse it.",
                        descriptor(id).unwrap()["title"].as_str().unwrap_or(id)
                    ),
                ))
            }
            "quit-tezbar" => return Ok(result(
                false,
                "Quit Tezbar is handled by the launcher so it can show the confirmation dialog.",
            )),
            "git-root" | "copy-current-path" => {
                let path = if cfg!(windows) {
                    {
                        #[cfg(windows)]
                        let folder = blocking(|| crate::windows_shell::explorer_folder().map(|p| json!(p)))
                            .await?
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        #[cfg(not(windows))]
                        let folder = String::new();
                        folder
                    }
                } else {
                    apple("tell application \"Finder\" to get POSIX path of (target of front Finder window as alias)").await?
                };
                let value = if id == "git-root" {
                    let mut c = Command::new("git");
                    c.args(["rev-parse", "--show-toplevel"]).current_dir(&path);
                    output(c, Duration::from_secs(10)).await?
                } else {
                    path
                };
                self.ctx
                    .request("clipboard:write", vec![json!(value)])
                    .await?;
                return Ok(result(true, value));
            }
            _ => {}
        }
        if cfg!(windows) {
            return self.native_windows(id).await;
        }
        if !cfg!(target_os = "macos") {
            return Err("Native system commands require macOS or Windows".into());
        }
        let script=match id {
 "toggle-dark-mode"=>{let was=apple("tell application \"System Events\" to tell appearance preferences to get dark mode").await?=="true";apple("tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode").await?;let on=apple("tell application \"System Events\" to tell appearance preferences to get dark mode").await?=="true";return Ok(state("Toggled Dark Mode",was,on));}
 "toggle-mute"=>{apple("set volume output muted (not (output muted of (get volume settings)))").await?;return Ok(result(true,"Toggled system mute"));}
 "volume-up"|"volume-down"=>{apple(&format!("set volume output volume (output volume of (get volume settings) {} 10)",if id=="volume-up"{"+"}else{"-"})).await?;return Ok(result(true,if id=="volume-up"{"Volume up"}else{"Volume down"}));}
 "empty-trash"=>{apple("tell application \"Finder\" to empty the trash").await?;return Ok(result(true,"Emptied Trash"));}
 "lock-screen"=>{apple("tell application \"System Events\" to keystroke \"q\" using {command down, control down}").await?;return Ok(result(true,"Screen locked"));}
 "sleep-system"=>{apple("tell application \"System Events\" to sleep").await?;return Ok(result(true,"System sleeping"));}
 "toggle-hide-desktop-icons"|"toggle-autohide-dock"|"toggle-autohide-menu-bar"=>{
 let (domain,key,restart,invert,default)=match id{"toggle-hide-desktop-icons"=>("com.apple.finder","CreateDesktop","Finder",true,true),"toggle-autohide-dock"=>("com.apple.dock","autohide","Dock",false,false),_=>("NSGlobalDomain","_HIHideMenuBar","SystemUIServer",false,false)};
 let raw=run("defaults",&["read",domain,key]).await;let before=raw.map(|r|r=="1"||r=="true").unwrap_or(default);run("defaults",&["write",domain,key,"-bool",if before{"false"}else{"true"}]).await?;run("killall",&[restart]).await?;return Ok(state("Preference toggled",before^invert,!before^invert));}
 "toggle-bluetooth"=>{let was=run("blueutil",&["-p"]).await.map_err(|_|"Bluetooth control requires blueutil. Install with brew install blueutil.")?=="1";run("blueutil",&["-p",if was{"0"}else{"1"}]).await?;return Ok(state("Bluetooth toggled",was,!was));}
 "toggle-wifi"=>{let iface=shell("networksetup -listallhardwareports | awk '/Wi-Fi/{getline; print $2; exit}'").await?;if iface.is_empty(){return Err("No Wi-Fi interface found".into());}let was=run("networksetup",&["-getairportpower",&iface]).await?.ends_with("On");run("networksetup",&["-setairportpower",&iface,if was{"off"}else{"on"}]).await?;return Ok(state("Wi-Fi toggled",was,!was));}
 "start-screen-saver"=>"open -a ScreenSaverEngine",
 "sleep-display"=>"pmset displaysleepnow",
 "restart-dock"=>"killall Dock","restart-finder"=>"killall Finder","restart-menu-bar"=>"killall SystemUIServer",
 "show-network-info"=>"echo \"IP: $(ipconfig getifaddr en0 2>/dev/null || echo n/a)\"; networksetup -getairportnetwork en0",
 "flush-dns-cache"=>"sudo -n dscacheutil -flushcache && sudo -n killall -HUP mDNSResponder",
 "show-macos-version"=>"sw_vers && uname -v",
 "show-cpu-info"=>"sysctl -n machdep.cpu.brand_string; sysctl -n hw.ncpu; uptime",
 "show-memory-info"=>"memory_pressure | head -n 6; vm_stat | head -n 6",
 "show-disk-usage"=>"df -h / | tail -n 1",
 "show-battery-status"=>"pmset -g batt",
 "brew-outdated"=>"brew outdated --quiet","brew-update"=>"brew update",
 _=>return Err(format!("Unknown native command: {id}")),
 };
        let mut c = Command::new("/bin/bash");
        c.args(["-lc", script]);
        let out = output(
            c,
            Duration::from_secs(if id == "brew-update" { 300 } else { 20 }),
        )
        .await?;
        Ok(result(
            true,
            if out.is_empty() {
                if id == "brew-outdated" {
                    "All Homebrew formulae are up to date."
                } else {
                    "Command completed"
                }
                .into()
            } else if id == "brew-update" {
                out.chars()
                    .rev()
                    .take(400)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect()
            } else {
                out
            },
        ))
    }
    pub(super) async fn execute_builtin(&self, id: &str, payload: &Value) -> CoreResult {
        if !matches!(
            id,
            "system.dark-mode.on" | "system.dark-mode.off" | "speech.read-aloud"
        ) {
            return Ok(result(false, format!("Unknown command: {id}")));
        }
        let context = json!({"commandId":id});
        let auth = self.authorize("native.command", context.clone()).await?;
        if auth["accepted"] != true {
            return Ok(result(false, "Cancelled"));
        }
        if auth["dryRun"] == true {
            return Ok(result(true, "Dry run: would have run system command."));
        }
        let execution:Result<String,String>=async{if id=="speech.read-aloud" {
 let text=payload["text"].as_str().unwrap_or("").trim();if text.is_empty(){return Err("No text provided for read-aloud".into());}
 #[cfg(windows)]{let text=text.to_owned();blocking(move||crate::windows_shell::speak(&text,||false).map(|()|Value::Null)).await?;}
 #[cfg(not(windows))]{let mut c=Command::new("say");c.arg(text);c.stdin(std::process::Stdio::null());output(c,Duration::from_secs(600)).await?;}
 Ok("Reading aloud".into())
 }else{let enabled=id.ends_with(".on");if cfg!(windows){#[cfg(windows)]crate::windows_shell::set_dark_mode(enabled)?;}else{apple(&format!("tell application \"System Events\" to tell appearance preferences to set dark mode to {enabled}")).await?;}Ok(format!("Dark mode {}",if enabled{"enabled"}else{"disabled"}))}}.await;
        let r = match execution {
            Ok(m) => result(true, m),
            Err(e) => result(false, e),
        };
        self.record("native.command", context, r.clone()).await?;
        Ok(r)
    }
}
fn random_index(bound: usize) -> usize {
    // Use an unconstrained random UUID byte, rejecting the modulo bias region.
    let ceiling = 256 / bound * bound;
    loop {
        let byte = uuid::Uuid::new_v4().as_bytes()[0] as usize;
        if byte < ceiling {
            return byte % bound;
        }
    }
}
pub(super) fn password() -> String {
    const CLASSES: [&[u8]; 4] = [
        b"ABCDEFGHJKLMNPQRSTUVWXYZ",
        b"abcdefghijkmnopqrstuvwxyz",
        b"23456789",
        b"!@#$%^&*()-_=+[]{}",
    ];
    let alphabet: Vec<u8> = CLASSES.concat();
    let mut chars: Vec<u8> = CLASSES
        .iter()
        .map(|class| class[random_index(class.len())])
        .collect();
    while chars.len() < 20 {
        chars.push(alphabet[random_index(alphabet.len())]);
    }
    for i in (1..chars.len()).rev() {
        let j = random_index(i + 1);
        chars.swap(i, j);
    }
    String::from_utf8(chars).expect("ASCII password alphabet")
}

impl SystemCore {
    #[cfg(windows)]
    async fn native_windows(&self, id: &str) -> CoreResult {
        let id = id.to_owned();
        blocking(move || native_windows_blocking(&id)).await
    }
    #[cfg(not(windows))]
    async fn native_windows(&self, id: &str) -> CoreResult {
        Err(format!("Command {id} is only available on macOS."))
    }
}

/// Every Windows built-in command runs in-process (registry, Win32, WMI, WinRT): no shell child.
#[cfg(windows)]
fn native_windows_blocking(id: &str) -> CoreResult {
    use super::windows_wmi::{Com, Wmi};
    use crate::windows_shell as shell;
    type Row = serde_json::Map<String, Value>;
    let gb = |bytes: f64| bytes / 1_073_741_824.0;
    let text = |row: &Row, key: &str| match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(v @ Value::Number(_)) => v.to_string(),
        _ => String::new(),
    };
    let number = |row: &Row, key: &str| {
        row.get(key)
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0.0)
    };
    let message = match id {
        "toggle-dark-mode" => {
            let was = shell::dark_mode();
            shell::set_dark_mode(!was)?;
            return Ok(state("Toggled Windows dark mode", was, shell::dark_mode()));
        }
        "start-screen-saver" | "lock-screen" => {
            shell::lock_workstation()?;
            "Screen locked".to_owned()
        }
        "toggle-mute" | "volume-up" | "volume-down" => {
            shell::tap_key(match id {
                "toggle-mute" => shell::VK_VOLUME_MUTE,
                "volume-up" => shell::VK_VOLUME_UP,
                _ => shell::VK_VOLUME_DOWN,
            });
            "System audio updated".to_owned()
        }
        "sleep-display" => {
            shell::sleep_display();
            "Display sleeping".to_owned()
        }
        "sleep-system" => {
            shell::suspend()?;
            "System sleeping".to_owned()
        }
        "flush-dns-cache" => {
            shell::flush_dns()?;
            "Successfully flushed the DNS Resolver Cache.".to_owned()
        }
        "toggle-wifi" => {
            let (was, on) = shell::toggle_wifi()?;
            return Ok(state("Wi-Fi toggled", was, on));
        }
        "empty-trash" => if shell::empty_recycle_bin()? {
            "Emptied Recycle Bin"
        } else {
            "Recycle Bin is already empty"
        }
        .to_owned(),
        "show-memory-info" => {
            use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
            let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
            memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
                return Err("Memory information is unavailable".into());
            }
            format!(
                "Free: {:.1} GB / Total: {:.1} GB",
                gb(memory.ullAvailPhys as f64),
                gb(memory.ullTotalPhys as f64)
            )
        }
        "show-battery-status" => {
            use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
            let mut power: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
            let ok = unsafe { GetSystemPowerStatus(&mut power) } != 0;
            if ok && power.BatteryFlag != 128 && power.BatteryFlag != 255 && power.BatteryLifePercent != 255 {
                format!("Battery: {}%", power.BatteryLifePercent)
            } else {
                "No battery detected".to_owned()
            }
        }
        "show-macos-version" | "show-cpu-info" | "show-disk-usage" | "show-network-info" => {
            let _com = Com::init()?;
            match id {
                "show-macos-version" => {
                    let rows = Wmi::connect(r"ROOT\CIMV2")?.query(
                        "SELECT Caption, Version, BuildNumber FROM Win32_OperatingSystem",
                        &["Caption", "Version", "BuildNumber"],
                    )?;
                    let os = rows.first().ok_or("Windows version is unavailable")?;
                    format!(
                        "{} \u{2014} version {}, build {}",
                        text(os, "Caption"),
                        text(os, "Version"),
                        text(os, "BuildNumber")
                    )
                }
                "show-cpu-info" => Wmi::connect(r"ROOT\CIMV2")?
                    .query("SELECT Name, NumberOfCores FROM Win32_Processor", &["Name", "NumberOfCores"])?
                    .iter()
                    .map(|cpu| format!("{} \u{2014} {} cores", text(cpu, "Name").trim(), text(cpu, "NumberOfCores")))
                    .collect::<Vec<_>>()
                    .join("\n"),
                "show-disk-usage" => Wmi::connect(r"ROOT\CIMV2")?
                    .query(
                        "SELECT DeviceID, FreeSpace, Size FROM Win32_LogicalDisk WHERE DriveType = 3",
                        &["DeviceID", "FreeSpace", "Size"],
                    )?
                    .iter()
                    .map(|disk| {
                        format!(
                            "{} Free: {:.1} GB / {:.1} GB",
                            text(disk, "DeviceID"),
                            gb(number(disk, "FreeSpace")),
                            gb(number(disk, "Size"))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => Wmi::connect(r"ROOT\StandardCimv2")?
                    .query("SELECT IPAddress FROM MSFT_NetIPAddress WHERE AddressFamily = 2", &["IPAddress"])?
                    .iter()
                    .map(|row| text(row, "IPAddress"))
                    .filter(|ip| !ip.is_empty() && !ip.starts_with("127."))
                    .collect::<Vec<_>>()
                    .join("\n"),
            }
        }
        _ => return Err(format!("Command {id} is only available on macOS.")),
    };
    Ok(result(true, message.pipe_nonempty("Command completed".into())))
}

/// Keep Awake: an in-process power request on Windows, `caffeinate` on macOS.
pub(super) enum KeepAwake {
    #[cfg(windows)]
    Native(crate::windows_shell::KeepAwake),
    #[cfg_attr(windows, allow(dead_code))]
    Child(tokio::process::Child),
}
impl KeepAwake {
    fn start() -> Result<Self, String> {
        #[cfg(windows)]
        return crate::windows_shell::KeepAwake::start().map(Self::Native);
        #[cfg(not(windows))]
        {
            let mut command = Command::new("/usr/bin/caffeinate");
            command
                .arg("-di")
                .kill_on_drop(true)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            command.spawn().map(Self::Child).map_err(|e| e.to_string())
        }
    }
    fn is_active(&mut self) -> Result<bool, String> {
        match self {
            #[cfg(windows)]
            Self::Native(awake) => Ok(awake.is_active()),
            Self::Child(child) => Ok(child.try_wait().map_err(|e| e.to_string())?.is_none()),
        }
    }
    pub(super) async fn stop(self) {
        match self {
            #[cfg(windows)]
            Self::Native(awake) => drop(awake),
            Self::Child(mut child) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
    }
}

#[cfg(all(test, windows))]
mod native_windows_tests {
    use super::*;
    /// Read-only built-ins answer in-process (theme, radio, power and lock are not exercised:
    /// they would change the desktop of whoever runs the suite).
    #[test]
    fn information_commands_answer_without_a_shell() {
        for id in [
            "show-macos-version",
            "show-cpu-info",
            "show-memory-info",
            "show-disk-usage",
            "show-battery-status",
            "show-network-info",
            "flush-dns-cache",
        ] {
            let reply = native_windows_blocking(id).unwrap();
            assert_eq!(reply["ok"], true, "{id}: {reply}");
            let message = reply["message"].as_str().unwrap();
            assert!(!message.is_empty(), "{id}");
            match id {
                "show-macos-version" => assert!(message.contains("build"), "{message}"),
                "show-memory-info" => assert!(message.starts_with("Free: "), "{message}"),
                "show-disk-usage" => assert!(message.contains(" GB / "), "{message}"),
                _ => {}
            }
        }
        assert!(native_windows_blocking("not-a-command").is_err());
    }
}
