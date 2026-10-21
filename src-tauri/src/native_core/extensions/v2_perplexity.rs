//! Native V2 wrapper preserving the original MIT Raycast Perplexity port.
//! Attribution: docs/NATIVE_EXTENSIONS.md and licenses/raycast-extensions-MIT.txt.
use super::{perplexity, v2_sdk};
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::Value;
pub fn commands() -> &'static [&'static str] {
    &[perplexity::COMMAND]
}
pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    _state: &mut Value,
) -> CoreResult {
    if command != perplexity::COMMAND {
        return Err("Unknown Perplexity command".into());
    }
    let session = perplexity::Session {
        use_app: payload["preferences"]["perplexityApp"]
            .as_bool()
            .unwrap_or(false),
    };
    let query = |field: &str, label: &str| -> Result<String, String> {
        match payload.get(field) {
            None => Ok(String::new()),
            Some(values) if values.is_object() => match values.get("query") {
                None => Ok(String::new()),
                Some(Value::String(query)) => Ok(query.clone()),
                _ => Err(format!("query {label} must be text")),
            },
            _ => Err(format!("{field} must be an object")),
        }
    };
    match operation {
        "extension:run-command" => {
            let query = query("argumentValues", "argument")?;
            if !query.is_empty() {
                return session.open(ctx, &query).await;
            }
            let mut view = session.view("");
            let object = view.as_object_mut().unwrap();
            for key in ["sessionId", "extensionId", "commandName"] {
                object.remove(key);
            }
            Ok(view)
        }
        "extension:invoke-action" => {
            if payload["actionId"] != perplexity::SUBMIT {
                return Err("Action is no longer available in this session.".into());
            }
            let query = query("formValues", "form value")?;
            if query.is_empty() {
                return Err("Query cannot be empty".into());
            }
            session.open(ctx, &query).await
        }
        "extension:refresh-session" | "extension:dispose-session" => Ok(v2_sdk::unchanged()),
        _ => Err(format!(
            "Unsupported native extension operation: {operation}"
        )),
    }
}
