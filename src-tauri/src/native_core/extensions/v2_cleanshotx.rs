//! Native V2 port of Raycast CleanShot X (MIT), by Aayush9029 and the
//! contributors named in its installed package.json. Behavioral source:
//! cleanshotx/.sc-build/{command,presets,manage-recording-presets}.js.
//! No bundled JavaScript is executed. CleanShot-specific macOS operations use
//! its original URL integration; Windows capture uses the native core and OS UI.
use super::v2_sdk;
use crate::native_core::common::{CoreContext, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SLUG: &str = "cleanshotx";
const CUSTOM: &str = "custom-presets";
const MAX_DIMENSION: u32 = 32768;

pub fn commands() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    return &[
        "open-history",
        "open-settings",
        "view-uploads",
        "all-in-one",
        "capture-fullscreen",
        "capture-area",
        "capture-previous-area",
        "capture-window",
        "self-timer",
        "scrolling-capture",
        "record-screen",
        "capture-text",
        "annotate",
        "open-from-clipboard",
        "toggle-desktop-icons",
        "hide-desktop-icons",
        "show-desktop-icons",
        "restore-recently-closed",
        "add-quick-access-overlay",
        "pin",
        "record-screen-preset",
        "manage-recording-presets",
    ];
    #[cfg(target_os = "windows")]
    return &[
        "open-history",
        "view-uploads",
        "all-in-one",
        "capture-fullscreen",
        "capture-area",
        "capture-window",
        "self-timer",
        "annotate",
        "open-from-clipboard",
        "toggle-desktop-icons",
        "hide-desktop-icons",
        "show-desktop-icons",
        "manage-recording-presets",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return &["view-uploads", "manage-recording-presets"];
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Preset {
    id: String,
    name: String,
    width: u32,
    height: u32,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn defaults() -> Vec<Preset> {
    [
        ("default-fullscreen", "Fullscreen", 0, 0),
        ("default-75", "75% of Screen", 0, 0),
        ("default-50", "50% of Screen", 0, 0),
        ("default-1080p", "1080p", 1920, 1080),
        ("default-720p", "720p", 1280, 720),
        ("default-4k", "4K", 3840, 2160),
        ("default-ig-story", "Instagram Story", 1080, 1920),
        ("default-yt-shorts", "YouTube Shorts", 1080, 1920),
    ]
    .into_iter()
    .map(|(id, name, width, height)| Preset {
        id: id.into(),
        name: name.into(),
        width,
        height,
        extra: BTreeMap::new(),
    })
    .collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn custom_presets(store: &Value) -> Result<Vec<Preset>, String> {
    let Some(value) = store.get(CUSTOM) else {
        return Ok(Vec::new());
    };
    let presets: Vec<Preset> = serde_json::from_value(value.clone())
        .map_err(|e| format!("Invalid native recording preset store: {e}"))?;
    if presets.len() > 256 {
        return Err("Too many custom recording presets (maximum 256)".into());
    }
    let mut ids = std::collections::HashSet::new();
    for p in &presets {
        if !valid_id(&p.id)
            || p.id.starts_with("default-")
            || !ids.insert(&p.id)
            || p.name.trim().is_empty()
            || p.name.len() > 160
            || p.name.chars().any(char::is_control)
            || p.width == 0
            || p.height == 0
            || p.width > MAX_DIMENSION
            || p.height > MAX_DIMENSION
        {
            return Err("Invalid or duplicate custom recording preset".into());
        }
    }
    Ok(presets)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Display {
    width: u32,
    height: u32,
}

fn resolved(preset: &Preset, screen: Display) -> (u32, u32) {
    let divisor = match preset.id.as_str() {
        "default-fullscreen" => Some((1, 1)),
        "default-75" => Some((3, 4)),
        "default-50" => Some((1, 2)),
        _ => None,
    };
    match divisor {
        Some((a, b)) => (
            (screen.width * a + b / 2) / b,
            (screen.height * a + b / 2) / b,
        ),
        None => (preset.width, preset.height),
    }
}

fn record_url(preset: &Preset, screen: Display, index: usize) -> Result<(String, bool), String> {
    if screen.width == 0 || screen.height == 0 || index > 63 {
        return Err("Invalid recording display".into());
    }
    let (w, h) = resolved(preset, screen);
    if w == 0 || h == 0 {
        return Err("Recording dimensions must be positive".into());
    }
    // JavaScript Math.round on a nonnegative half is equivalent to (n + 1) / 2.
    let x = screen.width.saturating_sub(w).div_ceil(2);
    let y = screen.height.saturating_sub(h).div_ceil(2);
    Ok((
        format!(
            "cleanshot://record-screen?x={x}&y={y}&width={w}&height={h}&display={}",
            index + 1
        ),
        w > screen.width || h > screen.height,
    ))
}

#[cfg(target_os = "macos")]
fn displays() -> Result<Vec<Display>, String> {
    use core_graphics::display::CGDisplay;
    let mut ids =
        CGDisplay::active_displays().map_err(|e| format!("Cannot enumerate displays: {e}"))?;
    ids.sort_by_key(|id| !CGDisplay::new(*id).is_main());
    let result: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let size = CGDisplay::new(id).bounds().size;
            Display {
                width: size.width.round() as u32,
                height: size.height.round() as u32,
            }
        })
        .collect();
    if result.is_empty() || result.iter().any(|d| d.width == 0 || d.height == 0) {
        return Err("No usable recording displays are available".into());
    }
    Ok(result)
}

#[cfg(not(target_os = "macos"))]
fn displays() -> Result<Vec<Display>, String> {
    Err("Preset screen recording requires CleanShot X on macOS; no native Windows recorder is configured".into())
}

fn string_arg<'a>(payload: &'a Value, name: &str) -> Result<Option<&'a str>, String> {
    let value = payload
        .get("argumentValues")
        .or_else(|| payload.get("arguments"))
        .and_then(|a| a.get(name));
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) if !s.chars().any(char::is_control) => Ok(Some(s)),
        _ => Err(format!("Invalid {name} argument")),
    }
}

