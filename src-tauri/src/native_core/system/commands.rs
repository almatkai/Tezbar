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
        | "show-battery-status"
        | "list-listening-ports" => Some("info"),
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
                    Some(c) => c.try_wait().map_err(|e| e.to_string())?.is_none(),
                    None => false,
                };
                if id == "stop-keep-awake" {
                    if let Some(mut c) = guard.take() {
                        let _ = c.kill().await;
                        let _ = c.wait().await;
                    }
                    return Ok(state("Keep Awake turned off.", was, false));
                }
                if !was {
                    let mut command = if cfg!(windows) {
                        let mut c = Command::new("powershell.exe");
                        c.args(["-NoProfile","-NonInteractive","-Command",r#"Add-Type 'using System;using System.Runtime.InteropServices;public class Awake{[DllImport("kernel32.dll")]public static extern uint SetThreadExecutionState(uint f);}'; [Awake]::SetThreadExecutionState(0x80000003) | Out-Null; while($true){Start-Sleep -Seconds 60}"#]);
                        c
                    } else {
                        let mut c = Command::new("/usr/bin/caffeinate");
                        c.arg("-di");
                        c
                    };
                    command
                        .kill_on_drop(true)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());
                    *guard = Some(command.spawn().map_err(|e| e.to_string())?);
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
            "list-listening-ports" => {
                return Ok(result(
                    true,
                    "Use Port Manager → Open Ports in Tezbar for a structured, filterable list.",
                ))
            }
            "open-clipboard-history"
            | "open-snippets"
            | "open-quick-notes"
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
                    ps(r#"$window=(New-Object -ComObject Shell.Application).Windows() | Where-Object {$_.FullName -match 'explorer.exe$' -and $_.Document.Folder.Self.Path} | Select-Object -First 1; if($null -eq $window){throw 'No Explorer folder window is open.'}; $window.Document.Folder.Self.Path"#).await?
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
 let mut c=if cfg!(windows){let mut c=Command::new("powershell.exe");c.args(["-NoLogo","-NoProfile","-NonInteractive","-Command","Add-Type -AssemblyName System.Speech; $voice=New-Object System.Speech.Synthesis.SpeechSynthesizer; try {$voice.Speak($env:TEZBAR_SPEECH_TEXT)} finally {$voice.Dispose()}"]).env("TEZBAR_SPEECH_TEXT",text);c}else{let mut c=Command::new("say");c.arg(text);c};c.stdin(std::process::Stdio::null());output(c,Duration::from_secs(600)).await?;Ok("Reading aloud".into())
 }else{let enabled=id.ends_with(".on");if cfg!(windows){ps(&format!(r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'; Set-ItemProperty -Path $p -Name AppsUseLightTheme -Value {0}; Set-ItemProperty -Path $p -Name SystemUsesLightTheme -Value {0}",if enabled{0}else{1})).await?;}else{apple(&format!("tell application \"System Events\" to tell appearance preferences to set dark mode to {enabled}")).await?;}Ok(format!("Dark mode {}",if enabled{"enabled"}else{"disabled"}))}}.await;
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
    async fn native_windows(&self, id: &str) -> CoreResult {
        let script=match id {
 "toggle-dark-mode"=>{let query=r"Get-ItemPropertyValue -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name AppsUseLightTheme";let was=ps(query).await?=="0";ps(&format!(r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize';Set-ItemProperty -Path $p -Name AppsUseLightTheme -Value {0};Set-ItemProperty -Path $p -Name SystemUsesLightTheme -Value {0}",if was{1}else{0})).await?;let on=ps(query).await?=="0";return Ok(state("Toggled Windows dark mode",was,on));}
 "start-screen-saver"|"lock-screen"=>{run("rundll32.exe",&["user32.dll,LockWorkStation"]).await?;return Ok(result(true,"Screen locked"));}
 "toggle-mute"|"volume-up"|"volume-down"=>{
 let key=match id{"toggle-mute"=>173,"volume-up"=>175,_=>174};ps(&format!(r#"Add-Type 'using System;using System.Runtime.InteropServices;public class TezbarAudio{{[DllImport("user32.dll")]public static extern void keybd_event(byte k,byte s,uint f,UIntPtr x);}}';[TezbarAudio]::keybd_event({key},0,0,[UIntPtr]::Zero);[TezbarAudio]::keybd_event({key},0,2,[UIntPtr]::Zero)"#)).await?;return Ok(result(true,"System audio updated"));}
 "sleep-display"=>r#"Add-Type 'using System;using System.Runtime.InteropServices;public class Display{[DllImport("user32.dll")]public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);}';[Display]::SendMessage([IntPtr]0xffff,0x0112,[IntPtr]0xF170,[IntPtr]2)|Out-Null"#,
 "sleep-system"=>"Add-Type -AssemblyName System.Windows.Forms; if(-not [System.Windows.Forms.Application]::SetSuspendState([System.Windows.Forms.PowerState]::Suspend,$false,$false)){throw 'Windows refused suspend request'}",
 "show-network-info"=>"Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -notlike '127.*'} | Select-Object -ExpandProperty IPAddress",
 "flush-dns-cache"=>{let s=run("ipconfig.exe",&["/flushdns"]).await?;return Ok(result(true,s));}
 "toggle-wifi"=>{
 let adapter=r#"$adapter=Get-NetAdapter -IncludeHidden | Where-Object {$_.HardwareInterface -and ($_.NdisPhysicalMedium -eq 'Native 802.11' -or $_.InterfaceDescription -match 'Wireless|Wi-Fi|802\.11')} | Select-Object -First 1;if($null -eq $adapter){throw 'No Wi-Fi adapter found.'};"#;
 let query=format!("{adapter} $adapter.AdminStatus");let was=ps(&query).await?=="Up";
 let toggle=format!("$ErrorActionPreference='Stop';{adapter} if($adapter.AdminStatus -eq 'Up'){{Disable-NetAdapter -Name $adapter.Name -Confirm:$false}}else{{Enable-NetAdapter -Name $adapter.Name -Confirm:$false}}");
 use base64::Engine;let bytes:Vec<u8>=toggle.encode_utf16().flat_map(u16::to_le_bytes).collect();let encoded=base64::engine::general_purpose::STANDARD.encode(bytes);
 ps(&format!("$p=Start-Process powershell.exe -Verb RunAs -Wait -PassThru -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand','{encoded}');if($p.ExitCode -ne 0){{throw 'Wi-Fi elevation cancelled or failed'}}")).await?;
 let on=ps(&query).await?=="Up";return Ok(state("Wi-Fi toggled",was,on));}
 "empty-trash"=>"Clear-RecycleBin -Force -ErrorAction Stop",
 "show-macos-version"=>"$os=Get-CimInstance Win32_OperatingSystem;'{0} — version {1}, build {2}' -f $os.Caption,$os.Version,$os.BuildNumber",
 "show-cpu-info"=>"Get-CimInstance Win32_Processor | ForEach-Object {\"$($_.Name) — $($_.NumberOfCores) cores\"}",
 "show-memory-info"=>"$os=Get-CimInstance Win32_OperatingSystem;'Free: {0:N1} GB / Total: {1:N1} GB' -f ($os.FreePhysicalMemory/1MB),($os.TotalVisibleMemorySize/1MB)",
 "show-disk-usage"=>"Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | ForEach-Object {'{0} Free: {1:N1} GB / {2:N1} GB' -f $_.DeviceID,($_.FreeSpace/1GB),($_.Size/1GB)}",
 "show-battery-status"=>"$b=Get-CimInstance Win32_Battery;if($b){$b | ForEach-Object {'Battery: {0}%' -f $_.EstimatedChargeRemaining}}else{'No battery detected'}",
 _=>return Err(format!("Command {id} is only available on macOS.")),
 };
        Ok(result(
            true,
            ps(script).await?.pipe_nonempty("Command completed".into()),
        ))
    }
}
