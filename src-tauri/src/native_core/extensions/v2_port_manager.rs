//! Native V2 adapter for the installed Raycast `port-manager` extension (MIT).
//! Attribution: installed `.sc-build/{kill-listening-process,named-ports,open-ports,
//! open-ports-menu-bar}.js` and package.json, author lucaschultz and contributors.
//! Uses Tezbar's Rust port enumerator, named-port store, safety confirmation/audit,
//! and the host's process utility; no bundled JavaScript is executed.
use super::v2_sdk as sdk;
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const SLUG: &str = "port-manager";
const REFRESH: &str = "port-manager:refresh";
const ADD: &str = "port-manager:add";
const SAVE: &str = "port-manager:save";
const BACK: &str = "port-manager:back";

pub fn commands() -> &'static [&'static str] {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        &[
            "kill-listening-process",
            "named-ports",
            "open-ports",
            "open-ports-menu-bar",
        ]
    } else {
        &["named-ports"]
    }
}

fn port(value: &Value) -> Result<u16, String> {
    let text = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
        .ok_or("Port must be an integer between 1 and 65535")?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Port must be an integer between 1 and 65535".into());
    }
    text.parse::<u16>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "Port must be an integer between 1 and 65535".into())
}
fn pid(value: &Value) -> Result<u32, String> {
    let text = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))
        .ok_or("Invalid process ID")?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Invalid process ID".into());
    }
    text.parse::<u32>()
        .ok()
        .filter(|n| *n > 4)
        .ok_or_else(|| "Refusing to target a protected process ID".into())
}
fn rows(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "Invalid open-port response".into())
}
fn named_rows(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "Invalid named-port response".into())
}
fn named_by_port(named: &[Value]) -> BTreeMap<u16, String> {
    named
        .iter()
        .filter_map(|v| Some((port(&v["port"]).ok()?, v["name"].as_str()?.to_owned())))
        .collect()
}
fn action(id: String, title: &str, destructive: bool) -> Value {
    json!({"id":id,"title":title,"kind":"action","style":if destructive {"destructive"} else {"default"}})
}
fn listener_id(pid: u32, port: u16) -> String {
    // The id includes a validated PID/port only, never executable user-controlled text.
    format!("port-manager:kill:{pid}:{port}:15")
}
fn list_node(items: Vec<Value>, empty: &str) -> Value {
    json!({"type":"List","props":{"searchBarPlaceholder":"Search ports"},"children":[
        {"type":"List.EmptyView","props":{"title":empty}},
        {"type":"List.Section","props":{"title":"Ports"},"children":items}
    ]})
}
fn matches_query(query: &str, fields: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || fields
            .iter()
            .any(|field| field.to_lowercase().contains(&query))
}
fn decorate(mut response: Value, message: &str) -> Value {
    if !message.is_empty() {
        response["message"] = json!(message);
    }
    response
}
async fn open_ports(ctx: &CoreContext, state: &mut Value, menu: bool, message: &str) -> CoreResult {
    let processes = ctx.request("open-ports:list", vec![]).await?;
    let named = ctx.request("port-manager:named:list", vec![]).await?;
    let labels = named_by_port(named_rows(&named)?);
    let mut items = Vec::new();
    let mut actions = vec![action(REFRESH.into(), "Reload Open Ports", false)];
    let preference = state["primaryPortAction"].as_str().unwrap_or("kill");
    let configured_signal = state["killSignal"].as_str().unwrap_or("ask");
    let query = state["searchText"].as_str().unwrap_or("");
    for process in rows(&processes)? {
        let Ok(process_id) = pid(&process["pid"]) else {
            continue;
        };
        let process_name = process["process"].as_str().unwrap_or("Unknown process");
        let user = process["user"].as_str().unwrap_or("");
        let ports = process["ports"].as_array().ok_or("Invalid process ports")?;
        for raw in ports {
            let Ok(number) = port(raw) else { continue };
            let kill = listener_id(process_id, number);
            let forced = format!("port-manager:kill:{process_id}:{number}:9");
            let copy = format!("port-manager:copy:{process_id}:{number}");
            let copy_command = format!("port-manager:command:{process_id}:{number}");
            let detail = format!("port-manager:detail:{process_id}:{number}");
            actions.push(action(
                kill.clone(),
                &format!("Stop {process_name} on {number} (SIGTERM)"),
                true,
            ));
            #[cfg(target_os = "macos")]
            actions.push(action(
                forced.clone(),
                &format!("Force Stop {process_name} on {number} (SIGKILL)"),
                true,
            ));
            actions.push(action(
                copy.clone(),
                &format!("Copy Details for {process_name}:{number}"),
                false,
            ));
            actions.push(action(
                copy_command.clone(),
                &format!("Copy Kill Command for PID {process_id}"),
                false,
            ));
            actions.push(action(
                detail.clone(),
                &format!("Show Details for {process_name}:{number}"),
                false,
            ));
            // Unsupported preferred actions must NEVER fall back to a destructive Enter.
            let first = match preference {
                "showDetails" | "showInFinder" | "killAll" | "killParent" => &detail,
                "copyInfo" => &copy,
                "copyCommands" => &copy_command,
                "reload" => REFRESH,
                "kill" | "" if cfg!(target_os = "macos") && configured_signal == "9" => &forced,
                "kill" | "" => &kill,
                _ => &detail,
            };
            let name = labels.get(&number).cloned().unwrap_or_default();
            let subtitle = if name.is_empty() {
                format!("TCP {number} · PID {process_id} · {user}")
            } else {
                format!("{name} · TCP {number} · PID {process_id} · {user}")
            };
            if !matches_query(
                query,
                &[
                    process_name,
                    &subtitle,
                    &number.to_string(),
                    &process_id.to_string(),
                ],
            ) {
                continue;
            }
            let mut action_ids = vec![first.to_owned(), detail, copy, copy_command, kill];
            #[cfg(target_os = "macos")]
            action_ids.push(forced);
            action_ids.dedup();
            items.push(
                json!({"type":"List.Item","props":{"id":format!("port:{process_id}:{number}"),
                "title":process_name,"subtitle":subtitle,"actionIds":action_ids,
                "accessories":[{"text":format!(":{number}")}]}}),
            );
        }
    }
    state["portRows"] = processes;
    state["screen"] = json!("list");
    let capability = if menu {
        "Menu-bar extension API is unavailable in Native V2; showing an on-demand port list instead (no background menu or polling)."
    } else {
        ""
    };
    let info = [capability, message]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(decorate(
        sdk::view(
            if menu {
                "Open Ports (menu-bar fallback)"
            } else {
                "Open Ports"
            },
            list_node(items, "No processes listening on TCP ports"),
            actions,
        ),
        &info,
    ))
}
async fn named_ports(ctx: &CoreContext, state: &mut Value, message: &str) -> CoreResult {
    let data = ctx.request("port-manager:named:list", vec![]).await?;
    let mut items = Vec::new();
    let mut actions = vec![action(ADD.into(), "Add Named Port", false)];
    let query = state["searchText"].as_str().unwrap_or("");
    for entry in named_rows(&data)? {
        let Some(id) = entry["id"].as_str() else {
            continue;
        };
        let Ok(number) = port(&entry["port"]) else {
            continue;
        };
        let Some(name) = entry["name"].as_str() else {
            continue;
        };
        let edit = format!("port-manager:edit:{id}");
        let remove = format!("port-manager:remove:{id}");
        if !matches_query(query, &[name, &number.to_string()]) {
            continue;
        }
        actions.push(action(edit.clone(), &format!("Edit {name}"), false));
        actions.push(action(remove.clone(), &format!("Remove {name}"), true));
        items.push(
            json!({"type":"List.Item","props":{"id":format!("named:{id}"),"title":name,
            "subtitle":format!("TCP {number}"),"actionIds":[edit,remove,ADD]}}),
        );
    }
    state["namedRows"] = data;
    state["screen"] = json!("list");
    Ok(decorate(
        sdk::view("Named Ports", list_node(items, "No Named Ports"), actions),
        message,
    ))
}
fn form(state: &mut Value, selected: Option<&Value>) -> Value {
    let name = selected.and_then(|v| v["name"].as_str()).unwrap_or("");
    let number = selected
        .and_then(|v| port(&v["port"]).ok())
        .map(|n| n.to_string())
        .unwrap_or_default();
    state["editingId"] = selected
        .and_then(|v| v["id"].as_str())
        .map_or(Value::Null, |v| json!(v));
    state["screen"] = json!("form");
    let fields = json!([{"type":"Form.TextField","props":{"id":"name","title":"Name","placeholder":"Next.js","value":name}},
        {"type":"Form.TextField","props":{"id":"port","title":"Port","placeholder":"3000","value":number}}]);
    sdk::view(
        if selected.is_some() {
            "Edit Named Port"
        } else {
            "Add Named Port"
        },
        json!({"type":"Form","children":fields}),
        vec![
            json!({"id":SAVE,"title":"Save Named Port","kind":"submit-form"}),
            action(BACK.into(), "Back to Named Ports", false),
        ],
    )
}
fn selected<'a>(state: &'a Value, id: &str, key: &str) -> Result<&'a Value, String> {
    state[key]
        .as_array()
        .and_then(|items| items.iter().find(|v| v["id"] == id))
        .ok_or_else(|| "This entry is no longer available; reload the list".into())
}
fn action_parts(id: &str) -> Result<(&str, u32, u16, Option<&str>), String> {
    let mut pieces = id.split(':');
    if pieces.next() != Some(SLUG) {
        return Err("Unknown Port Manager action".into());
    }
    let kind = pieces.next().ok_or("Unknown Port Manager action")?;
    let process_id = pid(&json!(pieces.next().ok_or("Missing process ID")?))?;
    let number = port(&json!(pieces.next().ok_or("Missing port")?))?;
    let signal = pieces.next();
    if pieces.next().is_some() || !matches!(kind, "kill" | "copy" | "detail" | "command") {
        return Err("Unknown Port Manager action".into());
    }
    Ok((kind, process_id, number, signal))
}
fn target<'a>(state: &'a Value, process_id: u32, number: u16) -> Result<&'a Value, String> {
    rows(&state["portRows"])?
        .iter()
        .find(|row| {
            pid(&row["pid"]).ok() == Some(process_id)
                && row["ports"]
                    .as_array()
                    .is_some_and(|ports| ports.iter().any(|p| port(p).ok() == Some(number)))
        })
        .ok_or_else(|| "Listener is no longer in the displayed list; reload first".into())
}
fn listening<'a>(
    snapshot: &'a Value,
    process_id: u32,
    number: u16,
    name: &str,
) -> Result<&'a str, String> {
    rows(snapshot)?
        .iter()
        .find(|row| {
            pid(&row["pid"]).ok() == Some(process_id)
                && row["process"].as_str() == Some(name)
                && row["ports"]
                    .as_array()
                    .is_some_and(|ports| ports.iter().any(|p| port(p).ok() == Some(number)))
        })
        .and_then(|row| row["identity"].as_str())
        .filter(|identity| !identity.is_empty())
        .ok_or_else(|| {
            "Listener changed or its identity could not be verified; no process was terminated"
                .into()
        })
}
fn process_detail(row: &Value, process_id: u32, number: u16) -> Value {
    let ports = row["ports"].as_array().cloned().unwrap_or_default();
    json!({"type":"Detail","props":{"markdown":format!("# {}\n\nTCP port: **{number}**\n\nPID: **{process_id}**\n\nUser: {}\n\nListening ports: {}",
        row["process"].as_str().unwrap_or("Unknown process"), row["user"].as_str().unwrap_or("Unknown"),
        ports.iter().filter_map(|p|port(p).ok()).map(|p|p.to_string()).collect::<Vec<_>>().join(", "))}})
}
async fn kill(
    ctx: &CoreContext,
    process_id: u32,
    number: u16,
    signal: &str,
    expected_name: &str,
) -> CoreResult {
    if signal != "15" && signal != "9" {
        return Err("Unsupported termination signal".into());
    }
    if cfg!(windows) && signal != "15" {
        return Err("SIGKILL is not available on Windows".into());
    }
    let before = ctx.request("core:ports:list-fresh", vec![]).await?;
    let identity = listening(&before, process_id, number, expected_name)?.to_owned();
    let context = json!({"port":number,"pid":process_id,"process":expected_name,"signal":signal,"identity":identity});
    let approval = ctx
        .request(
            "core:safety:authorize",
            vec![json!("port.kill"), context.clone()],
        )
        .await?;
    if approval["accepted"] != true {
        return Err("Process termination cancelled".into());
    }
    if approval["dryRun"] == true {
        return Ok(sdk::no_view("Dry run: no process was terminated"));
    }
    // Fresh enumeration after approval, then the platform helper verifies birth
    // time once more immediately before signaling (Windows keeps the handle).
    let after = ctx.request("core:ports:list-fresh", vec![]).await?;
    if listening(&after, process_id, number, expected_name)? != identity {
        return Err(
            "Listener changed while awaiting confirmation; no process was terminated".into(),
        );
    }
    let outcome = terminate(ctx, process_id, signal, &identity).await;
    let report = match &outcome {
        Ok(()) => {
            json!({"ok":true,"message":format!("Sent termination to PID {process_id} on port {number}")})
        }
        Err(error) => json!({"ok":false,"message":error}),
    };
    ctx.request(
        "core:safety:record",
        vec![json!("port.kill"), context, report],
    )
    .await?;
    outcome?;
    Ok(sdk::no_view(&format!(
        "Sent termination to PID {process_id} on TCP port {number}"
    )))
}
async fn terminate(
    ctx: &CoreContext,
    process_id: u32,
    signal: &str,
    identity: &str,
) -> Result<(), String> {
    let result = ctx
        .request(
            "core:ports:terminate-verified",
            vec![json!(process_id), json!(signal), json!(identity)],
        )
        .await?;
    if result != true {
        return Err("Process termination was not confirmed".into());
    }
    Ok(())
}
async fn save_named(ctx: &CoreContext, state: &mut Value, values: &Value) -> CoreResult {
    let name = values["name"].as_str().ok_or("Enter a name")?.trim();
    if name.is_empty() || name.len() > 120 {
        return Err("Name must contain 1–120 characters".into());
    }
    let number = port(&values["port"])?;
    let current = ctx.request("port-manager:named:list", vec![]).await?;
    let old = match state["editingId"].as_str() {
        Some(id) => named_rows(&current)?
            .iter()
            .find(|v| v["id"] == id)
            .cloned()
            .ok_or("Named port no longer exists")?,
        None => Value::Null,
    };
    if old["port"].as_u64() == Some(number as u64) && old["name"].as_str() == Some(name) {
        return named_ports(ctx, state, "No changes to save").await;
    }
    if named_rows(&current)?
        .iter()
        .any(|v| port(&v["port"]).ok() == Some(number) && v["id"] != old["id"])
    {
        return Err(format!("TCP port {number} already has a name"));
    }
    // Legacy native store exposes add/remove but no update. Remove then add on same-port edit,
    // compensating by restoring the old record if the add fails. Never report a partial save.
    let old_id = old["id"].as_str().map(str::to_owned);
    let same_port = old_id.is_some() && port(&old["port"]).ok() == Some(number);
    if same_port {
        let removed = ctx
            .request(
                "port-manager:named:remove",
                vec![json!(old_id.as_deref().unwrap())],
            )
            .await?;
        if removed != true {
            return Err("Named port no longer exists".into());
        }
    }
    let result = ctx
        .request(
            "port-manager:named:add",
            vec![json!({"name":name,"port":number})],
        )
        .await;
    match result {
        Ok(entry) if entry.is_object() && entry["id"].is_string() => {
            if let Some(id) = old_id.filter(|_| !same_port) {
                if let Ok(true) = ctx
                    .request("port-manager:named:remove", vec![json!(id)])
                    .await
                    .map(|v| v == true)
                {
                    // Old name removed; the replacement is now the only entry.
                } else {
                    let _ = ctx
                        .request("port-manager:named:remove", vec![entry["id"].clone()])
                        .await;
                    return Err("Could not replace the original named port".into());
                }
            }
            named_ports(ctx, state, &format!("Saved {name} → {number}")).await
        }
        other => {
            if same_port {
                let _ = ctx
                    .request(
                        "port-manager:named:add",
                        vec![json!({"name":old["name"],"port":old["port"]})],
                    )
                    .await;
            }
            Err(match other {
                Ok(_) => "Named port was not saved".into(),
                Err(e) => e,
            })
        }
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err(format!("Unsupported Port Manager command: {command}"));
    }
    match operation {
        "extension:run-command" => match command {
            "kill-listening-process" => {
                let number = port(&payload["argumentValues"]["port"])?;
                let live = ctx.request("core:ports:list-fresh", vec![]).await?;
                let targets = rows(&live)?
                    .iter()
                    .filter_map(|v| {
                        if !v["ports"]
                            .as_array()?
                            .iter()
                            .any(|p| port(p).ok() == Some(number))
                        {
                            return None;
                        }
                        Some((pid(&v["pid"]).ok()?, v["process"].as_str()?.to_owned()))
                    })
                    .collect::<BTreeSet<_>>();
                if targets.is_empty() {
                    return Err(format!("No process is listening on TCP port {number}"));
                }
                let mut killed = Vec::new();
                for (process_id, name) in targets {
                    let result = kill(ctx, process_id, number, "15", &name).await?;
                    if result["message"]
                        .as_str()
                        .is_some_and(|text| text.starts_with("Dry run:"))
                    {
                        return Ok(result);
                    }
                    killed.push(process_id);
                }
                Ok(sdk::no_view(&format!(
                    "Sent termination to PID(s) {} on TCP port {number}",
                    killed
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            }
            "named-ports" => {
                state["searchText"] = json!("");
                named_ports(ctx, state, "").await
            }
            "open-ports" | "open-ports-menu-bar" => {
                state["searchText"] = json!("");
                state["primaryPortAction"] = payload["preferences"]["primaryPortAction"].clone();
                state["killSignal"] = payload["preferences"]["killSignal"].clone();
                open_ports(ctx, state, command == "open-ports-menu-bar", "").await
            }
            _ => unreachable!(),
        },
        "extension:invoke-action" => {
            let id = payload["actionId"].as_str().ok_or("Missing action ID")?;
            if command == "named-ports" {
                match id {
                    ADD => return Ok(form(state, None)),
                    SAVE if state["screen"] == "form" => {
                        return save_named(ctx, state, &payload["formValues"]).await
                    }
                    BACK => return named_ports(ctx, state, "").await,
                    _ => {}
                }
                if let Some(target_id) = id.strip_prefix("port-manager:edit:") {
                    let entry = selected(state, target_id, "namedRows")?.clone();
                    return Ok(form(state, Some(&entry)));
                }
                if let Some(target_id) = id.strip_prefix("port-manager:remove:") {
                    let entry = selected(state, target_id, "namedRows")?;
                    let name = entry["name"].as_str().unwrap_or("named port").to_owned();
                    let approval=ctx.request("core:platform:confirm",vec![json!({"title":"Delete Named Port","message":format!("Delete {name}?"),"buttons":["Cancel","Delete"],"type":"warning"})]).await?;
                    if approval["response"] != 1 {
                        return named_ports(ctx, state, "Deletion cancelled").await;
                    }
                    let removed = ctx
                        .request("port-manager:named:remove", vec![json!(target_id)])
                        .await?;
                    if removed != true {
                        return Err("Named port no longer exists".into());
                    }
                    return named_ports(ctx, state, &format!("Removed {name}")).await;
                }
                return Err("Action is no longer available in Named Ports".into());
            }
            if id == REFRESH || (id == BACK && state["screen"] == "detail") {
                return open_ports(ctx, state, command == "open-ports-menu-bar", "").await;
            }
            let (kind, process_id, number, signal) = action_parts(id)?;
            let row = target(state, process_id, number)?;
            let name = row["process"]
                .as_str()
                .unwrap_or("Unknown process")
                .to_owned();
            match kind {
                "kill" => {
                    let signal = signal.ok_or("Missing termination signal")?;
                    let result = kill(ctx, process_id, number, signal, &name).await?;
                    let message = result["message"].as_str().unwrap_or("Termination sent");
                    open_ports(ctx, state, command == "open-ports-menu-bar", message).await
                }
                "command" if signal.is_none() => {
                    let text = if cfg!(windows) {
                        format!("taskkill.exe /PID {process_id} /F")
                    } else {
                        format!("/bin/kill -15 {process_id}")
                    };
                    sdk::copy(ctx, &text).await?;
                    open_ports(
                        ctx,
                        state,
                        command == "open-ports-menu-bar",
                        "Copied process kill command; not executed",
                    )
                    .await
                }
                "copy" if signal.is_none() => {
                    let text = format!(
                        "{name} · PID {process_id} · TCP {number} · {}",
                        row["user"].as_str().unwrap_or("")
                    );
                    sdk::copy(ctx, &text).await?;
                    open_ports(
                        ctx,
                        state,
                        command == "open-ports-menu-bar",
                        "Copied process details",
                    )
                    .await
                }
                "detail" if signal.is_none() => {
                    let detail = process_detail(row, process_id, number);
                    state["screen"] = json!("detail");
                    Ok(sdk::view(
                        "Port Details",
                        detail,
                        vec![action(BACK.into(), "Back to Open Ports", false)],
                    ))
                }
                _ => Err("Unknown Port Manager action".into()),
            }
        }
        "extension:search-text-changed" => {
            state["searchText"] = json!(payload["searchText"].as_str().unwrap_or(""));
            if command == "named-ports" {
                named_ports(ctx, state, "").await
            } else if command == "kill-listening-process" {
                Ok(sdk::unchanged())
            } else {
                open_ports(ctx, state, command == "open-ports-menu-bar", "").await
            }
        }
        "extension:load-more" => Ok(sdk::unchanged()),
        "extension:refresh-session" => {
            if command == "named-ports" {
                named_ports(ctx, state, "").await
            } else if command == "kill-listening-process" {
                Ok(sdk::unchanged())
            } else {
                open_ports(ctx, state, command == "open-ports-menu-bar", "").await
            }
        }
        "extension:dispose-session" => Ok(sdk::unchanged()),
        _ => Err(format!("Unsupported Port Manager operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::{CorePaths, CoreRouter};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    fn fixture() -> (CoreContext, Arc<Mutex<Vec<String>>>) {
        fixture_with_approval(false)
    }
    fn fixture_with_approval(dry_run: bool) -> (CoreContext, Arc<Mutex<Vec<String>>>) {
        let path =
            std::env::temp_dir().join(format!("tezbar-port-v2-test-{}", uuid::Uuid::new_v4()));
        let ctx = CoreContext::headless(CorePaths {
            data_dir: path.clone(),
            config_dir: path.clone(),
            resource_dir: path.clone(),
            temp_dir: path.clone(),
            home_dir: PathBuf::from("/tmp"),
        });
        let log = Arc::new(Mutex::new(Vec::new()));
        let records = log.clone();
        let router: CoreRouter = Arc::new(move |channel, args| {
            records.lock().unwrap().push(channel.clone());
            Box::pin(async move {
                match channel.as_str() {
                    "open-ports:list" => Ok(
                        json!([{"process":"node","pid":"12345","user":"dev","ports":[3000,8080]}]),
                    ),
                    "core:ports:list-fresh" => Ok(
                        json!([{"process":"node","pid":"12345","user":"dev","ports":[3000,8080],"identity":"birth:123"}]),
                    ),
                    "core:ports:terminate-verified" => {
                        Err("Fixture must not attempt termination".into())
                    }
                    "port-manager:named:list" => {
                        Ok(json!([{"id":"first","name":"API","port":3000}]))
                    }
                    "port-manager:named:add" => {
                        Ok(json!({"id":"second","name":args[0]["name"],"port":args[0]["port"]}))
                    }
                    "port-manager:named:remove" => Ok(json!(true)),
                    "core:platform:confirm" => Ok(json!({"response":0})),
                    "core:safety:authorize" => Ok(json!({"accepted":dry_run,"dryRun":dry_run})),
                    "clipboard:write" => Ok(json!(true)),
                    _ => Err(format!("Unexpected router request: {channel}")),
                }
            })
        });
        ctx.set_router(router).unwrap();
        (ctx, log)
    }
    #[test]
    fn validation_refuses_ambiguous_or_out_of_range_ports_and_pids() {
        for value in [
            json!("0"),
            json!(65536),
            json!("3000x"),
            json!("+3000"),
            json!(" 3000"),
            json!(3.5),
            json!(null),
        ] {
            assert!(port(&value).is_err(), "{value}");
        }
        assert_eq!(port(&json!("65535")), Ok(65535));
        for value in [json!("0"), json!("-100"), json!("123x"), json!(3)] {
            assert!(pid(&value).is_err());
        }
    }
    #[tokio::test]
    async fn list_and_named_form_use_rust_router_without_real_os_kill() {
        let (ctx, log) = fixture();
        let mut state = json!({});
        let view = handle(
            &ctx,
            "open-ports",
            "extension:run-command",
            &json!({"preferences":{"primaryPortAction":"showDetails"}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(view["mode"], "view");
        assert_eq!(
            view["root"]["children"][1]["children"][0]["props"]["subtitle"],
            "API · TCP 3000 · PID 12345 · dev"
        );
        let ids = view["root"]["children"][1]["children"][0]["props"]["actionIds"]
            .as_array()
            .unwrap();
        assert_eq!(ids[0], "port-manager:detail:12345:3000");
        let view = handle(
            &ctx,
            "named-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            view["root"]["children"][1]["children"][0]["props"]["title"],
            "API"
        );
        let form = handle(
            &ctx,
            "named-ports",
            "extension:invoke-action",
            &json!({"actionId":ADD}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(form["root"]["type"], "Form");
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .all(|channel| !channel.contains("safety")));
    }
    #[tokio::test]
    async fn typed_search_returns_full_filtered_views_for_both_lists_and_refresh() {
        let (ctx, _) = fixture();
        let mut state = json!({});
        handle(
            &ctx,
            "open-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let result = handle(
            &ctx,
            "open-ports",
            "extension:search-text-changed",
            &json!({"searchText":"8080"}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(result["mode"], "view");
        assert_eq!(
            result["root"]["children"][1]["children"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            result["root"]["children"][1]["children"][0]["props"]["id"],
            "port:12345:8080"
        );
        let refreshed = handle(
            &ctx,
            "open-ports",
            "extension:refresh-session",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            refreshed["root"]["children"][1]["children"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let cleared = handle(
            &ctx,
            "open-ports",
            "extension:search-text-changed",
            &json!({"searchText":""}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            cleared["root"]["children"][1]["children"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        handle(
            &ctx,
            "named-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let filtered = handle(
            &ctx,
            "named-ports",
            "extension:search-text-changed",
            &json!({"searchText":"not-here"}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(filtered["mode"], "view");
        assert!(filtered["root"]["children"][1]["children"]
            .as_array()
            .unwrap()
            .is_empty());
        let named = handle(
            &ctx,
            "named-ports",
            "extension:search-text-changed",
            &json!({"searchText":"API"}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            named["root"]["children"][1]["children"][0]["props"]["title"],
            "API"
        );
    }
    #[tokio::test]
    async fn named_form_rejects_duplicate_and_invalid_values_without_writes() {
        let (ctx, log) = fixture();
        let mut state = json!({});
        handle(
            &ctx,
            "named-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        handle(
            &ctx,
            "named-ports",
            "extension:invoke-action",
            &json!({"actionId":ADD}),
            &mut state,
        )
        .await
        .unwrap();
        for values in [
            json!({"name":"","port":"8080"}),
            json!({"name":"Web","port":"0"}),
            json!({"name":"API duplicate","port":"3000"}),
        ] {
            assert!(handle(
                &ctx,
                "named-ports",
                "extension:invoke-action",
                &json!({"actionId":SAVE,"formValues":values}),
                &mut state
            )
            .await
            .is_err());
        }
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|channel| channel == "port-manager:named:add"));
    }
    #[tokio::test]
    async fn confirmation_denial_never_terminates_or_audits() {
        let (ctx, log) = fixture();
        let mut state = json!({});
        handle(
            &ctx,
            "open-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let err = handle(
            &ctx,
            "open-ports",
            "extension:invoke-action",
            &json!({"actionId":"port-manager:kill:12345:3000:15"}),
            &mut state,
        )
        .await
        .unwrap_err();
        assert!(err.contains("cancelled"));
        assert_eq!(
            log.lock()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == "core:ports:list-fresh")
                .count(),
            1
        );
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|v| v == "core:safety:record"));
    }
    #[tokio::test]
    async fn reused_pid_after_approval_never_attempts_termination_or_audit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // Dedicated headless fixture: the PID/name/port stay the same, birth changes.
        let path =
            std::env::temp_dir().join(format!("tezbar-port-pid-reuse-{}", uuid::Uuid::new_v4()));
        let ctx = CoreContext::headless(CorePaths {
            data_dir: path.clone(),
            config_dir: path.clone(),
            resource_dir: path.clone(),
            temp_dir: path,
            home_dir: PathBuf::from("/tmp"),
        });
        let fresh_calls = Arc::new(AtomicUsize::new(0));
        let calls = fresh_calls.clone();
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let records = log.clone();
        let router: CoreRouter = Arc::new(move |channel, _| {
            records.lock().unwrap().push(channel.clone());
            let index = if channel == "core:ports:list-fresh" {
                calls.fetch_add(1, Ordering::SeqCst)
            } else {
                0
            };
            Box::pin(async move {
                match channel.as_str() {
                    "open-ports:list" => {
                        Ok(json!([{"process":"node","pid":"12345","ports":[3000]}]))
                    }
                    "port-manager:named:list" => Ok(json!([])),
                    "core:ports:list-fresh" => {
                        Ok(json!([{"process":"node","pid":"12345","ports":[3000],
                        "identity":if index == 0 { "birth:old" } else { "birth:new" }}]))
                    }
                    "core:safety:authorize" => Ok(json!({"accepted":true,"dryRun":false})),
                    "core:ports:terminate-verified" | "core:safety:record" => {
                        Err(format!("Unexpected side effect: {channel}"))
                    }
                    _ => Err(format!("Unexpected request: {channel}")),
                }
            })
        });
        ctx.set_router(router).unwrap();
        let mut state = json!({});
        handle(
            &ctx,
            "open-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let err = handle(
            &ctx,
            "open-ports",
            "extension:invoke-action",
            &json!({"actionId":"port-manager:kill:12345:3000:15"}),
            &mut state,
        )
        .await
        .unwrap_err();
        assert!(err.contains("changed"), "{err}");
        assert_eq!(fresh_calls.load(Ordering::SeqCst), 2);
        assert!(err.contains("awaiting confirmation"));
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|channel| channel == "core:ports:terminate-verified"
                || channel == "core:safety:record"));
    }
    #[tokio::test]
    async fn dry_run_never_claims_termination_or_calls_audit_or_os() {
        let (ctx, log) = fixture_with_approval(true);
        let mut state = json!({});
        let no_view = handle(
            &ctx,
            "kill-listening-process",
            "extension:run-command",
            &json!({"argumentValues":{"port":"3000"}}),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(no_view["mode"], "no-view");
        assert_eq!(no_view["message"], "Dry run: no process was terminated");
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|channel| channel == "core:safety:record"));
        handle(
            &ctx,
            "open-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        let refreshed = handle(
            &ctx,
            "open-ports",
            "extension:invoke-action",
            &json!({"actionId":"port-manager:kill:12345:3000:15"}),
            &mut state,
        )
        .await
        .unwrap();
        assert!(refreshed["message"].as_str().unwrap().contains("Dry run"));
    }
    #[tokio::test]
    async fn invalid_port_prevents_router_requests_and_named_removal_requires_confirmation() {
        let (ctx, log) = fixture();
        let mut state = json!({});
        assert!(handle(
            &ctx,
            "kill-listening-process",
            "extension:run-command",
            &json!({"argumentValues":{"port":"65536"}}),
            &mut state
        )
        .await
        .is_err());
        handle(
            &ctx,
            "named-ports",
            "extension:run-command",
            &json!({}),
            &mut state,
        )
        .await
        .unwrap();
        handle(
            &ctx,
            "named-ports",
            "extension:invoke-action",
            &json!({"actionId":"port-manager:remove:first"}),
            &mut state,
        )
        .await
        .unwrap();
        assert!(!log
            .lock()
            .unwrap()
            .iter()
            .any(|v| v == "port-manager:named:remove"));
    }
}
