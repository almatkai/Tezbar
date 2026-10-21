//! Native V2 port of benvp/audio-device (MIT). Behaviour and helper CLI protocol
//! were derived from the installed package's package.json and .sc-build files.
//! The macOS audio-devices/sound-control executables are pinned package assets.
//! Windows uses the pinned win-audio-cli 1.3.1 release, provisioned on first use
//! when it was not bundled. No JavaScript from the original extension is run.
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::v2_sdk;

const SLUG: &str = "audio-device";
const WINDOWS_HELPER_URL: &str =
    "https://github.com/Inovvia/go-win-audio-cli/releases/download/1.3.1/win-audio-cli.exe";
const WINDOWS_HELPER_SHA256: &str =
    "9bda285caea2477f5504a73b532d4102e5d8e37a63c16669faca1834ccfd2048";
const MAC_AUDIO_DEVICES_SHA256: &str =
    "638e4535c28f5fa8f9b8720ab63f672c8ce329dfcd5e7e01f11fbb3851a8f537";
const MAC_SOUND_CONTROL_SHA256: &str =
    "0f7d825d5fe08476b7efe881c79d4b391352ffa625891cb6df9d59853e1c7c88";
const MAX_HELPER_BYTES: usize = 16 * 1024 * 1024;
#[cfg(target_os = "windows")]
static WINDOWS_PROVISION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const ALL: &[&str] = &[
    "set-output-device",
    "set-input-device",
    "set-volume",
    "set-input-volume",
    "auto-switch-input",
    "auto-switch-output",
    "customize-order",
    "favourite",
    "toggle-favourites",
    "use-combo1",
    "use-combo2",
    "use-combo3",
];

pub fn commands() -> &'static [&'static str] {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        ALL
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[]
    }
}

#[derive(Clone, Debug)]
struct Device {
    id: String,
    uid: String,
    name: String,
    transport: String,
    is_communication: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Input,
    Output,
}
impl Direction {
    fn word(self) -> &'static str {
        if self == Self::Input {
            "input"
        } else {
            "output"
        }
    }
    fn title(self) -> &'static str {
        if self == Self::Input {
            "Input"
        } else {
            "Output"
        }
    }
    fn suffix(self) -> &'static str {
        if self == Self::Input {
            "Input"
        } else {
            "Output"
        }
    }
}

fn store_string<'a>(store: &'a Value, key: &str) -> Option<&'a str> {
    store
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}
fn stored_list(store: &Value, key: &str) -> Vec<String> {
    let value = &store[key];
    let parsed = if let Some(s) = value.as_str() {
        serde_json::from_str::<Value>(s).unwrap_or(Value::Null)
    } else {
        value.clone()
    };
    parsed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn store_bool(store: &Value, key: &str) -> bool {
    store[key]
        .as_bool()
        .unwrap_or_else(|| store[key].as_str() == Some("true"))
}
fn pin_key(direction: Direction, uid: &str) -> String {
    format!("pinnedVolume_{}_{}", direction.word(), uid)
}
fn pin(store: &Value, direction: Direction, uid: &str) -> Option<u8> {
    let value = &store[pin_key(direction, uid)];
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| *n <= 100)
        .map(|n| n as u8)
}
fn default_key(direction: Direction) -> String {
    format!("defaultDeviceUid{}", direction.suffix())
}
fn name_key(direction: Direction) -> String {
    format!("defaultDeviceName{}", direction.suffix())
}
fn hidden_key(direction: Direction) -> String {
    format!("hiddenDevices{}", direction.suffix())
}
fn show_key(direction: Direction) -> String {
    format!("showHiddenDevices{}", direction.suffix())
}
fn grace_key(direction: Direction) -> String {
    format!("graceUntil_{}", direction.word())
}

fn asset_path(ctx: &CoreContext, filename: &str) -> Result<PathBuf, String> {
    let candidates = [
        ctx.paths.resource_dir.join("audio-device").join(filename),
        ctx.paths
            .resource_dir
            .join("native/audio-device")
            .join(filename),
        ctx.paths
            .data_dir
            .join("extensions")
            .join(SLUG)
            .join(".tezbar-support")
            .join(filename),
        ctx.paths
            .data_dir
            .join("extensions")
            .join(SLUG)
            .join("assets")
            .join(filename),
        ctx.paths
            .data_dir
            .join("extensions/packages/raycast.audio-device/assets")
            .join(filename),
    ];
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        format!("Native audio helper {filename} is missing; install the audio-device native assets")
    })
}

fn expected_hash(filename: &str) -> Result<&'static str, String> {
    match filename {
        "audio-devices" => Ok(MAC_AUDIO_DEVICES_SHA256),
        "sound-control" => Ok(MAC_SOUND_CONTROL_SHA256),
        "win-audio-cli.exe" => Ok(WINDOWS_HELPER_SHA256),
        _ => Err(format!("Unknown audio helper: {filename}")),
    }
}

fn verify_helper_bytes(filename: &str, bytes: &[u8]) -> Result<(), String> {
    let expected = expected_hash(filename)?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{filename} checksum mismatch: expected {expected}, got {actual}"
        ))
    }
}

fn verify_helper_file(path: &Path, filename: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    verify_helper_bytes(filename, &bytes)
}

