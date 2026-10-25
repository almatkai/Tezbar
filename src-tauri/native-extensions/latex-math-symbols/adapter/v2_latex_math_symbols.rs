//! Compiled production adapter for beta fd300e2. Vendoring instructions: INTEGRATION.md.
use crate::native_core::common::{CoreContext, CoreResult};
use crate::native_core::extensions::v2_sdk;
use latex_math_symbols_native::{fresh_token, search, symbols, Hit, RECORD_COUNT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub fn commands() -> &'static [&'static str] {
    &["index"]
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    id: String,
    record: usize,
    retry: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Effect {
    record: usize,
    stage: Stage,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Stage {
    Pending,
    Uncertain,
    Acknowledged,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    query: String,
    bindings: Vec<Binding>,
    effect: Option<Effect>,
}

fn load(state: &Value) -> Result<Session, String> {
    let session: Session = serde_json::from_value(state.clone())
        .map_err(|_| "Session expired; reopen Search LateX Symbols".to_string())?;
    if session.bindings.len() > RECORD_COUNT {
        return Err("Session capability limit exceeded; reopen Search LateX Symbols".into());
    }
    Ok(session)
}
fn checkpoint(state: &mut Value, session: &Session) -> Result<(), String> {
    *state =
        serde_json::to_value(session).map_err(|_| "Could not save session state".to_string())?;
    Ok(())
}
fn uncertain(session: &Session, record: usize) -> bool {
    session
        .effect
        .as_ref()
        .is_some_and(|effect| effect.record == record && effect.stage != Stage::Acknowledged)
}
fn render(state: &mut Value, session: &mut Session, message: Option<&str>) -> CoreResult {
    // Compute everything before replacing committed capabilities. No awaits in a render.
    let hits = search(&session.query)?;
    let token = fresh_token()?;
    session.bindings = hits
        .iter()
        .map(|hit| Binding {
            id: format!("latex:{token}:copy:{}", hit.item.id),
            record: hit.item.id,
            retry: uncertain(session, hit.item.id),
        })
        .collect();
    let root = root(session, &hits);
    let actions = session
        .bindings
        .iter()
        .map(|binding| {
            json!({
                "id":binding.id,
                "title":if binding.retry { "Retry Copy to Clipboard" } else { "Copy to Clipboard" },
                "kind":"copy"
            })
        })
        .collect();
    let mut result = v2_sdk::view("Search LateX Symbols", root, actions);
    if let Some(message) = message {
        result["message"] = json!(message);
    }
    checkpoint(state, session)?;
    Ok(result)
}
fn root(session: &Session, hits: &[Hit]) -> Value {
    json!({
        "type":"List",
        "props":{
            "filtering":false,"__hasServerSearch":true,
            "searchBarPlaceholder":"Search LateX Math Symbols",
            "searchText":session.query
        },
        "children":hits.iter().zip(&session.bindings).map(|(hit,binding)| json!({
            "type":"List.Item",
            "props":{
                "id":hit.item.id.to_string(), "title":hit.item.title,
                "subtitle":hit.item.subtitle,
                "accessories":[{"text":hit.item.example}],
                "actionIds":[binding.id]
            }
        })).collect::<Vec<_>>()
    })
}

/// Treat every effect response as untrusted. Never surface transport/private payload errors.
pub fn acknowledged(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("ok") == Some(&Value::Bool(true))
        && object.get("error").is_none_or(Value::is_null)
        && object
            .get("success")
            .is_none_or(|v| v == &Value::Bool(true))
        && object.get("message").is_none_or(Value::is_string)
}
fn validate_payload(payload: &Value) -> Result<(), String> {
    if !payload.is_object() {
        return Err("Expected an operation object".into());
    }
    if payload.get("preferences").is_some_and(|p| !p.is_object()) {
        return Err("Preferences must be an object".into());
    }
    if let Some(args) = payload.get("argumentValues") {
        let object = args.as_object().ok_or("argumentValues must be an object")?;
        if !object.is_empty() {
            return Err("Search LateX Symbols has no arguments".into());
        }
    }
    if payload.get("formValues").is_some_and(|p| !p.is_object()) {
        return Err("formValues must be an object".into());
    }
    Ok(())
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err("Unknown LateX Math Symbols command".into());
    }
    validate_payload(payload)?;
    match operation {
        "extension:run-command" => {
            let mut session = Session {
                query: String::new(),
                bindings: Vec::new(),
                effect: None,
            };
            render(state, &mut session, None)
        }
        "extension:dispose-session" => {
            *state = json!({"disposed":true});
            Ok(v2_sdk::unchanged())
        }
        "extension:refresh-session" | "extension:load-more" => {
            // Refresh must never mint uncommitted capabilities on a cloned state.
            load(state)?;
            Ok(v2_sdk::unchanged())
        }
        "extension:search-text-changed" => {
            let query = payload["searchText"]
                .as_str()
                .ok_or("searchText must be a string")?;
            let mut session = load(state)?;
            session.query = query.to_owned();
            render(state, &mut session, None)
        }
        "extension:invoke-action" => {
            let id = payload["actionId"]
                .as_str()
                .ok_or("actionId must be a string")?;
            let mut session = load(state)?;
            let binding = session
                .bindings
                .iter()
                .find(|b| b.id == id)
                .cloned()
                .ok_or("Action expired; use an action in the current results")?;
            // Output always comes from the bundled canonical record. Event fields are never content.
            let record = symbols()?
                .get(binding.record)
                .ok_or("Invalid symbol binding")?;
            if uncertain(&session, binding.record) && !binding.retry {
                return render(state, &mut session, Some(
                    "Previous copy outcome is uncertain. Choose Retry Copy to Clipboard to repeat deliberately."
                ));
            }
            if binding.retry && !uncertain(&session, binding.record) {
                return Err("Retry action expired; use the current results".into());
            }
            // This checkpoint survives cancellation while awaiting the OS route.
            session.effect = Some(Effect {
                record: binding.record,
                stage: Stage::Pending,
            });
            checkpoint(state, &session)?;
            let response = v2_sdk::copy(ctx, &record.command).await;
            if !response.as_ref().is_ok_and(acknowledged) {
                session.effect = Some(Effect {
                    record: binding.record,
                    stage: Stage::Uncertain,
                });
                checkpoint(state, &session)?;
                return Err("Copy was not acknowledged; clipboard may have changed. Select Copy again to reveal an explicit Retry action.".into());
            }
            // Acknowledgement is durable in session state before any subsequent rendering failure.
            session.effect = Some(Effect {
                record: binding.record,
                stage: Stage::Acknowledged,
            });
            checkpoint(state, &session)?;
            render(state, &mut session, Some("Copied to Clipboard"))
        }
        _ => Err("Unsupported LateX Math Symbols operation".into()),
    }
}