fn mac_url(command: &str, payload: &Value, paths: &[String]) -> Result<Vec<String>, String> {
    let action = string_arg(payload, "action")?;
    if action.is_some_and(|a| !["copy", "save", "annotate", "upload", "pin"].contains(&a)) {
        return Err("Capture action must be copy, save, annotate, upload, or pin".into());
    }
    let base = match command {
        "view-uploads" => return Ok(vec!["https://cleanshot.cloud/".into()]),
        "annotate" => "open-annotate",
        "open-history"
        | "open-settings"
        | "all-in-one"
        | "capture-fullscreen"
        | "capture-area"
        | "capture-previous-area"
        | "capture-window"
        | "self-timer"
        | "scrolling-capture"
        | "record-screen"
        | "capture-text"
        | "open-from-clipboard"
        | "toggle-desktop-icons"
        | "hide-desktop-icons"
        | "show-desktop-icons"
        | "restore-recently-closed"
        | "add-quick-access-overlay"
        | "pin" => command,
        "capture-area-raycast-aichat" => {
            return Err(
                "Raycast AI Chat integration requires Raycast Pro and is unavailable in Tezbar"
                    .into(),
            )
        }
        _ => return Err(format!("Unknown CleanShot X command: {command}")),
    };
    let url = format!("cleanshot://{base}");
    if command == "open-settings" {
        return match string_arg(payload, "tab")? {
            Some(tab)
                if [
                    "general",
                    "wallpaper",
                    "shortcuts",
                    "quickaccess",
                    "recording",
                    "screenshots",
                    "annotate",
                    "cloud",
                    "advanced",
                    "about",
                ]
                .contains(&tab) =>
            {
                Ok(vec![format!("{url}?tab={tab}")])
            }
            Some(_) => Err("Invalid CleanShot settings tab".into()),
            None => Ok(vec![url]),
        };
    }
    if [
        "capture-fullscreen",
        "capture-area",
        "capture-previous-area",
        "capture-window",
        "self-timer",
    ]
    .contains(&command)
    {
        return Ok(vec![match action {
            Some(a) => format!("{url}?action={a}"),
            None => url,
        }]);
    }
    if ["annotate", "pin", "add-quick-access-overlay"].contains(&command) {
        if paths.is_empty() && command == "add-quick-access-overlay" {
            return Err("No file selected. Provide a file path or select a file in Finder".into());
        }
        if !paths.is_empty() {
            return paths
                .iter()
                .map(|p| {
                    // These URLs always target macOS, even when their pure builder
                    // is tested on Windows.
                    if !p.starts_with('/') || p.chars().any(char::is_control) {
                        return Err("CleanShot file paths must be absolute macOS paths".into());
                    }
                    Ok(format!("{url}?filepath={}", v2_sdk::encode_component(p)))
                })
                .collect();
        }
    }
    Ok(vec![url])
}

fn validate_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.chars().any(char::is_control) || !Path::new(path).is_absolute() {
        return Err("File path must be an absolute local path without control characters".into());
    }
    Ok(())
}

async fn open_urls(ctx: &CoreContext, urls: &[String]) -> CoreResult {
    ctx.request("core:platform:visibility", vec![json!(false)])
        .await?;
    for url in urls {
        v2_sdk::open(ctx, url).await?;
    }
    Ok(v2_sdk::no_view("Opened CleanShot X"))
}

#[cfg(target_os = "macos")]
async fn mac_installed(ctx: &CoreContext) -> Result<(), String> {
    for p in [
        PathBuf::from("/Applications/CleanShot X.app"),
        ctx.paths.home_dir.join("Applications/CleanShot X.app"),
    ] {
        if p.is_dir() {
            return Ok(());
        }
    }
    // Spotlight lookup can find relocated applications without opening/revealing
    // anything. Missing/disabled Spotlight never turns into a false success.
    let found = native_output(
        "/usr/bin/mdfind",
        &["-0", "kMDItemContentType == 'com.apple.application-bundle' && (kMDItemFSName == 'CleanShot X.app' || kMDItemDisplayName == 'CleanShot X')"],
    ).await.unwrap_or_default();
    if found
        .split('\0')
        .any(|path| !path.is_empty() && Path::new(path).is_dir())
    {
        Ok(())
    } else {
        Err("CleanShot X was not found in Applications or Spotlight. Install it to use this macOS integration".into())
    }
}

