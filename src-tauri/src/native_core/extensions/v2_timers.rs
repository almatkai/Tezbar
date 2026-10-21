//! Native port of installed `timers` package.json and .sc-build/{manageTimers,
//! manageStopwatches,startCustomTimer,timersMenuBar,stopwatchMenuBar,
//! configureMenubarPresets,previewAlertSounds,stopRunningTimer,dismissTimerAlert}.js.
//! Native V2 owns its own store; never executes the installed JS or reads legacy data.
use super::v2_sdk;
use crate::native_core::common::{now_ms, CoreContext, CoreResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const SLUG: &str = "timers";
const MAX_SECONDS: u64 = 365 * 24 * 3600;
const PRESETS: &[(&str, u64)] = &[
    ("2M", 120),
    ("5M", 300),
    ("10M", 600),
    ("15M", 900),
    ("30M", 1800),
    ("45M", 2700),
    ("60M", 3600),
    ("90M", 5400),
];
const SOUNDS: &[(&str, &str)] = &[
    ("alarmClock.wav", "Alarm Clock"),
    ("dismemberedWoodpecker.wav", "Dismembered Woodpecker"),
    ("fluteRiff.wav", "Flute Riff"),
    ("levelUp.wav", "Level Up"),
    ("pianoChime.wav", "Piano Chime"),
    ("terminator.wav", "Terminator"),
    ("speak_timer_name", "Speak Timer Name"),
];
const ALL: &[&str] = &[
    "start2MinuteTimer",
    "start5MinuteTimer",
    "start10MinuteTimer",
    "start15MinuteTimer",
    "start25MinuteTimer",
    "start30MinuteTimer",
    "start45MinuteTimer",
    "start60MinuteTimer",
    "start90MinuteTimer",
    "startCustomTimer",
    "manageTimers",
    "timersMenuBar",
    "startStopwatch",
    "manageStopwatches",
    "dismissTimerAlert",
    "stopwatchMenuBar",
    "previewAlertSounds",
    "stopRunningTimer",
    "configureMenubarPresets",
];
const SUBMIT: &str = "timers:submit";
const START_SW: &str = "timers:new-stopwatch";
const DISMISS: &str = "timers:dismiss";
const NEW_TIMER: &str = "timers:new-timer";

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

#[derive(Clone, Copy)]
struct Anchor {
    at: Instant,
    elapsed_ms: u64,
}
struct Runtime {
    wake: tokio::sync::watch::Sender<bool>,
    anchors: Mutex<HashMap<String, Anchor>>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Arc<Runtime>>>> = OnceLock::new();
fn runtimes() -> &'static Mutex<HashMap<PathBuf, Arc<Runtime>>> {
    RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn profile(ctx: &CoreContext) -> PathBuf {
    ctx.paths.data_dir.join("native-extensions-v2").join(SLUG)
}
fn runtime(ctx: &CoreContext) -> Option<Arc<Runtime>> {
    runtimes().lock().ok()?.get(&profile(ctx)).cloned()
}

/// Call during native-core startup (before any session opens). One worker per profile,
/// including when every extension view has been disposed. Safe to call again on launch.
pub fn start(ctx: &CoreContext) -> Result<(), String> {
    let key = profile(ctx);
    let mut all = runtimes().lock().map_err(|e| e.to_string())?;
    if all.contains_key(&key) {
        return Ok(());
    }
    let (wake, rx) = tokio::sync::watch::channel(false);
    let rt = Arc::new(Runtime {
        wake,
        anchors: Mutex::new(HashMap::new()),
        worker: Mutex::new(None),
    });
    let worker_ctx = ctx.clone();
    let worker_rt = rt.clone();
    let task = tokio::spawn(async move { worker(worker_ctx, worker_rt, rx).await });
    *rt.worker.lock().map_err(|e| e.to_string())? = Some(task);
    all.insert(key, rt);
    Ok(())
}
/// Call on native-core shutdown. Flushes no extra writes (all mutations already atomic).
pub async fn shutdown(ctx: &CoreContext) {
    let rt = runtimes()
        .lock()
        .ok()
        .and_then(|mut all| all.remove(&profile(ctx)));
    if let Some(rt) = rt {
        let _ = rt.wake.send(true);
        let worker = rt.worker.lock().ok().and_then(|mut worker| worker.take());
        if let Some(worker) = worker {
            let _ = worker.await;
        }
    }
}
fn changed(ctx: &CoreContext) {
    if let Some(rt) = runtime(ctx) {
        let _ = rt.wake.send(false);
    }
}
fn anchor(rt: &Runtime, id: &str, elapsed_ms: u64) {
    if let Ok(mut anchors) = rt.anchors.lock() {
        anchors.insert(
            id.to_owned(),
            Anchor {
                at: Instant::now(),
                elapsed_ms,
            },
        );
    }
}
fn unanchor(rt: &Runtime, id: &str) {
    if let Ok(mut anchors) = rt.anchors.lock() {
        anchors.remove(id);
    }
}
fn elapsed(rt: Option<&Runtime>, id: &str, persisted: u64, since: u64, now: u64) -> u64 {
    if let Some(rt) = rt {
        if let Ok(mut anchors) = rt.anchors.lock() {
            let a = anchors.entry(id.to_owned()).or_insert_with(|| Anchor {
                at: Instant::now(),
                elapsed_ms: persisted.saturating_add(now.saturating_sub(since)),
            });
            return a
                .elapsed_ms
                .saturating_add(a.at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        }
    }
    persisted.saturating_add(now.saturating_sub(since))
}
fn remaining(t: &Value, rt: Option<&Runtime>, now: u64) -> u64 {
    if t["status"] == "ringing" || t["status"] == "finished" {
        return 0;
    }
    let saved = t["elapsedMs"].as_u64().unwrap_or(0);
    let spent = if t["status"] == "paused" {
        saved
    } else {
        elapsed(
            rt,
            t["id"].as_str().unwrap_or(""),
            saved,
            t["resumedAt"].as_u64().unwrap_or(now),
            now,
        )
    };
    t["durationMs"].as_u64().unwrap_or(0).saturating_sub(spent)
}
fn stopwatch_elapsed(sw: &Value, rt: Option<&Runtime>, now: u64) -> u64 {
    let saved = sw["elapsedMs"].as_u64().unwrap_or(0);
    if sw["status"] == "paused" {
        saved
    } else {
        elapsed(
            rt,
            sw["id"].as_str().unwrap_or(""),
            saved,
            sw["resumedAt"].as_u64().unwrap_or(now),
            now,
        )
    }
}
fn checked_duration(hours: &Value, minutes: &Value, seconds: &Value) -> Result<u64, String> {
    fn part(v: &Value) -> Result<u64, String> {
        let input = match v {
            Value::Null => "0".to_owned(),
            Value::String(s) if s.trim().is_empty() => "0".to_owned(),
            Value::String(s) => s.trim().to_owned(),
            Value::Number(n) => n.to_string(),
            _ => return Err("Timer duration must contain whole numbers".into()),
        };
        if !input.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Timer duration must contain nonnegative whole numbers".into());
        }
        input
            .parse::<u64>()
            .map_err(|_| "Timer duration is too large".to_owned())
    }
    let (hours, minutes, seconds) = (part(hours)?, part(minutes)?, part(seconds)?);
    let duration = hours
        .checked_mul(3600)
        .and_then(|v| v.checked_add(minutes.checked_mul(60)?))
        .and_then(|v| v.checked_add(seconds))
        .ok_or("Timer duration is too large")?;
    if duration == 0 || duration > MAX_SECONDS {
        return Err("Timer duration must be between 1 second and 365 days".into());
    }
    Ok(duration)
}
fn title(v: &Value, default: &str) -> Result<String, String> {
    let name = v.as_str().unwrap_or("");
    if name.len() > 200 {
        return Err("Name must be at most 200 characters".into());
    }
    Ok(if name.trim().is_empty() {
        default.to_string()
    } else {
        name.to_owned()
    })
}
fn sound(v: &Value) -> Result<String, String> {
    let name = v.as_str().unwrap_or("default");
    if name == "default" || SOUNDS.iter().any(|(key, _)| key == &name) {
        Ok(name.to_owned())
    } else {
        Err("Unknown timer sound".into())
    }
}
fn selected_sound(preferences: &Value, selection: &str) -> Result<String, String> {
    if selection != "default" {
        return Ok(selection.into());
    }
    // Installed manifest's historic default is "Submarine", absent from its assets.
    let preference = preferences["selectedSound"]
        .as_str()
        .unwrap_or("alarmClock.wav");
    let preference = if preference == "Submarine" {
        "alarmClock.wav"
    } else {
        preference
    };
    sound(&json!(preference)).and_then(|value| {
        if value == "default" {
            Ok("alarmClock.wav".into())
        } else {
            Ok(value)
        }
    })
}
fn validate_volume(preferences: &Value) -> Result<f32, String> {
    let raw = preferences["volumeSetting"]
        .as_str()
        .unwrap_or("1")
        .replace(',', ".");
    let volume = raw
        .parse::<f32>()
        .map_err(|_| "Timer alert volume must be a number".to_owned())?;
    if !volume.is_finite() || !(0.0..=5.0).contains(&volume) {
        return Err("Timer alert volume must be between 0 and 5".into());
    }
    Ok(volume)
}
fn array_mut<'a>(store: &'a mut Value, key: &str) -> Result<&'a mut Vec<Value>, String> {
    let obj = store.as_object_mut().ok_or("Invalid timer storage")?;
    let value = obj.entry(key).or_insert_with(|| json!([]));
    value
        .as_array_mut()
        .ok_or_else(|| format!("Corrupt timers storage: {key}"))
}
fn record<'a>(items: &'a mut [Value], id: &str) -> Result<&'a mut Value, String> {
    items
        .iter_mut()
        .find(|item| item["id"] == id)
        .ok_or_else(|| "Timer or stopwatch no longer exists".into())
}
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn stamp() -> u64 {
    now_ms()
}
fn prefs(payload: &Value) -> &Value {
    &payload["preferences"]
}
fn args(payload: &Value) -> &Value {
    &payload["argumentValues"]
}
fn form(payload: &Value) -> &Value {
    &payload["formValues"]
}
fn input_id(action: &str, prefix: &str) -> Option<String> {
    action
        .strip_prefix(prefix)
        .filter(|id| !id.is_empty() && id.len() <= 100)
        .map(str::to_owned)
}
fn format_time(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    let h = seconds / 3600;
    if h > 0 {
        format!("{h}:{:02}:{:02}", seconds / 60 % 60, seconds % 60)
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
fn item(id: &str, title: &str, subtitle: &str, section: &str, ids: &[String]) -> Value {
    json!({"type":"List.Section","props":{"title":section},"children":[{"type":"List.Item","props":{"id":id,"title":title,"subtitle":subtitle,"actionIds":ids}}]})
}
fn action(id: String, title: &str) -> Value {
    json!({"id":id,"title":title})
}
fn list(title: &str, children: Vec<Value>, actions: Vec<Value>) -> Value {
    v2_sdk::view(title, json!({"type":"List","children":children}), actions)
}
fn fields(names: &[(&str, &str, &str, Value)]) -> Vec<Value> {
    names.iter().map(|(id, label, hint, value)| json!({"type":"Form.TextField","props":{"id":id,"title":label,"placeholder":hint,"value":value}})).collect()
}
fn dropdown() -> Value {
    let mut options =
        vec![json!({"type":"Form.Dropdown.Item","props":{"title":"Default","value":"default"}})];
    for (name, label) in SOUNDS {
        options.push(json!({"type":"Form.Dropdown.Item","props":{"title":label,"value":name}}));
    }
    json!({"type":"Form.Dropdown","props":{"id":"selectedSound","title":"Sound","value":"default"},"children":options})
}
fn custom_form(payload: &Value, state: &Value) -> Value {
    let mut input = fields(&[
        (
            "hours",
            "Hours",
            "0",
            state["hours"]
                .as_str()
                .or_else(|| args(payload)["hours"].as_str())
                .unwrap_or("")
                .into(),
        ),
        (
            "minutes",
            "Minutes",
            "00",
            state["minutes"]
                .as_str()
                .or_else(|| args(payload)["minutes"].as_str())
                .unwrap_or("")
                .into(),
        ),
        (
            "seconds",
            "Seconds",
            "00",
            state["seconds"]
                .as_str()
                .or_else(|| args(payload)["seconds"].as_str())
                .unwrap_or("")
                .into(),
        ),
    ]);
    if prefs(payload)["newTimerInputOrder"] == "ssmmhh" {
        input.reverse();
    }
    input.push(dropdown());
    input.extend(fields(&[("name", "Name", "Pour Tea", json!(""))]));
    input.push(json!({"type":"Form.Checkbox","props":{"id":"willBeSaved","title":"Save as preset","label":"Save as preset","value":false}}));
    v2_sdk::view(
        "Start Custom Timer",
        json!({"type":"Form","children":input}),
        vec![json!({"id":SUBMIT,"title":"Start Custom Timer","kind":"submit-form"})],
    )
}
fn new_timer_form(payload: &Value, state: &Value) -> Value {
    let mut result = custom_form(payload, state);
    if let Some(actions) = result["actions"].as_array_mut() {
        actions.push(action("timers:back".into(), "Back to Timers"));
    }
    result
}

async fn create_timer(
    ctx: &CoreContext,
    duration: u64,
    name: String,
    selection: &str,
    preferences: &Value,
    save: bool,
) -> Result<String, String> {
    let volume = validate_volume(preferences)?;
    let selected = selected_sound(preferences, selection)?;
    if selected != "speak_timer_name" {
        sound_path(ctx, &selected)?;
    }
    #[cfg(target_os = "windows")]
    if volume != 1.0 {
        return Err("Windows sound playback cannot set the requested volume".into());
    }
    let id = new_id();
    let now = stamp();
    let ringing = preferences["ringContinuously"].as_bool().unwrap_or(false);
    let timer = json!({"id":id,"name":name,"durationMs":duration*1000,"elapsedMs":0,"resumedAt":now,"startedAt":now,"status":"running","sound":selected,"volume":volume,"ringContinuously":ringing});
    let preset = json!({"id":new_id(),"name":name,"durationMs":duration*1000,"sound":selection,"visible":true});
    v2_sdk::update_store(ctx, SLUG, move |store| {
        array_mut(store, "timers")?.push(timer);
        if save {
            array_mut(store, "presets")?.push(preset);
        }
        Ok(())
    })
    .await?;
    if let Some(rt) = runtime(ctx) {
        anchor(&rt, &id, 0);
    }
    changed(ctx);
    Ok(format!(
        "Timer \"{name}\" started for {}",
        format_time(duration * 1000)
    ))
}
async fn create_stopwatch(ctx: &CoreContext, name: String) -> Result<String, String> {
    let id = new_id();
    let now = stamp();
    let sw = json!({"id":id,"name":name,"startedAt":now,"resumedAt":now,"elapsedMs":0,"status":"running"});
    v2_sdk::update_store(ctx, SLUG, move |store| {
        array_mut(store, "stopwatches")?.push(sw);
        Ok(())
    })
    .await?;
    if let Some(rt) = runtime(ctx) {
        anchor(&rt, &id, 0);
    }
    Ok(format!("Stopwatch \"{name}\" started"))
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err("Timers command is unavailable on this platform".into());
    }
    start(ctx)?;
    match operation {
        "extension:run-command" => {
            if let Some(minutes) = command
                .strip_prefix("start")
                .and_then(|s| s.strip_suffix("MinuteTimer"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                if ![2, 5, 10, 15, 25, 30, 45, 60, 90].contains(&minutes) {
                    return Err("Unknown timer preset".into());
                }
                return Ok(v2_sdk::no_view(
                    &create_timer(
                        ctx,
                        minutes * 60,
                        "Untitled".into(),
                        "default",
                        prefs(payload),
                        false,
                    )
                    .await?,
                ));
            }
            match command {
                "startStopwatch" => Ok(v2_sdk::no_view(
                    &create_stopwatch(ctx, title(&args(payload)["name"], "Untitled")?).await?,
                )),
                "stopRunningTimer" => {
                    let data = v2_sdk::read_store(ctx, SLUG).await?;
                    let now = stamp();
                    let rt = runtime(ctx);
                    let target = data["timers"]
                        .as_array()
                        .and_then(|arr| arr.iter().min_by_key(|t| remaining(t, rt.as_deref(), now)))
                        .ok_or("No timers found")?;
                    let name = target["name"].as_str().unwrap_or("Untitled").to_string();
                    mutate(
                        ctx,
                        "timers",
                        target["id"].as_str().ok_or("Invalid timer")?,
                        "stop",
                        None,
                    )
                    .await?;
                    Ok(v2_sdk::no_view(&format!("Timer \"{name}\" stopped")))
                }
                "dismissTimerAlert" => {
                    let data = v2_sdk::read_store(ctx, SLUG).await?;
                    let target = data["timers"]
                        .as_array()
                        .and_then(|arr| arr.iter().find(|t| t["status"] == "ringing"))
                        .ok_or("No ringing timer alerts found")?;
                    mutate(
                        ctx,
                        "timers",
                        target["id"].as_str().ok_or("Invalid timer")?,
                        "dismiss",
                        None,
                    )
                    .await?;
                    Ok(v2_sdk::no_view("Ringing timer alert dismissed"))
                }
                "startCustomTimer" => {
                    let has_args = ["hours", "minutes", "seconds"]
                        .iter()
                        .any(|key| args(payload)[key].as_str().is_some_and(|v| !v.is_empty()));
                    if has_args && prefs(payload)["customTimerFormBypass"] == true {
                        let seconds = checked_duration(
                            &args(payload)["hours"],
                            &args(payload)["minutes"],
                            &args(payload)["seconds"],
                        )?;
                        return Ok(v2_sdk::no_view(
                            &create_timer(
                                ctx,
                                seconds,
                                "Untitled".into(),
                                "default",
                                prefs(payload),
                                false,
                            )
                            .await?,
                        ));
                    }
                    Ok(custom_form(payload, state))
                }
                _ => render(ctx, command, payload, state).await,
            }
        }
        "extension:invoke-action" => {
            let action = payload["actionId"].as_str().ok_or("Missing timer action")?;
            if command == "startCustomTimer" && action == SUBMIT {
                let f = form(payload);
                let seconds = checked_duration(&f["hours"], &f["minutes"], &f["seconds"])?;
                let name = title(&f["name"], "Untitled")?;
                let sound = sound(&f["selectedSound"])?;
                let save = f["willBeSaved"].as_bool().unwrap_or(false);
                return Ok(v2_sdk::no_view(
                    &create_timer(ctx, seconds, name, &sound, prefs(payload), save).await?,
                ));
            }
            if !matches!(
                command,
                "manageTimers"
                    | "timersMenuBar"
                    | "manageStopwatches"
                    | "stopwatchMenuBar"
                    | "previewAlertSounds"
                    | "configureMenubarPresets"
            ) {
                return Err("This command has no actions".into());
            }
            if action == "timers:back"
                && matches!(
                    command,
                    "manageTimers" | "timersMenuBar" | "manageStopwatches" | "stopwatchMenuBar"
                )
            {
                *state = json!({});
                return render(ctx, command, payload, state).await;
            }
            if action == NEW_TIMER && matches!(command, "manageTimers" | "timersMenuBar") {
                *state = json!({"screen":"new-timer"});
                return Ok(new_timer_form(payload, state));
            }
            if state["screen"] == "rename" && action == "timers:submit-rename" {
                let collection = state["collection"]
                    .as_str()
                    .ok_or("Missing rename target")?;
                let id = state["target"].as_str().ok_or("Missing rename target")?;
                mutate(ctx, collection, id, "rename", form(payload).get("newName")).await?;
                *state = json!({});
                return render(ctx, command, payload, state).await;
            }
            if state["screen"] == "new-timer" && action == SUBMIT {
                let f = form(payload);
                let secs = checked_duration(&f["hours"], &f["minutes"], &f["seconds"])?;
                create_timer(
                    ctx,
                    secs,
                    title(&f["name"], "Untitled")?,
                    &sound(&f["selectedSound"])?,
                    prefs(payload),
                    f["willBeSaved"] == true,
                )
                .await?;
                *state = json!({});
                return render(ctx, command, payload, state).await;
            }
            if let Some(name) = action.strip_prefix("timers:fixed:") {
                if !matches!(command, "timersMenuBar") {
                    return Err("Action is unavailable in this command".into());
                }
                let seconds = PRESETS
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, seconds)| *seconds)
                    .ok_or("Unknown preset")?;
                create_timer(
                    ctx,
                    seconds,
                    "Untitled".into(),
                    "default",
                    prefs(payload),
                    false,
                )
                .await?;
            } else if action == START_SW
                && matches!(command, "manageStopwatches" | "stopwatchMenuBar")
            {
                create_stopwatch(ctx, "Untitled".into()).await?;
            } else if action == DISMISS && matches!(command, "manageTimers" | "timersMenuBar") {
                let data = v2_sdk::read_store(ctx, SLUG).await?;
                let target = data["timers"]
                    .as_array()
                    .and_then(|arr| arr.iter().find(|t| t["status"] == "ringing"))
                    .ok_or("No ringing timer alerts found")?;
                mutate(
                    ctx,
                    "timers",
                    target["id"].as_str().ok_or("Invalid timer")?,
                    "dismiss",
                    None,
                )
                .await?;
            } else if let Some(id) = input_id(action, "timers:start-preset:") {
                if !matches!(command, "manageTimers" | "timersMenuBar") {
                    return Err("Preset action is unavailable here".into());
                }
                let data = v2_sdk::read_store(ctx, SLUG).await?;
                let preset = data["presets"]
                    .as_array()
                    .and_then(|arr| arr.iter().find(|p| p["id"] == id))
                    .ok_or("Preset no longer exists")?;
                let seconds = preset["durationMs"].as_u64().ok_or("Invalid preset")? / 1000;
                if seconds == 0 || seconds > MAX_SECONDS {
                    return Err("Invalid preset duration".into());
                }
                create_timer(
                    ctx,
                    seconds,
                    title(&preset["name"], "Untitled")?,
                    &sound(&preset["sound"])?,
                    prefs(payload),
                    false,
                )
                .await?;
            } else if let Some(id) = input_id(action, "timers:play:") {
                if command != "previewAlertSounds" {
                    return Err("Sound preview is unavailable here".into());
                }
                let selected = sound(&json!(id))?;
                play_once(
                    ctx,
                    &selected,
                    "Untitled Timer",
                    validate_volume(prefs(payload))?,
                )
                .await?;
            } else if let Some(id) = input_id(action, "timers:toggle-fixed:") {
                if command != "configureMenubarPresets"
                    || !PRESETS.iter().any(|(key, _)| *key == id)
                {
                    return Err("Invalid default preset".into());
                }
                v2_sdk::update_store(ctx, SLUG, move |store| {
                    let o = store.as_object_mut().ok_or("Invalid timer storage")?;
                    let flags = o
                        .entry("presetVisibility")
                        .or_insert_with(|| json!({}))
                        .as_object_mut()
                        .ok_or("Invalid preset visibility")?;
                    let current = flags.get(&id).and_then(Value::as_bool).unwrap_or(true);
                    flags.insert(id, json!(!current));
                    Ok(())
                })
                .await?;
            } else {
                let mut matched = false;
                for (prefix, collection, change) in [
                    ("timers:pause:", "timers", "pause"),
                    ("timers:resume:", "timers", "resume"),
                    ("timers:stop:", "timers", "stop"),
                    ("timers:dismiss:", "timers", "dismiss"),
                    ("timers:save:", "timers", "save"),
                    ("timers:rename:", "timers", "rename"),
                    ("timers:sw-pause:", "stopwatches", "pause"),
                    ("timers:sw-resume:", "stopwatches", "resume"),
                    ("timers:sw-stop:", "stopwatches", "stop"),
                    ("timers:sw-restart:", "stopwatches", "restart"),
                    ("timers:sw-rename:", "stopwatches", "rename"),
                    ("timers:sw-copy:", "stopwatches", "copy"),
                    ("timers:delete-preset:", "presets", "delete"),
                    ("timers:rename-preset:", "presets", "rename"),
                    ("timers:toggle-preset:", "presets", "toggle"),
                ] {
                    if let Some(id) = input_id(action, prefix) {
                        let allowed = match collection {
                            "stopwatches" => {
                                matches!(command, "manageStopwatches" | "stopwatchMenuBar")
                            }
                            "presets" if change == "toggle" => command == "configureMenubarPresets",
                            "presets" => command == "manageTimers",
                            _ => matches!(command, "manageTimers" | "timersMenuBar"),
                        };
                        if !allowed {
                            return Err("Action is unavailable in this command".into());
                        }
                        if change == "rename" {
                            let data = v2_sdk::read_store(ctx, SLUG).await?;
                            if !data[collection]
                                .as_array()
                                .is_some_and(|items| items.iter().any(|v| v["id"] == id))
                            {
                                return Err("Rename target no longer exists".into());
                            }
                            *state = json!({"screen":"rename","collection":collection,"target":id});
                            return render(ctx, command, payload, state).await;
                        }
                        let copied = mutate(ctx, collection, &id, change, None).await?;
                        if let Some(text) = copied {
                            if change == "copy" || prefs(payload)["copyOnSwStop"] == true {
                                v2_sdk::copy(ctx, &text).await?;
                            }
                        }
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return Err("Unknown timer action".into());
                }
            }
            render(ctx, command, payload, state).await
        }
        "extension:refresh-session" | "extension:search-text-changed" | "extension:load-more" => {
            if command == "startCustomTimer" {
                Ok(custom_form(payload, state))
            } else if state["screen"] == "new-timer" {
                Ok(new_timer_form(payload, state))
            } else {
                render(ctx, command, payload, state).await
            }
        }
        "extension:dispose-session" => Ok(v2_sdk::unchanged()),
        _ => Err("Unsupported timers operation".into()),
    }
}

async fn mutate(
    ctx: &CoreContext,
    collection: &str,
    id: &str,
    change: &str,
    new_name: Option<&Value>,
) -> Result<Option<String>, String> {
    let key = collection.to_owned();
    let target = id.to_owned();
    let verb = change.to_owned();
    let requested_name = if verb == "rename" {
        let name = title(new_name.unwrap_or(&Value::Null), "")?;
        if name.trim().is_empty() {
            return Err("Enter a new name".into());
        }
        Some(name)
    } else {
        None
    };
    let now = stamp();
    let rt = runtime(ctx);
    let before = v2_sdk::read_store(ctx, SLUG).await?;
    let original = before[collection]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == target))
        .ok_or("Timer, stopwatch or preset no longer exists")?;
    let spent = if collection == "stopwatches" {
        stopwatch_elapsed(original, rt.as_deref(), now)
    } else {
        original["durationMs"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(remaining(original, rt.as_deref(), now))
    };
    let copied = if collection == "stopwatches" && ["copy", "stop", "restart"].contains(&change) {
        Some(format_time(spent))
    } else {
        None
    };
    if change == "copy" {
        return Ok(copied);
    }
    let old_name = original["name"].as_str().unwrap_or("Untitled").to_string();
    let sound = original["sound"].as_str().unwrap_or("default").to_string();
    let duration = original["durationMs"].as_u64().unwrap_or(0);
    let was_notified = original["notificationSent"] == true;
    if matches!(change, "resume" | "pause")
        && original["status"]
            != (if change == "pause" {
                "running"
            } else {
                "paused"
            })
    {
        return Err("Timer is not in a state that can be changed".into());
    }
    if change == "save" && (collection != "timers" || duration == 0) {
        return Err("Cannot save timer preset".into());
    }
    if change == "dismiss" && original["status"] != "ringing" {
        return Err("Timer is not ringing".into());
    }
    if collection == "presets" && !["delete", "rename", "toggle"].contains(&change) {
        return Err("Unsupported preset action".into());
    }
    let replacement_id = new_id();
    let replacement_id_for_store = replacement_id.clone();
    let preset_id = new_id();
    v2_sdk::update_store(ctx, SLUG, move |store| {
        if verb == "save" {
            array_mut(store, "presets")?.push(json!({"id":preset_id,"name":old_name,"durationMs":duration,"sound":sound,"visible":true}));
            return Ok(());
        }
        let items = array_mut(store, &key)?;
        let position = items.iter().position(|item| item["id"] == target).ok_or("Timer, stopwatch or preset no longer exists")?;
        match verb.as_str() {
            "stop" | "delete" | "dismiss" => { items.remove(position); }
            "rename" => { items[position]["name"] = json!(requested_name); }
            "toggle" => { let visible = items[position]["visible"].as_bool().unwrap_or(true); items[position]["visible"] = json!(!visible); }
            "pause" => { items[position]["elapsedMs"] = json!(spent); items[position]["status"] = json!("paused"); }
            "resume" => { items[position]["resumedAt"] = json!(now); items[position]["status"] = json!("running"); }
            "restart" => {
                let name = items[position]["name"].clone();
                items.remove(position);
                items.push(json!({"id":replacement_id_for_store,"name":name,"startedAt":now,"resumedAt":now,"elapsedMs":0,"status":"running"}));
            }
            _ => return Err("Unsupported timer action".into()),
        }
        Ok(())
    }).await?;
    if let Some(rt) = rt {
        if matches!(change, "stop" | "pause" | "restart" | "dismiss") {
            unanchor(&rt, id);
        }
        if change == "resume" {
            anchor(&rt, id, spent);
        }
        if change == "restart" {
            anchor(&rt, &replacement_id, 0);
        }
    }
    changed(ctx);
    if collection == "timers"
        && was_notified
        && matches!(change, "stop" | "dismiss")
        && ctx.app.is_some()
    {
        cancel_notification(id).await.map_err(|e| {
            format!("Timer stopped but its OS notification could not be removed: {e}")
        })?;
    }
    Ok(copied)
}

