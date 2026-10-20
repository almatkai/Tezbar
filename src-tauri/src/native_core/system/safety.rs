use super::super::common::{read_json, write_json_atomic};
use super::*;
pub(super) fn descriptors() -> Value {
    serde_json::from_str(include_str!("safety.json")).expect("embedded safety catalog")
}
fn descriptor(id: &str) -> CoreResult {
    descriptors()
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .cloned()
        .ok_or_else(|| format!("Unknown safety action: {id}"))
}
pub(super) fn redact(raw: &str) -> String {
    let mut s = raw.to_owned();
    for (pattern, replacement) in [
        (
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
            "[REDACTED_PRIVATE_KEY]",
        ),
        (r"\bsk-[A-Za-z0-9_-]{12,}\b", "[REDACTED_API_KEY]"),
        (r"\bgh[pousr]_[A-Za-z0-9_]{12,}\b", "[REDACTED_TOKEN]"),
        (r"\bAKIA[0-9A-Z]{16}\b", "[REDACTED_AWS_ACCESS_KEY]"),
        (
            r"(?i)\bbearer\s+[A-Za-z0-9._~+/\-]{16,}",
            "Bearer [REDACTED_TOKEN]",
        ),
        (
            r"(?i)\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*[^\s,;]+",
            "$1=[REDACTED]",
        ),
    ] {
        s = regex::Regex::new(pattern)
            .unwrap()
            .replace_all(&s, replacement)
            .into_owned();
    }
    s
}
fn sensitive_key(key: &str) -> bool {
    static KEYS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        regex::Regex::new("(?i)password|passwd|api.?key|secret|token|authorization").unwrap()
    })
    .is_match(key)
}
fn redact_nested(value: &Value, depth: usize) -> Value {
    if depth >= 8 {
        return json!("[OMITTED]");
    }
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .take(40)
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if sensitive_key(key) {
                            json!("[REDACTED]")
                        } else {
                            redact_nested(value, depth + 1)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => json!(values
            .iter()
            .take(40)
            .map(|v| redact_nested(v, depth + 1))
            .collect::<Vec<_>>()),
        Value::String(text) => json!(redact(text)),
        other => other.clone(),
    }
}
fn context_redacted(v: &Value) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(o) = v.as_object() {
        for (k, v) in o.iter().take(40) {
            let sensitive = sensitive_key(k);
            let s = if sensitive {
                "[REDACTED]".to_string()
            } else {
                redact(
                    &v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| redact_nested(v, 0).to_string()),
                )
            };
            let s = if s.chars().count() > 200 {
                format!("{}…", s.chars().take(200).collect::<String>())
            } else {
                s
            };
            out.insert(
                k.chars().take(100).collect(),
                if sensitive || v.is_string() || v.is_array() || v.is_object() {
                    json!(s)
                } else {
                    v.clone()
                },
            );
        }
    }
    Value::Object(out)
}
impl SystemCore {
    pub(super) async fn dry_run(&self) -> Result<bool, String> {
        let ctx = self.ctx.clone();
        Ok(blocking(move || ctx.config.read()).await?["safetyDryRun"] == true)
    }
    pub(super) async fn audit_log(&self, clear: bool) -> CoreResult {
        let _guard = self.state.audit.lock().await;
        let path = self.ctx.paths.data_dir.join("safety/action-log.json");
        blocking(move || {
            if clear {
                write_json_atomic(&path, &json!({"entries":[]}))?;
                return Ok(json!([]));
            }
            if !path.exists() {
                return Ok(json!([]));
            }
            let v = read_json(&path)?;
            Ok(v["entries"]
                .as_array()
                .cloned()
                .map(Value::Array)
                .unwrap_or(json!([])))
        })
        .await
    }
    pub(super) async fn record(
        &self,
        id: &str,
        context: Value,
        result: Value,
    ) -> Result<(), String> {
        let d = descriptor(id)?;
        let id = id.to_string();
        let path = self.ctx.paths.data_dir.join("safety/action-log.json");
        let _guard = self.state.audit.lock().await;
        blocking(move || {
            let mut value = if path.exists() {
                read_json(&path)?
            } else {
                json!({"entries": []})
            };
            if !value.is_object() || !value["entries"].is_array() {
                return Err("Invalid safety log schema".into());
            }
            let mut entries = value["entries"].as_array().cloned().unwrap_or_default();
            // A generated password has no recognizable credential prefix. Never retain it.
            let message = if context["commandId"] == "generate-password" {
                "Password generated and copied".to_owned()
            } else {
                redact(result["message"].as_str().unwrap_or(""))
                    .chars()
                    .take(2000)
                    .collect()
            };
            let entry = json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "at": now_ms(),
                "action": id,
                "title": d["title"],
                "risk": d["risk"],
                "ok": result["ok"] == true,
                "message": message,
                "context": context_redacted(&context)
            });
            entries.insert(0, entry);
            entries.truncate(200);
            value["entries"] = json!(entries);
            write_json_atomic(&path, &value)?;
            Ok(Value::Null)
        })
        .await?;
        Ok(())
    }
    /// Returns {accepted,dryRun}; denial and preview are durably recorded here.
    pub(super) async fn authorize(&self, id: &str, context: Value) -> CoreResult {
        let context = if context.is_object() {
            context
        } else {
            json!({})
        };
        let d = descriptor(id)?;
        let dry = self.dry_run().await?;
        let accepted = if d["requiresConfirmation"] == true || dry {
            let detail = format!(
                "{}{}\nRisk: {}",
                if dry {
                    "Dry-run mode: no changes will be made.\n"
                } else {
                    ""
                },
                context_redacted(&context),
                d["risk"].as_str().unwrap_or("high")
            );
            let label = format!(
                "{}{}",
                if dry { "Preview: " } else { "" },
                d["title"].as_str().unwrap_or(id)
            );
            let r=self.ctx.request("core:platform:confirm",vec![json!({"title":label,"message":d["summary"],"detail":detail,"buttons":["Cancel",label],"type":if d["risk"]=="high"&&!dry{"warning"}else{"question"}})]).await?;
            r["response"] == 1
        } else {
            true
        };
        if !accepted || dry {
            let r = result(
                accepted,
                if !accepted {
                    "Cancelled".into()
                } else {
                    format!(
                        "Dry run: would have {}.",
                        d["title"].as_str().unwrap_or(id).to_lowercase()
                    )
                },
            );
            let mut c = context.clone();
            c["dryRun"] = json!(dry);
            self.record(id, c, r).await?;
        }
        Ok(json!({"accepted":accepted,"dryRun":dry}))
    }
}