#[cfg(target_os = "macos")]
fn executable_path(ctx: &CoreContext, filename: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let source = asset_path(ctx, filename)?;
    verify_helper_file(&source, filename)?;
    if source
        .metadata()
        .map_err(|e| e.to_string())?
        .permissions()
        .mode()
        & 0o111
        != 0
    {
        return Ok(source);
    }
    // Installed assets are read-only; stage an executable copy in Native V2 data.
    let bin = ctx
        .paths
        .data_dir
        .join("native-extensions-v2/audio-device/bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let target = bin.join(filename);
    let source_len = source.metadata().map_err(|e| e.to_string())?.len();
    if target
        .metadata()
        .ok()
        .is_some_and(|m| m.len() == source_len && m.permissions().mode() & 0o111 != 0)
        && verify_helper_file(&target, filename).is_ok()
    {
        return Ok(target);
    }
    let temporary = bin.join(format!("{}.{}.tmp", filename, uuid::Uuid::new_v4()));
    std::fs::copy(&source, &temporary).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

#[cfg(target_os = "windows")]
async fn provision_windows_helper(ctx: &CoreContext) -> Result<PathBuf, String> {
    let _guard = WINDOWS_PROVISION_LOCK.lock().await;
    let target = ctx
        .paths
        .data_dir
        .join("native-extensions-v2/audio-device/bin/win-audio-cli.exe");
    if target.is_file() && verify_helper_file(&target, "win-audio-cli.exe").is_ok() {
        return Ok(target);
    }
    if let Ok(bundled) = asset_path(ctx, "win-audio-cli.exe") {
        if verify_helper_file(&bundled, "win-audio-cli.exe").is_ok() {
            return Ok(bundled);
        }
    }
    let parent = target.parent().ok_or("Invalid audio helper path")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(WINDOWS_HELPER_URL)
        .send()
        .await
        .map_err(|e| format!("Could not download the pinned Windows audio helper: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Windows audio helper download failed: {e}"))?;
    if response
        .content_length()
        .is_some_and(|n| n > MAX_HELPER_BYTES as u64)
    {
        return Err("Windows audio helper download exceeds size limit".into());
    }
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_HELPER_BYTES {
            return Err("Windows audio helper download exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    verify_helper_bytes("win-audio-cli.exe", &bytes)?;
    let temporary = parent.join(format!("win-audio-cli.{}.download", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    if target.is_file() {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| e.to_string())?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(format!("Could not install Windows audio helper: {error}"));
    }
    Ok(target)
}

async fn helper(ctx: &CoreContext, sound: bool, args: &[&str]) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let file = if sound {
        "sound-control"
    } else {
        "audio-devices"
    };
    #[cfg(target_os = "windows")]
    let file = {
        let _ = sound;
        "win-audio-cli.exe"
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, sound, args);
        return Err("Audio devices are unsupported on this platform".into());
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let path = executable_path(ctx, file)?;
        #[cfg(target_os = "windows")]
        let path = provision_windows_helper(ctx).await?;
        // Always verify immediately before execution, including bundled resources.
        verify_helper_file(&path, file)?;
        let mut process = tokio::process::Command::new(&path);
        process.args(args).kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(15), process.output())
            .await
            .map_err(|_| format!("{file} timed out"))?
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(format!(
                "{file} {} failed: {}",
                args.join(" "),
                if stderr.is_empty() {
                    output.status.to_string()
                } else {
                    stderr
                }
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| format!("{file} returned invalid UTF-8: {e}"))
    }
}
async fn json_helper(ctx: &CoreContext, sound: bool, args: &[&str]) -> Result<Value, String> {
    let text = helper(ctx, sound, args).await?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid audio helper JSON: {e}"))
}

fn parse_device(value: &Value, _direction: Direction) -> Result<Device, String> {
    let id = value["id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| value["id"].as_i64().map(|n| n.to_string()))
        .ok_or("Audio helper returned a device without an ID")?;
    let name = value["name"]
        .as_str()
        .ok_or("Audio helper returned a device without a name")?
        .to_owned();
    Ok(Device {
        uid: value["uid"].as_str().unwrap_or(&id).to_owned(),
        id,
        name,
        transport: value["transportType"].as_str().unwrap_or("").to_owned(),
        is_communication: value["isCommunication"].as_bool().unwrap_or(false),
    })
}
fn parse_devices(value: &Value, direction: Direction) -> Result<Vec<Device>, String> {
    let list = if value.is_array() {
        value
    } else {
        &value[if direction == Direction::Input {
            "inputs"
        } else {
            "outputs"
        }]
    };
    let list = list
        .as_array()
        .ok_or("Audio helper returned an invalid device list")?;
    list.iter().map(|v| parse_device(v, direction)).collect()
}
async fn devices(ctx: &CoreContext, direction: Direction) -> Result<Vec<Device>, String> {
    #[cfg(target_os = "macos")]
    {
        parse_devices(
            &json_helper(
                ctx,
                false,
                &[
                    "list",
                    if direction == Direction::Input {
                        "--input"
                    } else {
                        "--output"
                    },
                    "--json",
                ],
            )
            .await?,
            direction,
        )
    }
    #[cfg(target_os = "windows")]
    {
        parse_devices(
            &json_helper(ctx, false, &["list", "--json"]).await?,
            direction,
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, direction);
        Err("Audio devices are unsupported on this platform".into())
    }
}
async fn current(ctx: &CoreContext, direction: Direction) -> Result<Device, String> {
    #[cfg(target_os = "macos")]
    {
        parse_device(
            &json_helper(ctx, false, &[direction.word(), "get", "--json"]).await?,
            direction,
        )
    }
    #[cfg(target_os = "windows")]
    {
        let value = json_helper(ctx, false, &["list", "--json"]).await?;
        let list = &value[if direction == Direction::Input {
            "inputs"
        } else {
            "outputs"
        }];
        let found = list
            .as_array()
            .and_then(|a| a.iter().find(|v| v["isDefault"] == true))
            .ok_or("No default audio device found")?;
        parse_device(found, direction)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, direction);
        Err("Audio devices are unsupported on this platform".into())
    }
}
async fn switch(
    ctx: &CoreContext,
    direction: Direction,
    id: &str,
    system: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        helper(ctx, false, &[direction.word(), "set", id]).await?;
        if direction == Direction::Output && system {
            helper(ctx, false, &["system", "set", id]).await?;
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = system;
        helper(
            ctx,
            false,
            &[
                if direction == Direction::Input {
                    "switch-input"
                } else {
                    "switch-output"
                },
                "--id",
                id,
            ],
        )
        .await?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, direction, id, system);
        return Err("Audio devices are unsupported on this platform".into());
    }
    Ok(())
}
async fn volume(ctx: &CoreContext, direction: Direction, id: &str) -> Result<Option<u8>, String> {
    #[cfg(target_os = "macos")]
    let text = helper(
        ctx,
        true,
        &[
            if direction == Direction::Input {
                "get-input"
            } else {
                "get"
            },
            id,
        ],
    )
    .await?;
    #[cfg(target_os = "windows")]
    let text = helper(
        ctx,
        false,
        &[
            if direction == Direction::Input {
                "get-input-volume"
            } else {
                "get-output-volume"
            },
            "--id",
            id,
        ],
    )
    .await?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, direction, id);
        return Err("Audio devices are unsupported on this platform".into());
    }
    #[cfg(target_os = "macos")]
    {
        Ok(text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n.clamp(0.0, 100.0).round() as u8))
    }
    #[cfg(target_os = "windows")]
    {
        let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let returned_id = value["device"]["id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| value["device"]["id"].as_i64().map(|n| n.to_string()));
        if value["type"].as_str() != Some(direction.word()) || returned_id.as_deref() != Some(id) {
            return Err("Audio helper returned volume for a different device".into());
        }
        Ok(value["volume"]
            .as_f64()
            .filter(|n| n.is_finite())
            .map(|n| n.clamp(0.0, 100.0).round() as u8))
    }
}
async fn set_volume(
    ctx: &CoreContext,
    direction: Direction,
    id: &str,
    level: u8,
) -> Result<(), String> {
    let number = level.to_string();
    #[cfg(target_os = "macos")]
    {
        helper(
            ctx,
            true,
            &[
                if direction == Direction::Input {
                    "set-input"
                } else {
                    "set"
                },
                &number,
                id,
            ],
        )
        .await?;
    }
    #[cfg(target_os = "windows")]
    {
        helper(
            ctx,
            false,
            &[
                if direction == Direction::Input {
                    "set-input-volume"
                } else {
                    "set-output-volume"
                },
                "--volume",
                &number,
                "--id",
                id,
            ],
        )
        .await?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (ctx, direction, id, number);
        return Err("Audio devices are unsupported on this platform".into());
    }
    Ok(())
}
#[cfg(target_os = "macos")]
async fn muted(ctx: &CoreContext, direction: Direction, id: &str) -> Result<bool, String> {
    Ok(helper(
        ctx,
        true,
        &[
            if direction == Direction::Input {
                "mute-input"
            } else {
                "mute"
            },
            "get",
            id,
        ],
    )
    .await?
    .trim()
        == "true")
}
#[cfg(target_os = "macos")]
async fn set_mute(
    ctx: &CoreContext,
    direction: Direction,
    id: &str,
    mute: bool,
) -> Result<(), String> {
    helper(
        ctx,
        true,
        &[
            if direction == Direction::Input {
                "mute-input"
            } else {
                "mute"
            },
            if mute { "on" } else { "off" },
            id,
        ],
    )
    .await?;
    Ok(())
}