async fn render(ctx: &CoreContext, command: &str, payload: &Value, state: &Value) -> CoreResult {
    if state["screen"] == "rename" {
        return Ok(v2_sdk::view(
            "Rename",
            json!({"type":"Form","children":fields(&[("newName","New name","Enter name",json!(""))])}),
            vec![
                json!({"id":"timers:submit-rename","title":"Rename","kind":"submit-form"}),
                action("timers:back".into(), "Back to List"),
            ],
        ));
    }
    let data = v2_sdk::read_store(ctx, SLUG).await?;
    let rt = runtime(ctx);
    let now = stamp();
    let mut children = Vec::new();
    let mut actions = Vec::new();
    match command {
        "manageTimers" | "timersMenuBar" => {
            let mut timers = data["timers"].as_array().cloned().unwrap_or_default();
            timers.sort_by_key(|t| remaining(t, rt.as_deref(), now));
            for timer in &timers {
                let Some(id) = timer["id"].as_str() else {
                    continue;
                };
                let status = timer["status"].as_str().unwrap_or("running");
                let remaining_ms = remaining(timer, rt.as_deref(), now);
                let mut row_actions = Vec::new();
                if status == "running" || status == "paused" {
                    let (verb, label) = if status == "running" {
                        ("pause", "Pause Timer")
                    } else {
                        ("resume", "Resume Timer")
                    };
                    let key = format!("timers:{verb}:{id}");
                    row_actions.push(key.clone());
                    actions.push(action(key, label));
                }
                if status == "ringing" {
                    let key = format!("timers:dismiss:{id}");
                    row_actions.push(key.clone());
                    actions.push(action(key, "Dismiss Alert"));
                }
                let key = format!("timers:stop:{id}");
                row_actions.push(key.clone());
                actions.push(action(key, "Stop Timer"));
                if command == "manageTimers" {
                    let key = format!("timers:save:{id}");
                    row_actions.push(key.clone());
                    actions.push(action(key, "Save as Preset"));
                    let key = format!("timers:rename:{id}");
                    row_actions.push(key.clone());
                    actions.push(action(key, "Rename Timer"));
                }
                let label = match status {
                    "paused" => "Paused",
                    "ringing" => "Ringing",
                    "finished" => "Finished",
                    _ => "Running",
                };
                children.push(item(
                    id,
                    timer["name"].as_str().unwrap_or("Untitled"),
                    &format!("{} left · {label}", format_time(remaining_ms)),
                    "Timers",
                    &row_actions,
                ));
            }
            let mut presets = data["presets"].as_array().cloned().unwrap_or_default();
            presets.sort_by_key(|p| p["durationMs"].as_u64().unwrap_or(0));
            for preset in &presets {
                let Some(id) = preset["id"].as_str() else {
                    continue;
                };
                if command == "timersMenuBar" && preset["visible"] == false {
                    continue;
                }
                let mut ids = vec![format!("timers:start-preset:{id}")];
                actions.push(action(ids[0].clone(), "Start Timer"));
                if command == "manageTimers" {
                    for (verb, label) in [
                        ("rename-preset", "Rename Preset"),
                        ("delete-preset", "Delete Preset"),
                    ] {
                        let key = format!("timers:{verb}:{id}");
                        ids.push(key.clone());
                        actions.push(action(key, label));
                    }
                }
                children.push(item(
                    id,
                    preset["name"].as_str().unwrap_or("Untitled"),
                    &format_time(preset["durationMs"].as_u64().unwrap_or(0)),
                    "Saved Presets",
                    &ids,
                ));
            }
            if command == "timersMenuBar" {
                for (key, seconds) in PRESETS {
                    if data["presetVisibility"][*key].as_bool() == Some(false) {
                        continue;
                    }
                    let action_id = format!("timers:fixed:{key}");
                    actions.push(action(action_id.clone(), "Start Timer"));
                    children.push(item(
                        key,
                        &format!("{} Minute Timer", seconds / 60),
                        &format_time(seconds * 1000),
                        "Default Presets",
                        &[action_id],
                    ));
                }
            }
            actions.push(action(NEW_TIMER.into(), "New Custom Timer"));
            if timers.iter().any(|t| t["status"] == "ringing") {
                actions.push(action(DISMISS.into(), "Dismiss Ringing Timer"));
            }
        }
        "manageStopwatches" | "stopwatchMenuBar" => {
            let mut watches = data["stopwatches"].as_array().cloned().unwrap_or_default();
            watches.sort_by_key(|sw| stopwatch_elapsed(sw, rt.as_deref(), now));
            for sw in &watches {
                let Some(id) = sw["id"].as_str() else {
                    continue;
                };
                let (verb, label) = if sw["status"] == "paused" {
                    ("resume", "Resume Stopwatch")
                } else {
                    ("pause", "Pause Stopwatch")
                };
                let mut ids = Vec::new();
                for (verb, label) in [
                    (verb, label),
                    ("copy", "Copy Current Time"),
                    ("restart", "Restart Stopwatch"),
                    ("stop", "Stop Stopwatch"),
                    ("rename", "Rename Stopwatch"),
                ] {
                    if command == "stopwatchMenuBar" && verb == "rename" {
                        continue;
                    }
                    let key = format!("timers:sw-{verb}:{id}");
                    ids.push(key.clone());
                    actions.push(action(key, label));
                }
                let status = if sw["status"] == "paused" {
                    "Paused"
                } else {
                    "Running"
                };
                children.push(item(
                    id,
                    sw["name"].as_str().unwrap_or("Untitled"),
                    &format!(
                        "{} elapsed · {status}",
                        format_time(stopwatch_elapsed(sw, rt.as_deref(), now))
                    ),
                    "Stopwatches",
                    &ids,
                ));
            }
            actions.push(action(START_SW.into(), "Start Stopwatch"));
        }
        "previewAlertSounds" => {
            for (name, label) in SOUNDS {
                let key = format!("timers:play:{name}");
                children.push(item(
                    name,
                    label,
                    if payload["preferences"]["selectedSound"] == *name {
                        "Selected sound"
                    } else {
                        ""
                    },
                    "Sounds",
                    &[key.clone()],
                ));
                actions.push(action(key, "Play Sound"));
            }
        }
        "configureMenubarPresets" => {
            for (key, seconds) in PRESETS {
                let visible = data["presetVisibility"][*key].as_bool().unwrap_or(true);
                let id = format!("timers:toggle-fixed:{key}");
                actions.push(action(
                    id.clone(),
                    if visible {
                        "Hide in Menu Bar"
                    } else {
                        "Show in Menu Bar"
                    },
                ));
                children.push(item(
                    key,
                    &format!("{} Minute Timer", seconds / 60),
                    if visible { "Visible" } else { "Hidden" },
                    "Default Presets",
                    &[id],
                ));
            }
            for preset in data["presets"].as_array().into_iter().flatten() {
                let Some(id) = preset["id"].as_str() else {
                    continue;
                };
                let visible = preset["visible"].as_bool().unwrap_or(true);
                let key = format!("timers:toggle-preset:{id}");
                actions.push(action(
                    key.clone(),
                    if visible {
                        "Hide in Menu Bar"
                    } else {
                        "Show in Menu Bar"
                    },
                ));
                children.push(item(
                    id,
                    preset["name"].as_str().unwrap_or("Untitled"),
                    if visible { "Visible" } else { "Hidden" },
                    "Custom Presets",
                    &[key],
                ));
            }
        }
        _ => return Err("Unknown timers view".into()),
    }
    let label = match command {
        "manageTimers" => "Manage Timers",
        "timersMenuBar" => "Timers Menu Bar Controls",
        "manageStopwatches" => "Manage Stopwatches",
        "stopwatchMenuBar" => "Stopwatch Menu Bar Controls",
        "previewAlertSounds" => "Preview Alert Sounds",
        _ => "Configure Menu Bar Presets",
    };
    Ok(list(label, children, actions))
}