#[cfg(target_os = "macos")]
async fn finder_paths() -> Result<Vec<String>, String> {
    // Original extension treats Finder lookup failure as no selection. Never
    // activate Finder, launch applications, or run JavaScript to obtain paths.
    let script = "tell application \"System Events\" to set finderRunning to exists process \"Finder\"\nif not finderRunning then return \"\"\ntell application \"Finder\"\nset paths to {}\nrepeat with itemRef in (get selection)\nset end of paths to POSIX path of (itemRef as alias)\nend repeat\nset AppleScript's text item delimiters to character id 0\nreturn paths as text\nend tell";
    Ok(native_output("/usr/bin/osascript", &["-e", script])
        .await
        .unwrap_or_default()
        .trim_end_matches('\n')
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

async fn native_output(program: &str, args: &[&str]) -> Result<String, String> {
    let mut process = tokio::process::Command::new(program);
    process.args(args).kill_on_drop(true);
    #[cfg(target_os = "windows")]
    process.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(10), process.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn action(id: impl Into<String>, title: impl Into<String>) -> Value {
    json!({"id":id.into(),"title":title.into(),"kind":"action","style":"default"})
}

fn row(id: &str, title: &str, subtitle: &str, actions: &[Value]) -> Value {
    json!({"type":"List.Item","props":{"id":id,"title":title,"subtitle":subtitle,
        "actionIds":actions.iter().map(|a|a["id"].clone()).collect::<Vec<_>>()}})
}

async fn preset_view(ctx: &CoreContext, state: &Value) -> CoreResult {
    let store = v2_sdk::read_store(ctx, SLUG).await?;
    let custom = custom_presets(&store)?;
    let manage = state["page"] == "manage";
    let display_list: Vec<Display> =
        serde_json::from_value(state["displays"].clone()).unwrap_or_default();
    let query = state["query"].as_str().unwrap_or("").to_lowercase();
    let mut sections = Vec::new();
    let mut all_actions = Vec::new();
    for (name, items) in [("Custom Presets", custom), ("Default Presets", defaults())] {
        let mut children = Vec::new();
        let count = items.len();
        for (index, p) in items.into_iter().enumerate() {
            if !p.name.to_lowercase().contains(&query) {
                continue;
            }
            let mut actions = Vec::new();
            let subtitle = if p.width == 0 {
                display_list
                    .first()
                    .map(|d| {
                        let (w, h) = resolved(&p, *d);
                        format!("{w} × {h}")
                    })
                    .unwrap_or_else(|| "Relative".into())
            } else {
                format!("{} × {}", p.width, p.height)
            };
            if manage {
                if !p.id.starts_with("default-") {
                    actions.push(action(format!("cleanshotx:edit:{}", p.id), "Edit Preset"));
                    let mut delete = action(format!("cleanshotx:delete:{}", p.id), "Delete Preset");
                    delete["style"] = json!("destructive");
                    actions.push(delete);
                    if index > 0 {
                        actions.push(action(format!("cleanshotx:up:{}", p.id), "Move Up"));
                    }
                    if index + 1 < count {
                        actions.push(action(format!("cleanshotx:down:{}", p.id), "Move Down"));
                    }
                }
                actions.push(action("cleanshotx:new", "Create New Preset"));
            } else {
                for d in 0..display_list.len() {
                    actions.push(action(
                        format!("cleanshotx:record:{}:{d}", p.id),
                        if d == 0 {
                            "Record".into()
                        } else {
                            format!("Record on Display {}", d + 1)
                        },
                    ));
                }
                actions.push(action("cleanshotx:manage", "Manage Presets"));
            }
            let mut item = row(&p.id, &p.name, &subtitle, &actions);
            if !manage
                && display_list
                    .iter()
                    .any(|d| record_url(&p, *d, 0).map(|r| r.1).unwrap_or(true))
            {
                item["props"]["accessories"] = json!([{"tag":"Exceeds screen"}]);
            }
            all_actions.extend(actions);
            children.push(item);
        }
        sections.push(json!({"type":"List.Section","props":{"title":name},"children":children}));
    }
    if manage {
        let actions = vec![action("cleanshotx:new", "Create New Preset")];
        sections.push(row("create-preset", "Create New Preset", "", &actions));
        all_actions.extend(actions);
        if state["entry"] == "record-screen-preset" {
            let back = vec![action("cleanshotx:back", "Back to Recording Presets")];
            sections.push(row(
                "back-to-recording",
                "Back to Recording Presets",
                "",
                &back,
            ));
            all_actions.extend(back);
        }
    }
    Ok(v2_sdk::view(
        if manage {
            "Manage Recording Presets"
        } else {
            "Record Screen at Preset"
        },
        json!({"type":"List","props":{"searchBarPlaceholder":"Search presets…"},"children":sections}),
        all_actions,
    ))
}

fn preset_form(preset: Option<&Preset>) -> Value {
    let mut submit = action(
        "cleanshotx:save",
        if preset.is_some() { "Save" } else { "Create" },
    );
    submit["kind"] = json!("submit-form");
    v2_sdk::view(
        if preset.is_some() {
            "Edit Preset"
        } else {
            "New Preset"
        },
        json!({"type":"Form","children":[
            {"type":"Form.TextField","props":{"id":"name","title":"Name","value":preset.map(|p|p.name.clone()).unwrap_or_default()}},
            {"type":"Form.TextField","props":{"id":"width","title":"Width","placeholder":"1920","value":preset.map(|p|p.width.to_string()).unwrap_or_default()}},
            {"type":"Form.TextField","props":{"id":"height","title":"Height","placeholder":"1080","value":preset.map(|p|p.height.to_string()).unwrap_or_default()}}
        ]}),
        vec![submit, action("cleanshotx:cancel", "Cancel")],
    )
}

fn form_preset(values: &Value, id: String) -> Result<Preset, String> {
    let name = values["name"].as_str().ok_or("Name is required")?.trim();
    if name.is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
        return Err("Name must contain 1–160 bytes without control characters".into());
    }
    let dimension = |key: &str| -> Result<u32, String> {
        let v = &values[key];
        let n = if let Some(s) = v.as_str() {
            s.trim().parse::<u32>().ok()
        } else {
            v.as_u64().and_then(|n| u32::try_from(n).ok())
        };
        n.filter(|n| *n > 0 && *n <= MAX_DIMENSION)
            .ok_or_else(|| format!("{key} must be an integer from 1 to {MAX_DIMENSION}"))
    };
    Ok(Preset {
        id,
        name: name.into(),
        width: dimension("width")?,
        height: dimension("height")?,
        extra: BTreeMap::new(),
    })
}

async fn mutate_presets(
    ctx: &CoreContext,
    operation: &str,
    id: &str,
    new: Option<Preset>,
) -> CoreResult {
    let operation = operation.to_owned();
    let id = id.to_owned();
    v2_sdk::update_store(ctx, SLUG, move |store| {
        let mut presets = custom_presets(store)?;
        let index = presets.iter().position(|p| p.id == id);
        match operation.as_str() {
            "create" => {
                if presets.len() >= 256 || index.is_some() {
                    return Err("Preset limit or duplicate preset ID".into());
                }
                presets.push(new.ok_or("Missing preset")?);
            }
            "save" => {
                let p = &mut presets[index.ok_or("Preset no longer exists")?];
                let new = new.ok_or("Missing preset")?;
                p.name = new.name;
                p.width = new.width;
                p.height = new.height;
            }
            "delete" => {
                presets.remove(index.ok_or("Preset no longer exists")?);
            }
            "up" | "down" => {
                let i = index.ok_or("Preset no longer exists")?;
                let other = if operation == "up" {
                    i.checked_sub(1)
                } else {
                    i.checked_add(1).filter(|n| *n < presets.len())
                }
                .ok_or("Preset is already at the edge")?;
                presets.swap(i, other);
            }
            _ => return Err("Unknown preset operation".into()),
        }
        store[CUSTOM] = serde_json::to_value(presets).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

async fn preset_action(ctx: &CoreContext, payload: &Value, state: &mut Value) -> CoreResult {
    let id = payload["actionId"]
        .as_str()
        .ok_or("Missing CleanShot action ID")?;
    match id {
        "cleanshotx:manage" => {
            state["page"] = json!("manage");
            state["query"] = json!("");
        }
        "cleanshotx:back" if state["entry"] == "record-screen-preset" => {
            state["page"] = json!("record");
            state["query"] = json!("");
        }
        "cleanshotx:new" if state["page"] == "manage" => {
            state["page"] = json!("form");
            state["editing"] = Value::Null;
            return Ok(preset_form(None));
        }
        "cleanshotx:cancel" => {
            state["page"] = json!("manage");
            state["editing"] = Value::Null;
            state["deleting"] = Value::Null;
        }
        "cleanshotx:save" if state["page"] == "form" => {
            let editing = state["editing"].as_str().map(str::to_owned);
            let p = form_preset(
                &payload["formValues"],
                editing
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            )?;
            let preset_id = p.id.clone();
            mutate_presets(
                ctx,
                if editing.is_some() { "save" } else { "create" },
                &preset_id,
                Some(p),
            )
            .await?;
            state["page"] = json!("manage");
            state["editing"] = Value::Null;
        }
        "cleanshotx:confirm-delete" if state["page"] == "confirm-delete" => {
            let deleting = state["deleting"]
                .as_str()
                .ok_or("Missing preset to delete")?
                .to_owned();
            mutate_presets(ctx, "delete", &deleting, None).await?;
            state["page"] = json!("manage");
            state["deleting"] = Value::Null;
        }
        _ => {
            let parts: Vec<_> = id.split(':').collect();
            if parts.len() < 3 || parts[0] != "cleanshotx" || !valid_id(parts[2]) {
                return Err("Unknown CleanShot action".into());
            }
            let store = v2_sdk::read_store(ctx, SLUG).await?;
            let custom = custom_presets(&store)?;
            let preset = custom
                .iter()
                .chain(defaults().iter())
                .find(|p| p.id == parts[2])
                .cloned()
                .ok_or("Preset no longer exists")?;
            match parts[1] {
                "record" if parts.len() == 4 && state["page"] == "record" => {
                    let index = parts[3]
                        .parse::<usize>()
                        .map_err(|_| "Invalid display index")?;
                    // Refresh physical geometry at click time; never record against a stale/disconnected display.
                    let current = displays()?;
                    let screen = *current
                        .get(index)
                        .ok_or("Recording display is no longer available")?;
                    let (url, exceeds) = record_url(&preset, screen, index)?;
                    #[cfg(target_os = "macos")]
                    mac_installed(ctx).await?;
                    let mut result = open_urls(ctx, &[url]).await?;
                    result["message"] = json!(if exceeds {
                        "Opened CleanShot recording. Preset exceeds screen size; CleanShot will clip to screen bounds"
                    } else {
                        "Opened CleanShot recording at preset"
                    });
                    return Ok(result);
                }
                "edit"
                    if parts.len() == 3
                        && state["page"] == "manage"
                        && !preset.id.starts_with("default-") =>
                {
                    state["page"] = json!("form");
                    state["editing"] = json!(preset.id);
                    return Ok(preset_form(Some(&preset)));
                }
                "delete"
                    if parts.len() == 3
                        && state["page"] == "manage"
                        && !preset.id.starts_with("default-") =>
                {
                    state["page"] = json!("confirm-delete");
                    state["deleting"] = json!(preset.id);
                    let mut delete = action("cleanshotx:confirm-delete", "Delete Preset");
                    delete["style"] = json!("destructive");
                    return Ok(v2_sdk::view(
                        "Delete Preset",
                        json!({"type":"Detail","props":{"markdown":format!("Delete **{}** ({} × {})?",preset.name,preset.width,preset.height)}}),
                        vec![delete, action("cleanshotx:cancel", "Cancel")],
                    ));
                }
                "up" | "down" if parts.len() == 3 && state["page"] == "manage" => {
                    mutate_presets(ctx, parts[1], &preset.id, None).await?;
                }
                _ => return Err("This preset action is not available in the current view".into()),
            }
        }
    }
    preset_view(ctx, state).await
}

fn captures_dir(ctx: &CoreContext) -> PathBuf {
    ctx.paths.data_dir.join("native-v2-cleanshotx/captures")
}

async fn save_capture(ctx: &CoreContext, bytes: Vec<u8>) -> Result<PathBuf, String> {
    let path = captures_dir(ctx).join(format!("Capture-{}.png", uuid::Uuid::new_v4()));
    let output = path.clone();
    tokio::task::spawn_blocking(move || {
        // Validate/normalize actual image bytes. Corrupt/router-error payloads are never saved as screenshots.
        let image =
            image::load_from_memory(&bytes).map_err(|e| format!("Invalid screenshot: {e}"))?;
        std::fs::create_dir_all(output.parent().ok_or("Missing capture directory")?)
            .map_err(|e| e.to_string())?;
        image
            .save_with_format(&output, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(path)
}

async fn fullscreen(ctx: &CoreContext, action_name: Option<&str>) -> CoreResult {
    if action_name.is_some_and(|s| !["copy", "save", "annotate"].contains(&s)) {
        return Err("Windows capture supports copy, save, and annotate; CleanShot Cloud upload and pin overlays are unavailable".into());
    }
    ctx.request("core:platform:visibility", vec![json!(false)])
        .await?;
    let result = ctx
        .request("core:platform:screenshot", vec![json!({"maxWidth":4096})])
        .await?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            result["data"]
                .as_str()
                .ok_or("Native screenshot returned no image")?,
        )
        .map_err(|e| e.to_string())?;
    let path = save_capture(ctx, bytes).await?;
    match action_name {
        Some("copy") => {
            ctx.request("core:platform:clipboard-write", vec![json!({"file":path})])
                .await?;
        }
        Some("annotate") => {
            paint(ctx, &path).await?;
        }
        None => {
            v2_sdk::open(ctx, &path.to_string_lossy()).await?;
        }
        _ => {}
    }
    Ok(v2_sdk::no_view(&format!(
        "Captured display to {}{}",
        path.display(),
        if action_name == Some("copy") {
            " and copied image to clipboard"
        } else {
            ""
        }
    )))
}

async fn paint(ctx: &CoreContext, path: &Path) -> CoreResult {
    ctx.request(
        "core:platform:open-with-app",
        vec![json!({"path":path,"appPath":"mspaint.exe"})],
    )
    .await
}

async fn clipboard_image(ctx: &CoreContext) -> CoreResult {
    let bytes = tokio::task::spawn_blocking(|| {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        let data = clipboard
            .get_image()
            .map_err(|e| format!("Clipboard contains no readable image: {e}"))?;
        let image = image::RgbaImage::from_raw(
            data.width
                .try_into()
                .map_err(|_| "Clipboard image too wide")?,
            data.height
                .try_into()
                .map_err(|_| "Clipboard image too tall")?,
            data.bytes.into_owned(),
        )
        .ok_or("Invalid clipboard image dimensions")?;
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(out.into_inner())
    })
    .await
    .map_err(|e| e.to_string())??;
    let path = save_capture(ctx, bytes).await?;
    paint(ctx, &path).await?;
    Ok(v2_sdk::no_view("Opened clipboard image in Paint"))
}

async fn history_view(ctx: &CoreContext, state: &mut Value) -> CoreResult {
    let folder = captures_dir(ctx);
    let files = tokio::task::spawn_blocking(move || {
        if !folder.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_file()
                && entry.path().extension().is_some_and(|e| e == "png")
            {
                files.push((
                    entry.metadata().and_then(|m| m.modified()).ok(),
                    entry.path(),
                ));
            }
        }
        files.sort_by(|a, b| b.0.cmp(&a.0));
        files.truncate(500);
        Ok::<_, String>(
            files
                .into_iter()
                .map(|(_, p)| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    state["history"] = json!(files);
    let mut rows = Vec::new();
    let mut actions = Vec::new();
    let query = state["query"].as_str().unwrap_or("").to_lowercase();
    for (index, path) in files.iter().enumerate() {
        let title = Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if !title.to_lowercase().contains(&query) {
            continue;
        }
        let item_actions = vec![
            action(
                format!("cleanshotx:history-open:{index}"),
                "Open Screenshot",
            ),
            action(format!("cleanshotx:history-copy:{index}"), "Copy Image"),
            action(
                format!("cleanshotx:history-reveal:{index}"),
                "Show in Folder",
            ),
        ];
        rows.push(row(&index.to_string(), &title, path, &item_actions));
        actions.extend(item_actions);
    }
    if rows.is_empty() {
        rows.push(json!({"type":"List.EmptyView","props":{"title":"No Tezbar screenshots","description":"Fullscreen captures made with this extension appear here. Windows Snipping Tool keeps its own captures."}}));
    }
    Ok(v2_sdk::view(
        "Tezbar Capture History",
        json!({"type":"List","children":rows}),
        actions,
    ))
}

async fn history_action(ctx: &CoreContext, payload: &Value, state: &mut Value) -> CoreResult {
    let id = payload["actionId"]
        .as_str()
        .ok_or("Missing history action")?;
    let parts: Vec<_> = id.split(':').collect();
    if parts.len() != 3 || parts[0] != "cleanshotx" {
        return Err("Invalid history action".into());
    }
    let index: usize = parts[2].parse().map_err(|_| "Invalid history entry")?;
    let path = state["history"]
        .get(index)
        .and_then(Value::as_str)
        .ok_or("History entry no longer exists")?;
    let canonical = tokio::fs::canonicalize(path)
        .await
        .map_err(|e| e.to_string())?;
    let root = tokio::fs::canonicalize(captures_dir(ctx))
        .await
        .map_err(|e| e.to_string())?;
    if canonical.parent() != Some(root.as_path()) {
        return Err("Screenshot is outside Tezbar capture history".into());
    }
    match parts[1] {
        "history-open" => {
            v2_sdk::open(ctx, &canonical.to_string_lossy()).await?;
        }
        "history-copy" => {
            ctx.request(
                "core:platform:clipboard-write",
                vec![json!({"file":canonical})],
            )
            .await?;
        }
        "history-reveal" => {
            ctx.request("core:platform:reveal-file", vec![json!(canonical)])
                .await?;
        }
        _ => return Err("Unknown history action".into()),
    }
    history_view(ctx, state).await
}

#[cfg(target_os = "windows")]
async fn desktop_icons(command: &str) -> CoreResult {
    let command = command.to_string();
    let visible = tokio::task::spawn_blocking(move || unsafe {
        use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, FindWindowExW, FindWindowW, IsWindowVisible, ShowWindow, SW_HIDE, SW_SHOW,
        };
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(Some(0)).collect()
        }
        unsafe extern "system" fn find(parent: HWND, output: LPARAM) -> BOOL {
            let view = FindWindowExW(
                parent,
                std::ptr::null_mut(),
                wide("SHELLDLL_DefView").as_ptr(),
                std::ptr::null(),
            );
            if !view.is_null() {
                *(output as *mut HWND) = view;
                return 0;
            }
            1
        }
        let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        let mut view = FindWindowExW(
            progman,
            std::ptr::null_mut(),
            wide("SHELLDLL_DefView").as_ptr(),
            std::ptr::null(),
        );
        if view.is_null() {
            EnumWindows(Some(find), &mut view as *mut HWND as LPARAM);
        }
        if view.is_null() {
            return Err("Windows Explorer desktop is unavailable".to_string());
        }
        let list = FindWindowExW(
            view,
            std::ptr::null_mut(),
            wide("SysListView32").as_ptr(),
            std::ptr::null(),
        );
        if list.is_null() {
            return Err("Windows desktop icon view is unavailable".to_string());
        }
        let was_visible = IsWindowVisible(list) != 0;
        let requested = match command.as_str() {
            "show-desktop-icons" => true,
            "hide-desktop-icons" => false,
            _ => !was_visible,
        };
        ShowWindow(list, if requested { SW_SHOW } else { SW_HIDE });
        if (IsWindowVisible(list) != 0) != requested {
            return Err("Windows did not change desktop icon visibility".into());
        }
        Ok::<_, String>(requested)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(v2_sdk::no_view(if visible {
        "Desktop icons shown"
    } else {
        "Desktop icons hidden"
    }))
}

fn timer_form() -> Value {
    let mut submit = action("cleanshotx:timer-capture", "Start Capture Timer");
    submit["kind"] = json!("submit-form");
    v2_sdk::view(
        "Timed Fullscreen Capture",
        json!({"type":"Form","children":[{"type":"Form.Description","props":{"text":"Capture the display under the pointer after the delay. Native captures are saved in Tezbar history."}},{"type":"Form.TextField","props":{"id":"seconds","title":"Delay in Seconds","value":"5"}}]}),
        vec![submit],
    )
}

async fn windows_run(
    ctx: &CoreContext,
    command: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    match command {
        "open-history" => history_view(ctx, state).await,
        "capture-fullscreen" => fullscreen(ctx, string_arg(payload, "action")?).await,
        "all-in-one" | "capture-area" | "capture-window" => {
            let capture_action = string_arg(payload, "action")?;
            if capture_action.is_some_and(|s| s != "copy") {
                return Err("Interactive Windows snips support the default clipboard workflow. Use Capture Fullscreen for direct save/annotation; upload and pin are unavailable".into());
            }
            ctx.request("core:platform:visibility", vec![json!(false)])
                .await?;
            v2_sdk::open(ctx, "ms-screenclip:").await?;
            Ok(v2_sdk::no_view(if command == "capture-window" {
                "Opened Windows Snipping Tool. Choose Window mode and select a window to copy it"
            } else {
                "Opened Windows Snipping Tool. Select a region to copy it"
            }))
        }
        "self-timer" => {
            let a = string_arg(payload, "action")?;
            if a.is_some_and(|s| !["copy", "save", "annotate"].contains(&s)) {
                return Err("Windows timed capture supports copy, save, and annotate".into());
            }
            state["captureAction"] = json!(a);
            Ok(timer_form())
        }
        "annotate" => match string_arg(payload, "filepath")? {
            Some(path) => {
                validate_file_path(path)?;
                if !Path::new(path).is_file() {
                    return Err("Image file does not exist".into());
                }
                paint(ctx, Path::new(path)).await?;
                Ok(v2_sdk::no_view("Opened image in Paint"))
            }
            None => {
                let mut submit = action("cleanshotx:annotate-file", "Open in Paint");
                submit["kind"] = json!("submit-form");
                Ok(v2_sdk::view(
                    "Annotate Image",
                    json!({"type":"Form","children":[{"type":"Form.FilePicker","props":{"id":"file","title":"Image","allowMultipleSelection":false}}]}),
                    vec![submit],
                ))
            }
        },
        "open-from-clipboard" => clipboard_image(ctx).await,
        "toggle-desktop-icons" | "hide-desktop-icons" | "show-desktop-icons" => {
            #[cfg(target_os = "windows")]
            {
                desktop_icons(command).await
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err("Windows desktop integration is unavailable on this platform".into())
            }
        }
        "capture-area-raycast-aichat" => {
            Err("Raycast AI Chat integration is unavailable in Tezbar".into())
        }
        "open-settings"
        | "capture-previous-area"
        | "scrolling-capture"
        | "record-screen"
        | "capture-text"
        | "restore-recently-closed"
        | "add-quick-access-overlay"
        | "pin"
        | "record-screen-preset" => Err(format!(
            "{command} requires CleanShot X on macOS; no native Windows replacement is configured"
        )),
        _ => Err(format!("Unknown CleanShot X command: {command}")),
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    match operation {
        "extension:dispose-session" => {
            *state = json!({});
            return Ok(v2_sdk::unchanged());
        }
        "extension:refresh-session" | "extension:load-more" => return Ok(v2_sdk::unchanged()),
        "extension:search-text-changed" => {
            state["query"] = json!(payload["searchText"].as_str().unwrap_or(""));
            if ["record-screen-preset", "manage-recording-presets"].contains(&command)
                && ["record", "manage"].iter().any(|p| state["page"] == *p)
            {
                return preset_view(ctx, state).await;
            }
            if command == "open-history" && cfg!(target_os = "windows") {
                return history_view(ctx, state).await;
            }
            return Ok(v2_sdk::unchanged());
        }
        "extension:invoke-action" => {
            if ["record-screen-preset", "manage-recording-presets"].contains(&command) {
                return preset_action(ctx, payload, state).await;
            }
            if command == "open-history" && cfg!(target_os = "windows") {
                return history_action(ctx, payload, state).await;
            }
            if command == "self-timer"
                && cfg!(target_os = "windows")
                && payload["actionId"] == "cleanshotx:timer-capture"
            {
                let seconds = payload["formValues"]["seconds"]
                    .as_str()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .filter(|s| *s > 0 && *s <= 60)
                    .ok_or("Timer delay must be an integer from 1 to 60 seconds")?;
                let capture_action = state["captureAction"].as_str();
                ctx.request("core:platform:visibility", vec![json!(false)])
                    .await?;
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                return fullscreen(ctx, capture_action).await;
            }
            if command == "annotate"
                && cfg!(target_os = "windows")
                && payload["actionId"] == "cleanshotx:annotate-file"
            {
                let path = payload["formValues"]["file"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .ok_or("Choose an image file")?;
                validate_file_path(path)?;
                if !Path::new(path).is_file() {
                    return Err("Image file does not exist".into());
                }
                paint(ctx, Path::new(path)).await?;
                return Ok(v2_sdk::no_view("Opened image in Paint"));
            }
            return Err("Unknown CleanShot X action".into());
        }
        "extension:run-command" => {}
        _ => return Err(format!("Unsupported CleanShot X operation: {operation}")),
    }
    *state = json!({"entry":command,"query":""});
    if command == "capture-area-raycast-aichat" {
        return Err("This command requires Raycast AI Chat and a Raycast Pro subscription; that integration is unavailable in Tezbar".into());
    }
    if command == "view-uploads" {
        open_urls(ctx, &["https://cleanshot.cloud/".into()]).await?;
        return Ok(v2_sdk::no_view("Opened CleanShot Cloud uploads"));
    }
    if ["record-screen-preset", "manage-recording-presets"].contains(&command) {
        if command == "record-screen-preset" {
            state["displays"] = serde_json::to_value(displays()?).map_err(|e| e.to_string())?;
        }
        state["page"] = json!(if command == "manage-recording-presets" {
            "manage"
        } else {
            "record"
        });
        return preset_view(ctx, state).await;
    }
    #[cfg(target_os = "macos")]
    {
        // Validate arguments before any app lookup or native/UI operation.
        let file = string_arg(payload, "filepath")?;
        let needs_paths = ["annotate", "pin", "add-quick-access-overlay"].contains(&command);
        let explicit_paths = file.map(|s| vec![s.to_string()]).unwrap_or_default();
        // Add-overlay needs Finder selection before its final validation.
        if command != "add-quick-access-overlay" || file.is_some() {
            mac_url(command, payload, &explicit_paths)?;
        }
        mac_installed(ctx).await?;
        let paths = if needs_paths && file.is_none() {
            finder_paths().await?
        } else {
            explicit_paths
        };
        let urls = mac_url(command, payload, &paths)?;
        open_urls(ctx, &urls).await
    }
    #[cfg(target_os = "windows")]
    {
        windows_run(ctx, command, payload, state).await
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(format!("{command} is unavailable on this platform"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<(String, Vec<Value>)>>>;
    fn fixture(fail_open: bool) -> (tempfile::TempDir, CoreContext, Calls) {
        fixture_response(fail_open, json!({"ok":true}))
    }

    fn fixture_response(
        fail_open: bool,
        screenshot: Value,
    ) -> (tempfile::TempDir, CoreContext, Calls) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
            resource_dir: dir.path().join("resources"),
            temp_dir: dir.path().join("tmp"),
            home_dir: dir.path().join("home"),
        });
        let calls: Calls = Arc::new(Mutex::new(Vec::new()));
        let record = calls.clone();
        ctx.set_router(Arc::new(move |channel, args| {
            let record = record.clone();
            let screenshot = screenshot.clone();
            Box::pin(async move {
                record.lock().unwrap().push((channel.clone(), args));
                if fail_open && channel == "shell:open" {
                    return Err("Fixture launch failure".into());
                }
                if channel == "core:platform:screenshot" {
                    return Ok(screenshot);
                }
                Ok(json!({"ok":true}))
            })
        }))
        .unwrap();
        (dir, ctx, calls)
    }

    #[test]
    fn source_url_mapping_and_argument_validation() {
        let source_commands = [
            "open-history",
            "all-in-one",
            "capture-fullscreen",
            "capture-area",
            "capture-previous-area",
            "capture-window",
            "self-timer",
            "scrolling-capture",
            "record-screen",
            "capture-text",
            "open-from-clipboard",
            "toggle-desktop-icons",
            "hide-desktop-icons",
            "show-desktop-icons",
            "restore-recently-closed",
            "pin",
        ];
        for c in source_commands {
            assert_eq!(
                mac_url(c, &json!({}), &[]).unwrap(),
                vec![format!("cleanshot://{c}")]
            );
        }
        assert_eq!(
            mac_url("view-uploads", &json!({}), &[]).unwrap(),
            ["https://cleanshot.cloud/"]
        );
        assert_eq!(
            mac_url("annotate", &json!({}), &[]).unwrap(),
            ["cleanshot://open-annotate"]
        );
        assert!(mac_url("add-quick-access-overlay", &json!({}), &[]).is_err());
        assert_eq!(
            mac_url(
                "open-settings",
                &json!({"argumentValues":{"tab":"quickaccess"}}),
                &[]
            )
            .unwrap(),
            ["cleanshot://open-settings?tab=quickaccess"]
        );
        for c in [
            "capture-fullscreen",
            "capture-area",
            "capture-window",
            "capture-previous-area",
            "self-timer",
        ] {
            for a in ["copy", "save", "annotate", "upload", "pin"] {
                assert_eq!(
                    mac_url(c, &json!({"argumentValues":{"action":a}}), &[]).unwrap(),
                    [format!("cleanshot://{c}?action={a}")]
                );
            }
            assert!(mac_url(
                c,
                &json!({"argumentValues":{"action":"copy&evil=true"}}),
                &[]
            )
            .is_err());
        }
        assert!(mac_url(
            "open-settings",
            &json!({"argumentValues":{"tab":"invalid"}}),
            &[]
        )
        .is_err());
        assert!(mac_url("capture-area-raycast-aichat", &json!({}), &[]).is_err());
        for c in ["annotate", "pin", "add-quick-access-overlay"] {
            let urls = mac_url(
                c,
                &json!({}),
                &["/tmp/a & b.png".into(), "/tmp/你好.png".into()],
            )
            .unwrap();
            assert_eq!(urls.len(), 2);
            assert!(urls[0].ends_with("?filepath=%2Ftmp%2Fa%20%26%20b.png"));
            assert!(urls[1].contains("%E4%BD%A0%E5%A5%BD"));
            assert!(mac_url(c, &json!({}), &["relative.png".into()]).is_err());
        }
    }

    #[test]
    fn recording_uses_real_geometry_relative_sizes_rounding_and_one_based_display() {
        let presets = defaults();
        let screen = Display {
            width: 1513,
            height: 983,
        };
        assert_eq!(
            record_url(&presets[2], screen, 1).unwrap(),
            (
                "cleanshot://record-screen?x=378&y=246&width=757&height=492&display=2".into(),
                false
            )
        );
        assert_eq!(
            record_url(
                &presets[3],
                Display {
                    width: 2560,
                    height: 1440
                },
                0
            )
            .unwrap()
            .0,
            "cleanshot://record-screen?x=320&y=180&width=1920&height=1080&display=1"
        );
        let (url, exceeds) = record_url(&presets[5], screen, 0).unwrap();
        assert!(exceeds);
        assert!(url.contains("x=0&y=0&width=3840&height=2160"));
        assert!(record_url(
            &presets[0],
            Display {
                width: 0,
                height: 0
            },
            0
        )
        .is_err());
    }

    #[tokio::test]
    async fn url_actions_really_dispatch_hide_then_each_open_and_propagate_errors() {
        let (_dir, ctx, calls) = fixture(false);
        let urls = mac_url(
            "pin",
            &json!({}),
            &["/tmp/one.png".into(), "/tmp/two.png".into()],
        )
        .unwrap();
        let result = open_urls(&ctx, &urls).await.unwrap();
        assert_eq!(result["mode"], "no-view");
        let seen = calls.lock().unwrap();
        assert_eq!(
            seen[0],
            ("core:platform:visibility".into(), vec![json!(false)])
        );
        assert_eq!(seen[1], ("shell:open".into(), vec![json!(urls[0])]));
        assert_eq!(seen[2], ("shell:open".into(), vec![json!(urls[1])]));
        drop(seen);
        let record = record_url(
            &defaults()[3],
            Display {
                width: 2560,
                height: 1440,
            },
            1,
        )
        .unwrap()
        .0;
        open_urls(&ctx, &[record.clone()]).await.unwrap();
        assert_eq!(
            calls.lock().unwrap().last().unwrap(),
            &("shell:open".into(), vec![json!(record)])
        );
        let (_dir, ctx, _) = fixture(true);
        assert!(open_urls(&ctx, &urls)
            .await
            .unwrap_err()
            .contains("Fixture launch failure"));
    }

    #[tokio::test]
    async fn native_store_crud_order_confirmation_and_unknown_fields() {
        let (_dir, ctx, calls) = fixture(false);
        v2_sdk::update_store(&ctx, SLUG, |s| {
            s["future"] = json!({"keep":true});
            Ok(())
        })
        .await
        .unwrap();
        let mut state = json!({});
        let result = handle(
            &ctx,
            "manage-recording-presets",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(result["root"]["type"], "List");
        for (name, w, h) in [("One", "1920", "1080"), ("Two", "1280", "720")] {
            preset_action(&ctx, &json!({"actionId":"cleanshotx:new"}), &mut state)
                .await
                .unwrap();
            preset_action(&ctx,&json!({"actionId":"cleanshotx:save","formValues":{"name":name,"width":w,"height":h}}),&mut state).await.unwrap();
        }
        let p = custom_presets(&v2_sdk::read_store(&ctx, SLUG).await.unwrap()).unwrap();
        preset_action(
            &ctx,
            &json!({"actionId":format!("cleanshotx:up:{}",p[1].id)}),
            &mut state,
        )
        .await
        .unwrap();
        let store = v2_sdk::read_store(&ctx, SLUG).await.unwrap();
        assert_eq!(store[CUSTOM][0]["name"], "Two");
        assert_eq!(store["future"]["keep"], true);
        preset_action(
            &ctx,
            &json!({"actionId":format!("cleanshotx:edit:{}",p[0].id)}),
            &mut state,
        )
        .await
        .unwrap();
        assert!(preset_action(&ctx,&json!({"actionId":"cleanshotx:save","formValues":{"name":"Bad","width":"NaN","height":"0"}}),&mut state).await.is_err());
        preset_action(&ctx,&json!({"actionId":"cleanshotx:save","formValues":{"name":"Edited","width":"640","height":"480"}}),&mut state).await.unwrap();
        preset_action(
            &ctx,
            &json!({"actionId":format!("cleanshotx:delete:{}",p[0].id)}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            custom_presets(&v2_sdk::read_store(&ctx, SLUG).await.unwrap())
                .unwrap()
                .len(),
            2
        );
        preset_action(
            &ctx,
            &json!({"actionId":"cleanshotx:confirm-delete"}),
            &mut state,
        )
        .await
        .unwrap();
        let persisted = custom_presets(&v2_sdk::read_store(&ctx, SLUG).await.unwrap()).unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].name, "Two");
        assert!(
            calls.lock().unwrap().is_empty(),
            "Preset management must never capture, record, or open an app"
        );
    }

    #[test]
    fn invalid_presets_and_malformed_store_never_silently_reset() {
        for (w, h) in [
            ("0", "1"),
            ("-1", "1"),
            ("1.5", "2"),
            ("NaN", "2"),
            ("32769", "2"),
        ] {
            assert!(form_preset(&json!({"name":"x","width":w,"height":h}), "test".into()).is_err());
        }
        assert!(custom_presets(&json!({CUSTOM:"broken"})).is_err());
        assert!(custom_presets(
            &json!({CUSTOM:[{"id":"default-1080p","name":"x","width":1,"height":1}]})
        )
        .is_err());
    }

    #[tokio::test]
    async fn unavailable_integrations_and_invalid_windows_arguments_have_no_os_effects() {
        let (_dir, ctx, calls) = fixture(false);
        let mut state = json!({});
        for command in [
            "capture-previous-area",
            "scrolling-capture",
            "record-screen",
            "capture-text",
            "pin",
            "add-quick-access-overlay",
            "restore-recently-closed",
            "open-settings",
            "capture-area-raycast-aichat",
        ] {
            assert!(windows_run(&ctx, command, &json!({}), &mut state)
                .await
                .is_err());
        }
        for command in [
            "capture-area",
            "capture-window",
            "capture-fullscreen",
            "self-timer",
        ] {
            assert!(windows_run(
                &ctx,
                command,
                &json!({"argumentValues":{"action":"upload"}}),
                &mut state
            )
            .await
            .is_err());
        }
        assert!(calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_capture_has_no_fake_history_and_empty_history_uses_temp_fixture() {
        let (_dir, ctx, calls) = fixture(false);
        let mut state = json!({});
        assert!(fullscreen(&ctx, Some("copy"))
            .await
            .unwrap_err()
            .contains("no image"));
        assert!(!captures_dir(&ctx).exists());
        assert!(calls
            .lock()
            .unwrap()
            .iter()
            .all(|(c, _)| c != "core:platform:clipboard-write"));
        let result = history_view(&ctx, &mut state).await.unwrap();
        assert_eq!(result["root"]["children"][0]["type"], "List.EmptyView");
    }

    #[tokio::test]
    async fn fixture_png_capture_is_saved_copied_and_reopenable_without_os_capture() {
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([42, 84, 126, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let response = json!({"data":base64::engine::general_purpose::STANDARD.encode(png.into_inner()),"mimeType":"image/png"});
        let (_dir, ctx, calls) = fixture_response(false, response);
        let result = fullscreen(&ctx, Some("copy")).await.unwrap();
        assert_eq!(result["mode"], "no-view");
        let mut state = json!({});
        let history = history_view(&ctx, &mut state).await.unwrap();
        assert_eq!(history["root"]["children"].as_array().unwrap().len(), 1);
        let path = state["history"][0].as_str().unwrap();
        assert_eq!(image::open(path).unwrap().width(), 2);
        let seen = calls.lock().unwrap();
        assert_eq!(
            seen[0],
            ("core:platform:visibility".into(), vec![json!(false)])
        );
        assert_eq!(seen[1].0, "core:platform:screenshot");
        assert_eq!(
            seen[2],
            (
                "core:platform:clipboard-write".into(),
                vec![json!({"file":path})]
            )
        );
        drop(seen);
        history_action(
            &ctx,
            &json!({"actionId":"cleanshotx:history-open:0"}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(calls.lock().unwrap().last().unwrap().0, "shell:open");
    }

    #[tokio::test]
    async fn malformed_store_cannot_be_overwritten_and_session_operations_have_no_os_effect() {
        let (_dir, ctx, calls) = fixture(false);
        v2_sdk::update_store(&ctx, SLUG, |s| {
            s[CUSTOM] = json!("malformed");
            s["future"] = json!(7);
            Ok(())
        })
        .await
        .unwrap();
        let before = v2_sdk::read_store(&ctx, SLUG).await.unwrap();
        let p = form_preset(
            &json!({"name":"New","width":"1920","height":"1080"}),
            "fixture".into(),
        )
        .unwrap();
        assert!(mutate_presets(&ctx, "create", "fixture", Some(p))
            .await
            .is_err());
        assert_eq!(v2_sdk::read_store(&ctx, SLUG).await.unwrap(), before);
        let mut state = json!({"page":"form"});
        for operation in ["extension:refresh-session", "extension:load-more"] {
            let value = handle(
                &ctx,
                "manage-recording-presets",
                operation,
                &json!({}),
                &mut state,
            )
            .await
            .unwrap();
            assert_eq!(value["mode"], "unchanged");
            assert_eq!(state["page"], "form");
        }
        handle(
            &ctx,
            "manage-recording-presets",
            "extension:dispose-session",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(state, json!({}));
        assert!(calls.lock().unwrap().is_empty());
    }
}
