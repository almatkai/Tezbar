//! Pure wire adaptation and presentation. Search/ranking and persistent stores remain core-owned.
use serde_json::{Value, json};
use std::collections::HashMap;
use tezbar_ui_model::launcher::{Action, ArgumentField, Arguments, IntegerValidation, Row};
fn string(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}
pub fn terminal_prompt(value: &Value, cwd: Option<&str>) -> String {
    let directory = tezbar_ui_model::terminal::compact_path(
        cwd.unwrap_or_else(|| value["dir"].as_str().unwrap_or("~")),
    );
    format!(
        "{}@{} {directory} %",
        string(value, "user"),
        string(value, "host")
    )
}
pub fn path_section(section: &str) -> Option<&'static str> {
    match section {
        "recommended" => Some("Recommended"),
        "default" => Some("Default"),
        "applications" => Some("All Applications"),
        _ => None,
    }
}
pub fn kind_label(category: &str) -> &str {
    match category {
        "applications" | "application" => "Application",
        "files" | "file" => "File",
        "directory" => "Folder",
        "quick-notes" => "Note",
        "extensions" => "Extension",
        "calculator" => "Calculator",
        "currency" => "Currency",
        "color-converter" => "Color",
        "commands" | "native-command" | "mac-cli" => "Command",
        "knowledge" => "Document",
        "clipboard" => "Clipboard",
        "snippets" => "Snippet",
        "quick-links" => "Quicklink",
        "vault" => "Vault",
        "store" => "Store",
        "chat" => "Chat",
        "terminal" => "Terminal",
        _ => category,
    }
}
pub fn path_rows(value: Value) -> Result<(Vec<Row>, HashMap<String, Value>), String> {
    let values = value.as_array().ok_or("Invalid path completion response")?;
    let mut metadata = HashMap::new();
    let rows = values
        .iter()
        .take(1000)
        .map(|v| {
            let id = string(v, "id");
            metadata.insert(id.clone(), v.clone());
            let action = if v["kind"] == "application" {
                if v["applicationAction"] == "open" {
                    Action::OpenApp {
                        app_name: string(v, "appName"),
                        app_path: v["appPath"].as_str().map(str::to_owned),
                    }
                } else {
                    Action::OpenWithApp {
                        path: string(v, "path"),
                        app_name: v["appName"].as_str().map(str::to_owned),
                        app_path: v["appPath"].as_str().map(str::to_owned),
                    }
                }
            } else {
                Action::OpenFile {
                    path: string(v, "path"),
                }
            };
            Row {
                id,
                title: string(v, "title"),
                subtitle: string(v, "subtitle"),
                category: string(v, "kind"),
                score: v["score"].as_f64().unwrap_or(0.),
                action,
                icon_data_url: None,
                ephemeral: false,
            }
        })
        .collect();
    Ok((rows, metadata))
}
pub fn history_rows(value: Value, terminal: bool) -> Result<Vec<Row>, String> {
    let values = value.as_array().ok_or("Invalid session list")?;
    Ok(values
        .iter()
        .take(100)
        .map(|v| {
            let id = string(v, if terminal { "sessionId" } else { "id" });
            let mut row = Row::command(
                if terminal {
                    "__terminal-session"
                } else {
                    "__chat-session"
                },
                "",
            );
            row.id = format!("{}:{id}", if terminal { "terminal" } else { "chat" });
            row.title = v["title"]
                .as_str()
                .or_else(|| v["label"].as_str())
                .unwrap_or(&id)
                .to_owned();
            row.subtitle = v[if terminal { "cwd" } else { "preview" }]
                .as_str()
                .or_else(|| v["workingDirectory"].as_str())
                .unwrap_or_default()
                .to_owned();
            row.category = if terminal { "terminal" } else { "chat" }.into();
            if let Action::InvokeCommand { payload, .. } = &mut row.action {
                *payload = v.clone();
            }
            row
        })
        .collect())
}
pub fn instant_rows(query: &str) -> Vec<Row> {
    use tezbar_core::instant::{calculator, color, emoji};
    let mut rows = Vec::new();
    let mut copy = |id: String, title: String, subtitle: String, category: &str, text: String| {
        let mut row = Row::command(&id, &title);
        row.id = id;
        row.subtitle = subtitle;
        row.category = category.into();
        row.ephemeral = true;
        row.action = Action::CopyText { text };
        rows.push(row);
    };
    if let Some(c) = calculator::evaluate_expression(query) {
        copy(
            "calculator:result".into(),
            c.formatted,
            c.expression,
            "calculator",
            c.clipboard,
        );
    }
    for c in color::conversion_rows(query) {
        copy(
            format!("color:{}", c.key),
            c.value.clone(),
            c.title.into(),
            "color-converter",
            c.value,
        );
    }
    // Same owned emoji table/fuzzy matcher as the picker. No copied ranking or scanner.
    if let Some(q) = query.strip_prefix(':') {
        for e in emoji::search(q, None, 20) {
            copy(
                format!("emoji:{}", e.char),
                format!("{} {}", e.char, e.name),
                e.category.clone(),
                "emoji",
                e.char.clone(),
            );
        }
    }
    rows
}
#[derive(Clone, Debug)]
pub struct Field {
    pub title: String,
    pub password: bool,
    pub placeholder: String,
    pub dropdown: bool,
}
pub fn arguments(row: &Row) -> Option<(Arguments, Vec<Field>)> {
    arguments_for(row, false)
}
pub fn arguments_for(row: &Row, edit_optional: bool) -> Option<(Arguments, Vec<Field>)> {
    let Action::RunExtensionCommand { extra, .. } = &row.action else {
        return None;
    };
    let wire = json!(extra);
    let inline = &wire["launcher"]["inlineArgument"];
    let defs = wire["commandArgumentDefinitions"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| {
            if inline.is_object() {
                vec![inline.clone()]
            } else if let Some(name) = wire["argumentName"].as_str() {
                vec![json!({"name":"argument","title":name,"required":true})]
            } else {
                vec![]
            }
        });
    if defs.is_empty() {
        return None;
    }
    if !edit_optional
        && !inline.is_object()
        && !defs
            .iter()
            .any(|definition| definition["required"].as_bool().unwrap_or(false))
    {
        return None;
    }
    let mut labels = Vec::new();
    let mut initial = Vec::new();
    let fields = defs
        .iter()
        .map(|d| {
            let name = string(d, "name");
            initial.push(
                wire["argumentValues"][&name]
                    .as_str()
                    .or_else(|| d["default"].as_str())
                    .or_else(|| d["defaultValue"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
            );
            labels.push(Field {
                title: d["title"]
                    .as_str()
                    .or_else(|| d["prompt"].as_str())
                    .unwrap_or(&name)
                    .to_owned(),
                password: d["type"] == "password",
                placeholder: string(d, "placeholder"),
                dropdown: d["type"] == "dropdown",
            });
            let v = &d["validation"];
            ArgumentField {
                name,
                required: d["required"].as_bool().unwrap_or(inline.is_object()),
                validation: (v["kind"] == "integer").then(|| IntegerValidation {
                    min: v["min"].as_u64().unwrap_or(0),
                    max: v["max"].as_u64().unwrap_or(u64::MAX),
                    max_length: v["maxLength"].as_u64().unwrap_or(16) as usize,
                    message: string(v, "message"),
                }),
                choices: d["data"]
                    .as_array()
                    .or_else(|| d["options"].as_array())
                    .into_iter()
                    .flatten()
                    .map(|o| (string(o, "title"), string(o, "value")))
                    .collect(),
            }
        })
        .collect();
    let mut args = Arguments::new(row.id.clone(), fields);
    args.values = initial;
    // Pre-supplied complete argument values skip the form.
    if !edit_optional && args.submit().is_ok() && wire["argumentValues"].is_object() {
        None
    } else {
        Some((args, labels))
    }
}
#[derive(Clone, Debug)]
pub enum RowAction {
    Open,
    Arguments,
    Reveal,
    Copy,
    Paste,
    QuickLook,
    Pin,
}
pub fn menu(row: &Row) -> Vec<(&'static str, RowAction)> {
    let mut a = vec![("Open", RowAction::Open)];
    if arguments_for(row, true).is_some() {
        a.push(("Run with Arguments... (Shift+Enter)", RowAction::Arguments));
    }
    if matches!(
        row.action,
        Action::OpenFile { .. } | Action::OpenWithApp { .. }
    ) {
        a.push(("Reveal in folder", RowAction::Reveal));
        a.push(("Copy path", RowAction::Copy));
    }
    if matches!(
        row.action,
        Action::CopyText { .. } | Action::CopyAndPasteText { .. }
    ) {
        a.push(("Copy", RowAction::Copy));
        a.push(("Copy and paste", RowAction::Paste));
    }
    if tezbar_ui_model::launcher::quick_look_path(row).is_some() {
        a.push(("Quick Look", RowAction::QuickLook));
    }
    a.push(("Pin / Unpin", RowAction::Pin));
    a
}

pub fn update_action(
    status: &tezbar_update::AppUpdateStatus,
) -> Option<super::navigation::UpdateAction> {
    use super::navigation::UpdateAction as A;
    use tezbar_update::AppUpdateStatus as S;
    match status {
        S::Ready { .. } => Some(A::Restart),
        S::Available { .. } => Some(A::Download),
        S::ManualAvailable { installer_url, .. } => Some(A::Installer(installer_url.clone())),
        S::Checking | S::Downloading { .. } => None,
        _ => Some(A::Check),
    }
}
pub fn update_row(
    status: &tezbar_update::AppUpdateStatus,
    checked: bool,
    query: &str,
) -> Option<Row> {
    use tezbar_ui_model::update_policy as p;
    use tezbar_update::AppUpdateStatus as S;
    if !p::matches(query, status, checked) {
        return None;
    }
    let mut row = Row::command("check-for-updates", "Check for updates");
    row.title = match status {
        S::Idle => "Check for updates".into(),
        S::Checking => "Checking for updates…".into(),
        S::Available { version, .. } => format!("Update to Tezbar {version}"),
        S::ManualAvailable { version, .. } => format!("Download Tezbar {version}"),
        S::Downloading { .. } => "Downloading update…".into(),
        S::Ready { .. } => "Restart to update".into(),
        S::UpToDate { .. } => "Tezbar is up to date".into(),
        S::Error { .. } => "Could not check for updates".into(),
    };
    row.subtitle = match status {
        S::Error { message } => message.clone(),
        S::Downloading {
            downloaded,
            total: Some(total),
            ..
        } if *total > 0 => format!("{}%", downloaded.saturating_mul(100) / total),
        _ => String::new(),
    };
    row.score = p::score(status, None, false);
    Some(row)
}

/// Display labels/order are ported from aiProviders.ts. Model defaults/configuration stay M3-owned.
pub fn picker_providers(config: &Value) -> Vec<(String, String)> {
    let mut rows = [
        ("openai", "OpenAI"),
        ("deepseek", "DeepSeek"),
        ("antigravity", "Antigravity"),
        ("openai-compatible", "OpenAI Compatible"),
        ("tokenrouter", "TokenRouter"),
        ("gemini", "Gemini"),
        ("anthropic", "Anthropic"),
        ("ollama", "Ollama"),
        ("copilot", "GitHub Copilot"),
        ("opencode", "OpenCode"),
    ]
    .into_iter()
    .map(|(id, title)| (id.to_owned(), title.to_owned()))
    .collect::<Vec<_>>();
    for p in config["customProviders"].as_array().into_iter().flatten() {
        if let Some(id) = p["id"].as_str() {
            if !rows.iter().any(|(existing, _)| existing == id) {
                rows.push((id.into(), p["title"].as_str().unwrap_or(id).into()));
            }
        }
    }
    rows
}
pub fn picker_models(config: &Value, provider: &str, query: &str) -> (Vec<Value>, usize) {
    use tezbar_ui_model::settings::{provider_catalog, providers::normalize_models};
    let active = config["provider"].as_str().unwrap_or("ollama");
    let selected = config["providerSelectedModels"][active]
        .as_str()
        .or_else(|| config["model"].as_str());
    let defaults = provider_catalog();
    let raw = config["providerModels"][provider]
        .as_array()
        .or_else(|| defaults["DEFAULT_PROVIDER_MODELS"][provider].as_array())
        .cloned()
        .unwrap_or_default();
    let models = normalize_models(&raw);
    let visible = models
        .iter()
        .filter(|m| {
            m["hiddenFromPicker"] != true || (active == provider && m["id"].as_str() == selected)
        })
        .cloned()
        .collect::<Vec<_>>();
    let hidden = models.len() - visible.len();
    let q = query.trim().to_lowercase();
    (
        visible
            .into_iter()
            .filter(|m| {
                format!(
                    "{} {}",
                    m["id"].as_str().unwrap_or_default(),
                    m["capabilities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase()
                .contains(&q)
            })
            .collect(),
        hidden,
    )
}

pub fn pinned_row(pin: &tezbar_ui_model::ui_state::Pin) -> Row {
    Row {
        id: pin.id.clone(),
        title: pin.title.clone(),
        subtitle: pin.subtitle.clone(),
        category: pin.category.clone(),
        score: 0.,
        action: pin.action.clone(),
        icon_data_url: pin.icon_data_url.clone(),
        ephemeral: false,
    }
}

/// Worker effects have already executed. This only classifies view/notice presentation.
pub(super) fn extension_has_view(value: &Value) -> bool {
    value["ok"] != false
        && (value["mode"] == "view"
            || value["root"].is_object()
            || value["sessionId"].is_string()
            || value["preferenceSetup"].is_object())
}
pub(crate) fn extension_notice(value: &Value) -> Option<String> {
    let effects: Vec<tezbar_ui_model::extensions::WireEffect> =
        serde_json::from_value(value["effects"].clone()).unwrap_or_default();
    let mut records = tezbar_ui_model::extensions::WorkerRecords::default();
    records
        .notice(&effects, value["effectSequence"].as_u64())
        .or_else(|| {
            value["message"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        })
}