/// Single app-lifetime scheduler for one Native V2 profile; views never own workers.
async fn worker(ctx: CoreContext, rt: Arc<Runtime>, mut wake: tokio::sync::watch::Receiver<bool>) {
    let mut playing: Option<(String, tokio::process::Child, bool)> = None;
    loop {
        if *wake.borrow() {
            break;
        }
        let next = match tick(&ctx, &rt).await {
            Ok(delay) => delay,
            Err(error) => {
                log::warn!("Native timers scheduler: {error}");
                Duration::from_secs(15)
            }
        };
        // At most one sound process exists per profile, including continuous alerts.
        let data = v2_sdk::read_store(&ctx, SLUG).await.unwrap_or(Value::Null);
        let timers = data["timers"].as_array();
        let ringing = timers.and_then(|items| items.iter().find(|t| t["status"] == "ringing"));
        let pending_single = timers.and_then(|items| {
            items.iter().find(|t| {
                t["status"] == "finished"
                    && t["soundPlayed"] != true
                    && stamp().saturating_sub(t["lastSoundAttempt"].as_u64().unwrap_or(0)) >= 30_000
            })
        });
        let mut outcome = None;
        let mut just_finished: Option<String> = None;
        if let Some((id, child, once)) = playing.as_mut() {
            let exists = timers.is_some_and(|items| {
                items.iter().any(|t| {
                    t["id"] == *id && (t["status"] == "ringing" || t["status"] == "finished")
                })
            });
            if !exists {
                let _ = child.start_kill();
                playing = None;
            } else {
                outcome = match child.try_wait() {
                    Ok(Some(status)) => Some((*once, status.success())),
                    Err(error) => {
                        log::warn!("Timer sound process failed: {error}");
                        Some((*once, false))
                    }
                    Ok(None) => None,
                };
            }
        }
        if let Some((once, successful)) = outcome {
            if let Some((id, _, _)) = playing.take() {
                just_finished = Some(id.clone());
                if once {
                    mark_sound_attempt(&ctx, id, successful).await;
                } else if !successful {
                    mark_sound_attempt(&ctx, id, false).await;
                }
            }
        }
        let ringing_for_audio = ringing.filter(|t| {
            t["soundFailed"] != true
                || stamp().saturating_sub(t["lastSoundAttempt"].as_u64().unwrap_or(0)) >= 30_000
        });
        let pending_single =
            pending_single.filter(|t| just_finished.as_deref() != t["id"].as_str());
        let ringing_for_audio =
            ringing_for_audio.filter(|t| just_finished.as_deref() != t["id"].as_str());
        if playing.is_none() {
            if let Some(timer) = pending_single.or(ringing_for_audio) {
                let sound = timer["sound"].as_str().unwrap_or("alarmClock.wav");
                let name = timer["name"].as_str().unwrap_or("Untitled");
                let timer_id = timer["id"].as_str().unwrap_or("").to_owned();
                let once = timer["status"] == "finished";
                match spawn_sound(&ctx, sound, name, timer_volume(timer)) {
                    Ok(child) => playing = Some((timer_id, child, once)),
                    Err(e) => {
                        log::warn!("Timer alert sound unavailable: {e}");
                        mark_sound_attempt(&ctx, timer_id, false).await;
                    }
                }
            }
        }
        let delay = if playing.is_some() || ringing.is_some() || pending_single.is_some() {
            next.min(Duration::from_secs(2))
        } else {
            next
        };
        tokio::select! {
            _ = tokio::time::sleep(delay) => {},
            changed = wake.changed() => { if changed.is_err() || *wake.borrow() { break; } }
        }
    }
    if let Some((_, mut child, _)) = playing {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

async fn mark_sound_attempt(ctx: &CoreContext, id: String, successful: bool) {
    let attempt = stamp();
    if let Err(error) = v2_sdk::update_store(ctx, SLUG, move |store| {
        if let Some(t) = array_mut(store, "timers")?
            .iter_mut()
            .find(|t| t["id"] == id)
        {
            t["soundFailed"] = json!(!successful);
            t["lastSoundAttempt"] = json!(attempt);
            if successful && t["status"] == "finished" {
                t["soundPlayed"] = json!(true);
            }
        }
        Ok(())
    })
    .await
    {
        log::warn!("Could not record timer alert sound result: {error}");
    }
}

fn expire_items(items: &mut [Value], due: &[String]) {
    for item in items {
        if due.iter().any(|id| item["id"] == *id) && item["status"] == "running" {
            item["status"] = json!(if item["ringContinuously"] == true {
                "ringing"
            } else {
                "finished"
            });
            item["elapsedMs"] = item["durationMs"].clone();
            item["notificationSent"] = json!(false);
            item["lastNotificationAttempt"] = json!(0);
            item["soundPlayed"] = json!(false);
            item["lastSoundAttempt"] = json!(0);
        }
    }
}

fn timer_volume(timer: &Value) -> f32 {
    timer["volume"].as_f64().unwrap_or(1.0) as f32
}

async fn tick(ctx: &CoreContext, rt: &Runtime) -> Result<Duration, String> {
    let notification_ctx = ctx.clone();
    tick_with_notify(ctx, rt, move |id, name| {
        let ctx = notification_ctx.clone();
        async move { notify(&ctx, &id, &name).await }
    })
    .await
}

async fn tick_with_notify<F, Fut>(
    ctx: &CoreContext,
    rt: &Runtime,
    notify_fn: F,
) -> Result<Duration, String>
where
    F: Fn(String, String) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let now = stamp();
    let data = v2_sdk::read_store(ctx, SLUG).await?;
    let mut due = Vec::new();
    let mut wait = Duration::from_secs(60);
    for t in data["timers"].as_array().into_iter().flatten() {
        if t["status"] == "running" {
            let remaining_ms = remaining(t, Some(rt), now);
            if remaining_ms == 0 {
                if let Some(id) = t["id"].as_str() {
                    due.push(id.to_owned());
                }
            } else {
                wait = wait.min(Duration::from_millis(remaining_ms));
            }
        }
    }
    if !due.is_empty() {
        let ids = due.clone();
        v2_sdk::update_store(ctx, SLUG, move |store| {
            expire_items(array_mut(store, "timers")?, &ids);
            Ok(())
        })
        .await?;
        for id in &due {
            unanchor(rt, id);
        }
        ctx.emit(
            "native-timer-changed",
            json!({"extensionId":"timers","completed":due}),
        );
    }
    let data = if due.is_empty() {
        data
    } else {
        v2_sdk::read_store(ctx, SLUG).await?
    };
    for t in data["timers"].as_array().into_iter().flatten() {
        if t["status"] != "ringing" && t["status"] != "finished" {
            continue;
        }
        if t["notificationSent"] == true {
            continue;
        }
        let last = t["lastNotificationAttempt"].as_u64().unwrap_or(0);
        if now.saturating_sub(last) < 30_000 {
            wait = wait.min(Duration::from_millis(30_000 - now.saturating_sub(last)));
            continue;
        }
        let Some(id) = t["id"].as_str() else {
            continue;
        };
        // The platform API is a real OS notification; an error leaves this record
        // pending, not a manufactured success. Retry after 30 seconds.
        let result = notify_fn(
            id.to_owned(),
            t["name"].as_str().unwrap_or("Untitled").to_owned(),
        )
        .await;
        let sent = result.is_ok();
        if let Err(error) = result {
            log::warn!("Timer notification delivery failed: {error}");
            ctx.emit(
                "native-timer-notification-error",
                json!({"id":id,"message":error}),
            );
        }
        let target = id.to_owned();
        let _ = v2_sdk::update_store(ctx, SLUG, move |store| {
            if let Some(item) = array_mut(store, "timers")?
                .iter_mut()
                .find(|item| item["id"] == target)
            {
                item["notificationSent"] = json!(sent);
                item["lastNotificationAttempt"] = json!(now);
            }
            Ok(())
        })
        .await?;
        if !sent {
            wait = wait.min(Duration::from_secs(30));
        }
    }
    Ok(wait.max(Duration::from_millis(100)))
}

/// Native timers ship their own whitelisted sounds, independent of legacy installs.
fn sound_path(ctx: &CoreContext, sound: &str) -> Result<PathBuf, String> {
    if !SOUNDS.iter().any(|(name, _)| *name == sound) || sound == "speak_timer_name" {
        return Err("Unknown sound file".into());
    }
    let path = ctx
        .paths
        .resource_dir
        .join("resources/native-timers")
        .join(sound);
    if !path.is_file() {
        return Err(format!("Bundled timer sound is missing: {sound}"));
    }
    Ok(path)
}
#[cfg(target_os = "macos")]
fn spawn_sound(
    ctx: &CoreContext,
    sound: &str,
    name: &str,
    volume: f32,
) -> Result<tokio::process::Child, String> {
    use std::process::Stdio;
    let mut cmd = if sound == "speak_timer_name" {
        let mut c = tokio::process::Command::new("/usr/bin/say");
        c.arg(name);
        c
    } else {
        let mut c = tokio::process::Command::new("/usr/bin/afplay");
        c.arg(sound_path(ctx, sound)?)
            .arg("--volume")
            .arg(volume.to_string());
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()
        .map_err(|e| format!("Cannot play timer sound: {e}"))
}
#[cfg(target_os = "windows")]
fn spawn_sound(
    ctx: &CoreContext,
    sound: &str,
    name: &str,
    _volume: f32,
) -> Result<tokio::process::Child, String> {
    use std::process::Stdio;
    let script = if sound == "speak_timer_name" {
        format!("Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Speak('{}')", ps_string(name))
    } else {
        let path = sound_path(ctx, sound)?;
        format!(
            "$p=New-Object System.Media.SoundPlayer('{}'); $p.PlaySync()",
            ps_string(&path.to_string_lossy())
        )
    };
    tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot play timer sound: {e}"))
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn spawn_sound(
    _ctx: &CoreContext,
    _sound: &str,
    _name: &str,
    _volume: f32,
) -> Result<tokio::process::Child, String> {
    Err("Timer audio is unsupported on this platform".into())
}
#[cfg(target_os = "windows")]
fn ps_string(s: &str) -> String {
    s.replace('\'', "''")
}

async fn play_once(ctx: &CoreContext, sound: &str, name: &str, volume: f32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if volume != 1.0 {
        return Err("Windows sound playback cannot set the requested volume".into());
    }
    let mut child = spawn_sound(ctx, sound, name, volume)?;
    match tokio::time::timeout(Duration::from_secs(20), child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("Sound player exited with {status}")),
        Ok(Err(e)) => Err(format!("Sound player failed: {e}")),
        Err(_) => {
            let _ = child.start_kill();
            Err("Sound preview timed out".into())
        }
    }
}
#[cfg(target_os = "macos")]
async fn cancel_notification(id: &str) -> Result<(), String> {
    use objc2_foundation::{NSArray, NSString};
    use objc2_user_notifications::UNUserNotificationCenter;
    if !app_bundle() {
        return Err(
            "Bundleless AppleScript notifications cannot be removed from Notification Center"
                .into(),
        );
    }
    let identifier = NSString::from_str(&format!("tezbar-v2-timer-{id}"));
    let identifiers = NSArray::from_slice(&[&*identifier]);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removePendingNotificationRequestsWithIdentifiers(&identifiers);
    center.removeDeliveredNotificationsWithIdentifiers(&identifiers);
    Ok(())
}
#[cfg(target_os = "windows")]
async fn cancel_notification(id: &str) -> Result<(), String> {
    let script = format!("$ErrorActionPreference='Stop'; [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; [Windows.UI.Notifications.ToastNotificationManager]::History.Remove('{}','TezbarNativeTimers','Tezbar')",ps_string(id));
    let output = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Toast removal failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn cancel_notification(_id: &str) -> Result<(), String> {
    Err("Notifications are unsupported on this platform".into())
}

#[cfg(target_os = "macos")]
fn app_bundle() -> bool {
    use objc2_foundation::NSBundle;
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some()
        && bundle
            .bundleURL()
            .path()
            .is_some_and(|path| path.to_string().ends_with(".app"))
}
#[cfg(target_os = "macos")]
async fn notify(_ctx: &CoreContext, id: &str, name: &str) -> Result<(), String> {
    if app_bundle() {
        use block2::RcBlock;
        use objc2_foundation::NSString;
        use objc2_user_notifications::{
            UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
        };
        let rx = {
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str("Ding!"));
            content.setBody(&NSString::from_str(&format!("Timer \"{name}\" complete")));
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&format!("tezbar-v2-timer-{id}")),
                &content,
                None,
            );
            let (tx, rx) = tokio::sync::oneshot::channel();
            let tx = Arc::new(Mutex::new(Some(tx)));
            let callback = RcBlock::new(move |error: *mut objc2_foundation::NSError| {
                let outcome = if let Some(error) = unsafe { error.as_ref() } {
                    Err(error.localizedDescription().to_string())
                } else {
                    Ok(())
                };
                if let Ok(mut sender) = tx.lock() {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(outcome);
                    }
                }
            });
            UNUserNotificationCenter::currentNotificationCenter()
                .addNotificationRequest_withCompletionHandler(&request, Some(&callback));
            rx
        };
        return tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(|_| "Native notification timed out".to_owned())?
            .map_err(|_| "Native notification callback closed".to_owned())?;
    }
    // Development binaries lack an app bundle; AppleScript cannot remove its
    // delivered banner, but OS notification delivery still reports failure.
    let script = r#"on run argv
 display notification ("Timer \"" & item 1 of argv & "\" complete") with title "Ding!"