fn action(id: String, title: &str, kind: &str) -> Value {
    json!({"id":id,"title":title,"kind":kind})
}
fn a(direction: Direction, verb: &str, uid: &str) -> String {
    format!("audio:{}:{verb}:{uid}", direction.word())
}
fn list_title(direction: Direction) -> String {
    format!("Set {} Device", direction.title())
}
fn hidden(store: &Value, direction: Direction) -> Vec<String> {
    let key = hidden_key(direction);
    if store.get(&key).is_some() {
        stored_list(store, &key)
    } else {
        let mut list = stored_list(store, "hiddenDevices");
        list.extend(stored_list(store, "disabledDevices"));
        list.sort();
        list.dedup();
        list
    }
}
fn showing(store: &Value, direction: Direction) -> bool {
    let key = show_key(direction);
    if store.get(&key).is_some() {
        store_bool(store, &key)
    } else {
        store_bool(store, "showHiddenDevices")
    }
}

// The legacy Raycast LocalStorage shim stores a flat string map in the package
// directory. Import only audio-device keys once; never mutate that file.
async fn store_with_migration(ctx: &CoreContext) -> CoreResult {
    let store = v2_sdk::read_store(ctx, SLUG).await?;
    if store["_audioDeviceV2Imported"] == true {
        return Ok(store);
    }
    let paths = [
        ctx.paths
            .data_dir
            .join("extensions/audio-device/.tezbar-local-storage.json"),
        ctx.paths
            .data_dir
            .join("extensions/packages/raycast.audio-device/.tezbar-local-storage.json"),
    ];
    let legacy = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let Some(path) = paths.into_iter().find(|p| p.is_file()) else {
            return Ok(json!({}));
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if !value.is_object() {
            return Err("Legacy audio storage is not an object".into());
        }
        Ok(value)
    })
    .await
    .map_err(|e| e.to_string())??;
    v2_sdk::update_store(ctx, SLUG, move |data| {
        let target = data.as_object_mut().ok_or("Invalid audio store")?;
        for (key, value) in legacy.as_object().ok_or("Invalid legacy audio store")? {
            let relevant = matches!(
                key.as_str(),
                "hiddenDevices"
                    | "disabledDevices"
                    | "showHiddenDevices"
                    | "hiddenDevicesInput"
                    | "hiddenDevicesOutput"
                    | "showHiddenDevicesInput"
                    | "showHiddenDevicesOutput"
                    | "defaultDeviceUidInput"
                    | "defaultDeviceUidOutput"
                    | "defaultDeviceNameInput"
                    | "defaultDeviceNameOutput"
                    | "deviceOrderInput"
                    | "deviceOrderOutput"
                    | "graceUntil_input"
                    | "graceUntil_output"
            ) || key.starts_with("pinnedVolume_input_")
                || key.starts_with("pinnedVolume_output_");
            if relevant && !target.contains_key(key) {
                target.insert(key.clone(), value.clone());
            }
        }
        // Legacy priority lists designated their first device as the default.
        for direction in [Direction::Input, Direction::Output] {
            let key = default_key(direction);
            if !target.contains_key(&key) {
                let old = if direction == Direction::Input {
                    "deviceOrderInput"
                } else {
                    "deviceOrderOutput"
                };
                let old_value = target.get(old).cloned().unwrap_or(Value::Null);
                let first = old_value
                    .as_str()
                    .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                    .or_else(|| serde_json::from_value::<Vec<String>>(old_value).ok())
                    .and_then(|list| list.into_iter().next());
                if let Some(uid) = first {
                    target.insert(key, json!(uid));
                }
            }
        }
        target.insert("_audioDeviceV2Imported".into(), json!(true));
        Ok(())
    })
    .await
}
fn sorted(mut list: Vec<Device>, current_uid: &str, preferred_uid: Option<&str>) -> Vec<Device> {
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list.sort_by_key(|d| {
        if d.uid == current_uid {
            0
        } else if Some(d.uid.as_str()) == preferred_uid {
            1
        } else {
            2
        }
    });
    list
}
async fn list_view(ctx: &CoreContext, direction: Direction, store: &Value) -> CoreResult {
    let list = devices(ctx, direction).await?;
    let active = current(ctx, direction).await?;
    let preferred = store_string(store, &default_key(direction));
    let hidden_list = hidden(store, direction);
    let show = showing(store, direction);
    let mut actions = Vec::new();
    let mut rows = Vec::new();
    for device in sorted(list, &active.uid, preferred) {
        let is_hidden = hidden_list.contains(&device.uid);
        if is_hidden && !show {
            continue;
        }
        let mut ids = Vec::new();
        let mut add = |verb: &str, title: &str, kind: &str| {
            let id = a(direction, verb, &device.uid);
            ids.push(id.clone());
            actions.push(action(id, title, kind));
        };
        add(
            "select",
            &format!("Set as {} Device", direction.title()),
            "action",
        );
        #[cfg(target_os = "windows")]
        add(
            "communication",
            &format!("Set as {} Communication Device", direction.title()),
            "action",
        );
        #[cfg(target_os = "macos")]
        if muted(ctx, direction, &device.id).await.is_ok() {
            add("mute", "Toggle Mute", "action");
        }
        let level = volume(ctx, direction, &device.id).await.ok().flatten();
        let pinned = pin(store, direction, &device.uid);
        if pinned.is_some() {
            add("unpin", "Unpin Volume", "action");
        } else if level.is_some() {
            add("pin", "Pin Volume", "action");
        }
        add(
            if preferred == Some(device.uid.as_str()) {
                "clear-default"
            } else {
                "default"
            },
            if preferred == Some(device.uid.as_str()) {
                "Clear Default Device"
            } else {
                "Set as Default Device"
            },
            "action",
        );
        add("copy", "Copy Device Name", "copy");
        add(
            "visibility",
            if is_hidden {
                "Show Device"
            } else {
                "Hide Device"
            },
            "action",
        );
        add(
            "show-hidden",
            if show {
                "Hide Hidden Devices"
            } else {
                "Show Hidden Devices"
            },
            "action",
        );
        let mut accessories = Vec::new();
        if let Some(level) = level {
            accessories.push(json!({"text":format!("{level}%")}));
        }
        if let Some(level) = pinned {
            accessories.push(json!({"text":format!("Pinned: {level}%")}));
        }
        if preferred == Some(device.uid.as_str()) {
            accessories.push(json!({"text":"Default"}));
        }
        if device.uid == active.uid {
            accessories.push(json!({"text":"Current"}));
        }
        if is_hidden {
            accessories.push(json!({"text":"Hidden"}));
        }
        if device.is_communication {
            accessories.push(json!({"text":"Communication"}));
        }
        rows.push(json!({"type":"List.Item","props":{"id":device.uid,"title":device.name,"subtitle":device.transport,"accessories":accessories,"actionIds":ids}}));
    }
    if rows.is_empty() {
        let id = format!("audio:{}:show-hidden", direction.word());
        actions.push(action(id.clone(), "Show Hidden Devices", "action"));
        rows.push(json!({"type":"List.Item","props":{"id":"empty","title":"No visible devices","actionIds":[id]}}));
    }
    Ok(v2_sdk::view(
        &list_title(direction),
        json!({"type":"List","children":rows}),
        actions,
    ))
}
async fn volume_view(
    ctx: &CoreContext,
    direction: Direction,
    store: &Value,
    selected_id: Option<&str>,
) -> CoreResult {
    let list = devices(ctx, direction).await?;
    let active = current(ctx, direction).await?;
    let selected = match selected_id {
        Some(id) => list
            .iter()
            .find(|d| d.id == id)
            .ok_or("Selected audio device is no longer available")?,
        None => list.iter().find(|d| d.id == active.id).unwrap_or(&active),
    }.clone();
    let level = volume(ctx, direction, &selected.id).await.ok().flatten();
    let pinned = pin(store, direction, &selected.uid).is_some();
    let options: Vec<Value> = sorted(list, &active.uid, None)
        .into_iter()
        .map(|d| json!({"type":"Form.Dropdown.Item","props":{"value":d.id,"title":d.name}}))
        .collect();
    Ok(v2_sdk::view(
        &format!("Set {} Volume", direction.title()),
        json!({"type":"Form","children":[
            {"type":"Form.Dropdown","props":{"id":"device","title":format!("{} Device",direction.title()),"value":selected.id,"actionId":format!("audio:{}:device-change",direction.word())},"children":options},
            {"type":"Form.Description","props":{"title":format!("Current {} Volume",direction.title()),"text":level.map(|n| format!("{n}%")).unwrap_or_else(|| "Unknown".into())}},
            {"type":"Form.TextField","props":{"id":"level","title":format!("New {} Volume",direction.title()),"placeholder":"0-100","value":level.map(|n| n.to_string()).unwrap_or_default()}},
            {"type":"Form.Checkbox","props":{"id":"pinVolume","label":"Pin Volume","value":pinned}}
        ]}),
        vec![action(
            format!("audio:{}:submit", direction.word()),
            &format!("Set {} Volume", direction.title()),
            "submit-form",
        )],
    ))
}

