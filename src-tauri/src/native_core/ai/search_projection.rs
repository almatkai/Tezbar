use super::*;
pub(super) fn launcher_results(raw: &Value, limit: usize) -> CoreResult {
    let results = raw
        .as_array()
        .ok_or("Invalid launcher search response")?
        .iter()
        .filter(|r| {
            !s(&r["id"]).starts_with("deep-search:") && !s(&r["id"]).starts_with("note-add:")
        })
        .take(limit.clamp(1, 20))
        .map(|r| {
            let mut result = json!({});
            for key in ["id", "title", "subtitle", "category", "score"] {
                if let Some(v) = r.get(key) {
                    result[key] = v.clone();
                }
            }
            let action = &r["action"];
            let target = match s(&action["type"]) {
                "open-file" | "open-with-app" => action["path"].clone(),
                "open-app" => action.get("appPath").unwrap_or(&action["appName"]).clone(),
                "open-url" => action["url"].clone(),
                "run-extension-command" => json!(format!(
                    "{}/{}",
                    s(&action["extensionId"]),
                    s(&action["commandName"])
                )),
                "invoke-command" | "run-native-command" => action["commandId"].clone(),
                "run-shell" => action["command"].clone(),
                _ => Value::Null,
            };
            if !target.is_null() {
                result["target"] = target;
            }
            result
        })
        .collect::<Vec<_>>();

    Ok(json!({"results":results}))
}