end run"#;
    let output = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", script, "--", name])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Apple notification failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
#[cfg(target_os = "windows")]
async fn notify(_ctx: &CoreContext, id: &str, name: &str) -> Result<(), String> {
    let body = name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    let xml = format!("<toast><visual><binding template='ToastGeneric'><text>Timer complete</text><text>{body}</text></binding></visual><audio src='ms-winsoundevent:Notification.Default'/></toast>");
    let script = format!("$ErrorActionPreference='Stop'; [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] > $null; $xml=New-Object Windows.Data.Xml.Dom.XmlDocument; $xml.LoadXml('{}'); $toast=[Windows.UI.Notifications.ToastNotification]::new($xml); $toast.Tag='{}'; $toast.Group='TezbarNativeTimers'; [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Tezbar').Show($toast)",ps_string(&xml),ps_string(id));
    let output = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Windows notification failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn notify(_ctx: &CoreContext, _id: &str, _name: &str) -> Result<(), String> {
    Err("Timer notification is unsupported on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;

    struct Fixture {
        ctx: CoreContext,
        dir: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("tezbar-v2-timers-test-{}", new_id()));
            let paths = CorePaths {
                data_dir: dir.join("data"),
                config_dir: dir.join("config"),
                resource_dir: dir.join("resources"),
                temp_dir: dir.join("tmp"),
                home_dir: dir.join("home"),
            };
            let assets = paths.resource_dir.join("resources/native-timers");
            std::fs::create_dir_all(&assets).unwrap();
            for (name, _) in SOUNDS.iter().filter(|(name, _)| name.ends_with(".wav")) {
                std::fs::copy(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../resources/native-timers")
                        .join(name),
                    assets.join(name),
                )
                .unwrap();
            }
            Self {
                ctx: CoreContext::headless(paths),
                dir,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn empty_runtime() -> Runtime {
        let (wake, _rx) = tokio::sync::watch::channel(false);
        Runtime {
            wake,
            anchors: Mutex::new(HashMap::new()),
            worker: Mutex::new(None),
        }
    }

    #[tokio::test]
    async fn clean_native_install_can_start_default_timer_with_bundled_sounds() {
        let f = Fixture::new();
        assert!(!f.ctx.paths.data_dir.join("extensions/timers").exists());
        create_timer(&f.ctx, 5, "Native timer".into(), "default", &json!({}), false)
            .await
            .expect("A native timer must not require a legacy extension installation");
        for (name, _) in SOUNDS.iter().filter(|(name, _)| name.ends_with(".wav")) {
            let path = sound_path(&f.ctx, name).unwrap();
            assert!(path.starts_with(&f.ctx.paths.resource_dir));
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
        }
        assert!(sound_path(&f.ctx, "../alarmClock.wav").is_err());
    }

    #[test]
    fn custom_duration_rejects_negative_fractional_zero_and_overflow() {
        assert_eq!(
            checked_duration(&json!("1"), &json!("30"), &json!("7")).unwrap(),
            5407
        );
        for (h, m, s) in [
            (json!(""), json!(""), json!("")),
            (json!("-1"), json!("0"), json!("0")),
            (json!("0.5"), json!("0"), json!("0")),
            (json!("999999999999999999999999999"), json!("0"), json!("0")),
            (json!("8761"), json!("0"), json!("0")),
        ] {
            assert!(checked_duration(&h, &m, &s).is_err());
        }
        assert!(sound(&json!("../user.wav")).is_err());
        assert!(validate_volume(&json!({"volumeSetting":"6"})).is_err());
    }

    #[test]
    fn active_elapsed_uses_monotonic_clock_paused_and_restarted_uses_wall_time() {
        let rt = empty_runtime();
        rt.anchors.lock().unwrap().insert(
            "one".into(),
            Anchor {
                at: Instant::now() - Duration::from_secs(4),
                elapsed_ms: 3_000,
            },
        );
        let active = elapsed(Some(&rt), "one", 3_000, 9_000, 10_000);
        assert!(active >= 7_000 && active < 8_000);
        assert_eq!(elapsed(None, "one", 3_000, 9_000, 10_000), 4_000);
        assert_eq!(
            remaining(
                &json!({"id":"one","durationMs":10000,"elapsedMs":3200,"status":"paused"}),
                Some(&rt),
                10_000
            ),
            6800
        );
    }

    #[tokio::test]
    async fn records_persist_and_controls_pause_resume_stop_and_presets() {
        let f = Fixture::new();
        create_timer(&f.ctx, 120, " Focus ".into(), "default", &json!({}), true)
            .await
            .unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        let id = store["timers"][0]["id"].as_str().unwrap();
        assert_eq!(store["presets"].as_array().unwrap().len(), 1);
        assert_eq!(store["timers"][0]["name"], " Focus ");
        mutate(&f.ctx, "timers", id, "pause", None).await.unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(store["timers"][0]["status"], "paused");
        mutate(&f.ctx, "timers", id, "resume", None).await.unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(store["timers"][0]["status"], "running");
        mutate(&f.ctx, "timers", id, "stop", None).await.unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert!(store["timers"].as_array().unwrap().is_empty());
        assert_eq!(store["presets"].as_array().unwrap().len(), 1);
        assert!(f
            .ctx
            .paths
            .data_dir
            .join("native-extensions-v2/timers/storage.json")
            .exists());
    }

    #[tokio::test]
    async fn stopwatch_pause_resume_rename_restart_copy_and_delete_persist() {
        let f = Fixture::new();
        create_stopwatch(&f.ctx, "Draft".into()).await.unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        let id = store["stopwatches"][0]["id"].as_str().unwrap().to_owned();
        mutate(&f.ctx, "stopwatches", &id, "pause", None)
            .await
            .unwrap();
        let paused = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(paused["stopwatches"][0]["status"], "paused");
        mutate(&f.ctx, "stopwatches", &id, "resume", None)
            .await
            .unwrap();
        mutate(
            &f.ctx,
            "stopwatches",
            &id,
            "rename",
            Some(&json!("Tea Break")),
        )
        .await
        .unwrap();
        let copied = mutate(&f.ctx, "stopwatches", &id, "copy", None)
            .await
            .unwrap()
            .unwrap();
        assert!(
            copied == "00:00" || copied == "00:01",
            "Expected initial stopwatch copy to be near zero, got {copied}"
        );
        mutate(&f.ctx, "stopwatches", &id, "restart", None)
            .await
            .unwrap();
        let restarted = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_ne!(restarted["stopwatches"][0]["id"], id);
        assert_eq!(restarted["stopwatches"][0]["name"], "Tea Break");
        let new_id = restarted["stopwatches"][0]["id"].as_str().unwrap();
        mutate(&f.ctx, "stopwatches", new_id, "stop", None)
            .await
            .unwrap();
        assert!(
            v2_sdk::read_store(&f.ctx, SLUG).await.unwrap()["stopwatches"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn fake_notification_recorder_delivers_due_timer_once_without_real_os_side_effects() {
        let f = Fixture::new();
        let rt = empty_runtime();
        let past = stamp() - 5_000;
        v2_sdk::update_store(&f.ctx, SLUG, move |store| {
            array_mut(store,"timers")?.push(json!({"id":"due","name":"Tea","durationMs":1000,"elapsedMs":0,"resumedAt":past,"status":"running","ringContinuously":false}));
            Ok(())
        }).await.unwrap();
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        for _ in 0..2 {
            let calls = calls.clone();
            tick_with_notify(&f.ctx, &rt, move |_id, name| {
                let calls = calls.clone();
                async move {
                    calls.lock().unwrap().push(name);
                    Ok(())
                }
            })
            .await
            .unwrap();
        }
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(store["timers"][0]["status"], "finished");
        assert_eq!(store["timers"][0]["notificationSent"], true);
        assert_eq!(*calls.lock().unwrap(), vec!["Tea"]);
    }

    #[tokio::test]
    async fn row_dismiss_targets_only_its_ringing_timer() {
        let f = Fixture::new();
        v2_sdk::update_store(&f.ctx, SLUG, |store| {
            let timers = array_mut(store,"timers")?;
            for id in ["first","second"] {
                timers.push(json!({"id":id,"name":id,"durationMs":1000,"elapsedMs":1000,"status":"ringing"}));
            }
            Ok(())
        }).await.unwrap();
        let view = render(&f.ctx, "manageTimers", &json!({}), &json!({}))
            .await
            .unwrap();
        let id = "timers:dismiss:second";
        assert!(view["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["id"] == id));
        mutate(
            &f.ctx,
            "timers",
            &input_id(id, "timers:dismiss:").unwrap(),
            "dismiss",
            None,
        )
        .await
        .unwrap();
        let stored = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(stored["timers"].as_array().unwrap().len(), 1);
        assert_eq!(stored["timers"][0]["id"], "first");
    }

    #[tokio::test]
    async fn fake_failed_notification_remains_pending_and_ringing_can_be_dismissed() {
        let f = Fixture::new();
        let rt = empty_runtime();
        let past = stamp() - 5_000;
        v2_sdk::update_store(&f.ctx, SLUG, move |store| {
            array_mut(store,"timers")?.push(json!({"id":"ring","name":"Tea","durationMs":1000,"elapsedMs":0,"resumedAt":past,"status":"running","ringContinuously":true}));
            Ok(())
        }).await.unwrap();
        tick_with_notify(&f.ctx, &rt, |_, _| async {
            Err("fake notification denied".into())
        })
        .await
        .unwrap();
        let store = v2_sdk::read_store(&f.ctx, SLUG).await.unwrap();
        assert_eq!(store["timers"][0]["status"], "ringing");
        assert_eq!(store["timers"][0]["notificationSent"], false);
        mutate(&f.ctx, "timers", "ring", "dismiss", None)
            .await
            .unwrap();
        assert!(v2_sdk::read_store(&f.ctx, SLUG).await.unwrap()["timers"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