async fn set_grace(ctx: &CoreContext, direction: Direction) -> Result<(), String> {
    let until = crate::native_core::common::now_ms()
        .saturating_add(60_000)
        .to_string();
    let key = grace_key(direction);
    v2_sdk::update_store(ctx, SLUG, move |store| {
        store[key] = json!(until);
        Ok(())
    })
    .await?;
    Ok(())
}
async fn select(
    ctx: &CoreContext,
    direction: Direction,
    device: &Device,
    prefs: &Value,
) -> Result<(), String> {
    switch(
        ctx,
        direction,
        &device.id,
        prefs["systemOutput"].as_bool().unwrap_or(false),
    )
    .await?;
    set_grace(ctx, direction).await
}
fn find_device<'a>(list: &'a [Device], uid: &str) -> Result<&'a Device, String> {
    list.iter()
        .find(|d| d.uid == uid || d.id == uid)
        .ok_or_else(|| format!("Audio device {uid} is no longer available"))
}
fn prefs<'a>(payload: &'a Value, command: &str) -> &'a Value {
    let p = &payload["preferences"];
    if p["commands"][command].is_object() {
        &p["commands"][command]
    } else {
        p
    }
}
async fn enforce(
    ctx: &CoreContext,
    direction: Direction,
    store: &Value,
    preferences: &Value,
) -> CoreResult {
    let list = devices(ctx, direction).await?;
    let mut switched = false;
    let mut adjusted = 0usize;
    let mut pinned = 0usize;
    let grace = store_string(store, &grace_key(direction))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if crate::native_core::common::now_ms() >= grace {
        if let Some(uid) = store_string(store, &default_key(direction)) {
            if let Some(device) = list.iter().find(|d| d.uid == uid) {
                if current(ctx, direction).await?.uid != uid {
                    switch(
                        ctx,
                        direction,
                        &device.id,
                        preferences["systemOutput"].as_bool().unwrap_or(false),
                    )
                    .await?;
                    switched = true;
                }
            }
        }
    }
    for device in &list {
        if let Some(target) = pin(store, direction, &device.uid) {
            pinned += 1;
            if let Some(actual) = volume(ctx, direction, &device.id).await? {
                if (i16::from(actual) - i16::from(target)).abs() >= 2 {
                    set_volume(ctx, direction, &device.id, target).await?;
                    adjusted += 1;
                }
            }
        }
    }
    let message = if store_string(store, &default_key(direction)).is_none() && pinned == 0 {
        format!(
            "No default {} device or pinned volumes configured",
            direction.word()
        )
    } else {
        format!(
            "Checked {} device: {} switch, {} of {} pinned volumes adjusted",
            direction.word(),
            if switched { 1 } else { 0 },
            adjusted,
            pinned
        )
    };
    Ok(v2_sdk::no_view(&message))
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err(format!("Unsupported audio-device command: {command}"));
    }
    if operation == "extension:dispose-session" {
        return Ok(v2_sdk::unchanged());
    }
    // The native host may refresh visible commands every 500 ms. Device listing
    // and per-device volume reads are external process calls; keep them below one
    // enumeration per two seconds for a session. Background enforcement is a
    // one-shot command until the host configures its own schedule.
    if operation == "extension:refresh-session" {
        if command == "auto-switch-input" || command == "auto-switch-output" {
            return Ok(v2_sdk::unchanged());
        }
        let now = crate::native_core::common::now_ms();
        if state["audioLastEnumeratedAt"]
            .as_u64()
            .is_some_and(|last| now.saturating_sub(last) < 2_000)
        {
            return Ok(v2_sdk::unchanged());
        }
        state["audioLastEnumeratedAt"] = json!(now);
    } else if operation == "extension:run-command" {
        state["audioLastEnumeratedAt"] = json!(crate::native_core::common::now_ms());
    }
    let store = store_with_migration(ctx).await?;
    let preferences = prefs(payload, command);
    match operation {
        "extension:run-command"
        | "extension:refresh-session"
        | "extension:search-text-changed"
        | "extension:load-more" => match command {
            "set-input-device" | "set-output-device" => {
                let direction = if command == "set-input-device" {
                    Direction::Input
                } else {
                    Direction::Output
                };
                if operation == "extension:run-command" {
                    let args = &payload["argumentValues"];
                    let context = &payload["launchContext"];
                    let id = args["deviceId"]
                        .as_str()
                        .or_else(|| context["deviceId"].as_str());
                    let name = args["deviceName"]
                        .as_str()
                        .or_else(|| context["deviceName"].as_str());
                    if id.is_some() || name.is_some() {
                        let list = devices(ctx, direction).await?;
                        let device = list
                            .iter()
                            .find(|d| Some(d.id.as_str()) == id)
                            .or_else(|| list.iter().find(|d| Some(d.name.as_str()) == name))
                            .ok_or("Requested audio device was not found")?;
                        select(ctx, direction, device, preferences).await?;
                    }
                }
                list_view(ctx, direction, &store).await
            }
            "set-volume" => {
                volume_view(
                    ctx,
                    Direction::Output,
                    &store,
                    state["volumeDevice"].as_str(),
                )
                .await
            }
            "set-input-volume" => {
                volume_view(
                    ctx,
                    Direction::Input,
                    &store,
                    state["volumeDevice"].as_str(),
                )
                .await
            }
            "auto-switch-input" => enforce(ctx, Direction::Input, &store, preferences).await,
            "auto-switch-output" => enforce(ctx, Direction::Output, &store, preferences).await,
            "favourite" | "toggle-favourites" => {
                if operation != "extension:run-command" {
                    return Ok(v2_sdk::unchanged());
                }
                let first = preferences["favourite"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("No favourite output audio device specified")?;
                let list = devices(ctx, Direction::Output).await?;
                let target = if command == "toggle-favourites"
                    && current(ctx, Direction::Output).await?.name == first
                {
                    preferences["favourite2"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(first)
                } else {
                    first
                };
                let device = list
                    .iter()
                    .find(|d| d.name == target)
                    .ok_or_else(|| format!("Device \"{target}\" not found"))?;
                select(ctx, Direction::Output, device, preferences).await?;
                Ok(v2_sdk::no_view(&format!(
                    "Active output audio device set to {}",
                    device.name
                )))
            }
            "use-combo1" | "use-combo2" | "use-combo3" => {
                if operation != "extension:run-command" {
                    return Ok(v2_sdk::unchanged());
                }
                let input = preferences["input"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Combo input device is not configured")?;
                let output = preferences["output"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Combo output device is not configured")?;
                let inputs = devices(ctx, Direction::Input).await?;
                let outputs = devices(ctx, Direction::Output).await?;
                let input_device = inputs
                    .iter()
                    .find(|d| d.name == input)
                    .ok_or_else(|| format!("Input device \"{input}\" not found"))?;
                let output_device = outputs
                    .iter()
                    .find(|d| d.name == output)
                    .ok_or_else(|| format!("Output device \"{output}\" not found"))?;
                select(ctx, Direction::Input, input_device, preferences).await?;
                select(ctx, Direction::Output, output_device, preferences).await?;
                Ok(v2_sdk::no_view(&format!(
                    "{} in use",
                    preferences["comboName"].as_str().unwrap_or("Combo")
                )))
            }
            "customize-order" => {
                let redirected = match state["viewDirection"].as_str() {
                    Some("input") => Some(Direction::Input),
                    Some("output") => Some(Direction::Output),
                    _ => None,
                };
                if let Some(direction) = redirected {
                    list_view(ctx, direction, &store).await
                } else {
                    Ok(v2_sdk::view(
                        "Customize Order (Replaced)",
                        json!({"type":"Detail","props":{"markdown":"# Command Replaced\n\nUse Set Output Device or Set Input Device and choose Set as Default Device."}}),
                        vec![
                            action(
                                "audio:open-output".into(),
                                "Open Set Output Device",
                                "action",
                            ),
                            action("audio:open-input".into(), "Open Set Input Device", "action"),
                        ],
                    ))
                }
            }
            _ => Err("Unknown audio command".into()),
        },
        "extension:invoke-action" => {
            let id = payload["actionId"].as_str().ok_or("Missing action ID")?;
            if command == "customize-order" {
                let redirect = match id {
                    "audio:open-output" => Some(Direction::Output),
                    "audio:open-input" => Some(Direction::Input),
                    _ => None,
                };
                if let Some(direction) = redirect {
                    state["viewDirection"] = json!(direction.word());
                    return list_view(ctx, direction, &store).await;
                }
            }
            let direction = match command {
                "set-input-device" | "set-input-volume" => Direction::Input,
                "set-output-device" | "set-volume" => Direction::Output,
                "customize-order" if state["viewDirection"] == "input" => Direction::Input,
                "customize-order" if state["viewDirection"] == "output" => Direction::Output,
                _ => return Err("This command has no actions".into()),
            };
            if id == format!("audio:{}:device-change", direction.word())
                && (command == "set-volume" || command == "set-input-volume")
            {
                let chosen = payload["formValues"]["value"]
                    .as_str()
                    .ok_or("Missing audio device selection")?;
                state["volumeDevice"] = json!(chosen);
                return volume_view(ctx, direction, &store, Some(chosen)).await;
            }
            if id == format!("audio:{}:submit", direction.word())
                && (command == "set-volume" || command == "set-input-volume")
            {
                let values = &payload["formValues"];
                let device_id = values["device"].as_str().ok_or("Choose an audio device")?;
                let list = devices(ctx, direction).await?;
                let device = list
                    .iter()
                    .find(|d| d.id == device_id)
                    .ok_or("Selected audio device is no longer available")?;
                let raw = values["level"]
                    .as_str()
                    .ok_or("Enter a volume from 0 to 100")?;
                let level = raw
                    .trim()
                    .parse::<i16>()
                    .map_err(|_| "Enter a volume from 0 to 100")?
                    .clamp(0, 100) as u8;
                #[cfg(target_os = "macos")]
                if level > 0 {
                    let _ = set_mute(ctx, direction, &device.id, false).await;
                }
                set_volume(ctx, direction, &device.id, level).await?;
                let key = pin_key(direction, &device.uid);
                let pinned = values["pinVolume"].as_bool().unwrap_or(false);
                v2_sdk::update_store(ctx, SLUG, move |store| {
                    if pinned {
                        store[&key] = json!(level.to_string());
                    } else {
                        store
                            .as_object_mut()
                            .ok_or("Invalid audio store")?
                            .remove(&key);
                    }
                    Ok(())
                })
                .await?;
                state["volumeDevice"] = json!(device_id);
                return volume_view(
                    ctx,
                    direction,
                    &v2_sdk::read_store(ctx, SLUG).await?,
                    Some(device_id),
                )
                .await;
            }
            if id == format!("audio:{}:show-hidden", direction.word()) {
                let key = show_key(direction);
                v2_sdk::update_store(ctx, SLUG, move |store| {
                    store[key] = json!("true");
                    Ok(())
                })
                .await?;
                return list_view(ctx, direction, &v2_sdk::read_store(ctx, SLUG).await?).await;
            }
            let mut parts = id.splitn(4, ':');
            if parts.next() != Some("audio") || parts.next() != Some(direction.word()) {
                return Err("Unknown audio action".into());
            }
            let verb = parts.next().ok_or("Unknown audio action")?;
            let uid = parts.next().ok_or("Unknown audio action")?;
            let list = devices(ctx, direction).await?;
            let device = find_device(&list, uid)?;
            match verb {
                "select" => select(ctx, direction, device, preferences).await?,
                "default" => {
                    let key = default_key(direction);
                    let name = name_key(direction);
                    let uid = device.uid.clone();
                    let label = device.name.clone();
                    select(ctx, direction, device, preferences).await?;
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        store[key] = json!(uid);
                        store[name] = json!(label);
                        Ok(())
                    })
                    .await?;
                }
                "clear-default" => {
                    let key = default_key(direction);
                    let name = name_key(direction);
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        let map = store.as_object_mut().ok_or("Invalid audio store")?;
                        map.remove(&key);
                        map.remove(&name);
                        Ok(())
                    })
                    .await?;
                }
                "pin" => {
                    let level = volume(ctx, direction, &device.id)
                        .await?
                        .ok_or("Cannot read current volume")?;
                    let key = pin_key(direction, &device.uid);
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        store[key] = json!(level.to_string());
                        Ok(())
                    })
                    .await?;
                }
                "unpin" => {
                    let key = pin_key(direction, &device.uid);
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        store
                            .as_object_mut()
                            .ok_or("Invalid audio store")?
                            .remove(&key);
                        Ok(())
                    })
                    .await?;
                }
                "visibility" => {
                    let key = hidden_key(direction);
                    let uid = device.uid.clone();
                    let mut values = hidden(&store, direction);
                    if values.contains(&uid) {
                        values.retain(|v| v != &uid);
                    } else {
                        values.push(uid);
                    }
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        store[key] = json!(values);
                        Ok(())
                    })
                    .await?;
                }
                "show-hidden" => {
                    let key = show_key(direction);
                    let show = !showing(&store, direction);
                    v2_sdk::update_store(ctx, SLUG, move |store| {
                        store[key] = json!(show.to_string());
                        Ok(())
                    })
                    .await?;
                }
                "copy" => {
                    v2_sdk::copy(ctx, &device.name).await?;
                }
                "mute" => {
                    #[cfg(target_os = "macos")]
                    {
                        set_mute(
                            ctx,
                            direction,
                            &device.id,
                            !muted(ctx, direction, &device.id).await?,
                        )
                        .await?;
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        return Err("Per-device mute is unavailable on this platform".into());
                    }
                }
                "communication" => {
                    #[cfg(target_os = "windows")]
                    {
                        helper(
                            ctx,
                            false,
                            &[
                                if direction == Direction::Input {
                                    "switch-input-communication"
                                } else {
                                    "switch-output-communication"
                                },
                                "--id",
                                &device.id,
                            ],
                        )
                        .await?;
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        return Err("Communication device selection is Windows-only".into());
                    }
                }
                _ => return Err("Unknown audio action".into()),
            }
            list_view(ctx, direction, &v2_sdk::read_store(ctx, SLUG).await?).await
        }
        _ => Err(format!("Unsupported audio operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_helper_is_an_explicit_error() {
        use crate::native_core::common::CorePaths;
        let root = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: root.path().join("data"),
            config_dir: root.path().join("config"),
            resource_dir: root.path().join("resources"),
            temp_dir: root.path().join("temp"),
            home_dir: root.path().join("home"),
        });
        assert!(asset_path(&ctx, "audio-devices")
            .unwrap_err()
            .contains("missing"));
    }
    #[test]
    fn helper_hash_mismatch_blocks_execution() {
        assert!(verify_helper_bytes("win-audio-cli.exe", b"fake fixture")
            .unwrap_err()
            .contains("checksum mismatch"));
        assert!(verify_helper_bytes("sound-control", b"fake fixture")
            .unwrap_err()
            .contains("checksum mismatch"));
    }
    #[test]
    fn parses_native_device_fixtures_and_preserves_current_order() {
        let raw = json!([{"id":"42","uid":"usb-42","name":"USB Mic","transportType":"usb"},{"id":"1","uid":"built-in","name":"Built-in Mic"}]);
        let list = parse_devices(&raw, Direction::Input).unwrap();
        let result = sorted(list, "built-in", Some("usb-42"));
        assert_eq!(
            result.iter().map(|d| d.uid.as_str()).collect::<Vec<_>>(),
            vec!["built-in", "usb-42"]
        );
    }
    #[test]
    fn legacy_hidden_and_pinned_keys_are_understood_without_reading_profile() {
        let store = json!({"hiddenDevices":"[\"a\"]","disabledDevices":"[\"b\"]","showHiddenDevices":"true","pinnedVolume_output_a":"61","future":"keep"});
        assert_eq!(hidden(&store, Direction::Output), vec!["a", "b"]);
        assert!(showing(&store, Direction::Output));
        assert_eq!(pin(&store, Direction::Output, "a"), Some(61));
    }
    #[test]
    fn malformed_device_output_is_an_error() {
        assert!(parse_devices(&json!({"outputs":[{"name":"No ID"}]}), Direction::Output).is_err());
    }

    #[tokio::test]
    async fn migrates_only_audio_keys_from_fake_legacy_storage() {
        use crate::native_core::common::CorePaths;
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("extensions/audio-device");
        std::fs::create_dir_all(&package).unwrap();
        let source = json!({
            "deviceOrderOutput":"[\"usb-speaker\",\"built-in\"]",
            "pinnedVolume_output_usb-speaker":"48",
            "hiddenDevicesOutput":"[\"built-in\"]",
            "unrelatedSecret":"do-not-migrate"
        });
        let source_bytes = serde_json::to_vec(&source).unwrap();
        std::fs::write(package.join(".tezbar-local-storage.json"), &source_bytes).unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: root.path().to_path_buf(),
            config_dir: root.path().join("config"),
            resource_dir: root.path().join("resources"),
            temp_dir: root.path().join("temp"),
            home_dir: root.path().join("home"),
        });
        let imported = store_with_migration(&ctx).await.unwrap();
        assert_eq!(imported["defaultDeviceUidOutput"], "usb-speaker");
        assert_eq!(pin(&imported, Direction::Output, "usb-speaker"), Some(48));
        assert!(imported.get("unrelatedSecret").is_none());
        assert_eq!(
            std::fs::read(package.join(".tezbar-local-storage.json")).unwrap(),
            source_bytes
        );
        assert_eq!(store_with_migration(&ctx).await.unwrap(), imported);
    }
}
