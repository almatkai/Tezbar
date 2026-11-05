use super::*;
impl SystemCore {
    pub(super) async fn background_tasks(&self) -> CoreResult {
        // The knowledge route is expressly cached; never perform corpus/SQL counts here.
        let cached = self
            .ctx
            .request("core:knowledge:background", vec![])
            .await?;
        let root = self.ctx.paths.data_dir.clone();
        blocking(move || {
            let mut tasks = knowledge_tasks(&cached);
            let candidates = [
                "extensions/timers",
                "extensions/raycast.timers",
                "extensions/packages/raycast.timers",
                "extension-registry/packages/raycast.timers",
            ];
            if let Some(package) = candidates
                .iter()
                .map(|p| root.join(p))
                .find(|p| p.join("package.json").exists())
            {
                let mut timers = Vec::new();
                if let Ok(entries) = std::fs::read_dir(package.join(".tezbar-support")) {
                    for e in entries.flatten() {
                        if let (Some(name), Ok(raw)) =
                            (e.file_name().to_str(), std::fs::read_to_string(e.path()))
                        {
                            if let Some(v) = timer(name, &raw, now_ms()) {
                                timers.push(v);
                            }
                        }
                    }
                }
                timers.sort_by_key(|v| std::cmp::Reverse(v["startedAt"].as_i64().unwrap_or(0)));
                tasks.extend(timers);
            }
            Ok(json!(tasks))
        })
        .await
    }
}
fn progress(v: &Value) -> Value {
    v.as_f64()
        .filter(|n| n.is_finite())
        .map(|n| json!(n.clamp(0.0, 1.0)))
        .unwrap_or(Value::Null)
}
fn knowledge_tasks(k: &Value) -> Vec<Value> {
    let mut tasks = Vec::new();
    let s = &k["status"];
    let semantic = &k["semanticSearch"];
    if matches!(s["state"].as_str(), Some("scanning" | "indexing")) {
        tasks.push(json!({"id":format!("indexing:{}",s["jobId"].as_str().unwrap_or("active")),"kind":"indexing","title":"Indexing…","detail":s["detail"].as_str().unwrap_or(if s["state"]=="scanning"{"Scanning folders"}else{"Building local index"}),"progress":progress(&s["progress"])}));
    }
    if semantic["modelState"] == "downloading" {
        let bytes = semantic["downloadBytes"].as_f64().unwrap_or(0.0) / 1e6;
        let total = semantic["downloadTotalBytes"].as_f64().unwrap_or(0.0) / 1e6;
        tasks.push(json!({"id":"semantic-downloading:active","kind":"indexing","title":"Downloading model…","detail":if total>0.0{format!("EmbeddingGemma 2 · {bytes:.1} / {total:.1} MB")}else{format!("EmbeddingGemma 2 · {bytes:.1} MB")},"progress":progress(&semantic["downloadProgress"])}));
    }
    let total = semantic["totalChunks"].as_f64().unwrap_or(0.0);
    let indexed = semantic["indexedChunks"].as_f64().unwrap_or(0.0);
    if k["settings"]["embeddingModel"] == "embeddinggemma-2"
        && semantic["modelState"] == "ready"
        && s["state"] != "paused"
        && semantic["error"].as_str().is_none_or(str::is_empty)
        && (semantic["indexing"] == true || total > indexed)
        && !(total > 0.0 && indexed >= total)
    {
        tasks.push(json!({"id":"semantic-indexing:active","kind":"indexing","title":"Semantic indexing…","detail":if total>0.0{format!("{} / {} chunks",grouped(indexed as u64),grouped(total as u64))}else{"Preparing semantic index…".into()},"progress":if total>0.0{json!((indexed/total).clamp(0.0,1.0))}else{Value::Null}}));
    }
    tasks
}
fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub(super) fn timer(name: &str, raw: &str, now: u64) -> Option<Value> {
    let stem = name.strip_suffix(".timer")?;
    let (date, duration) = stem.rsplit_once("---")?;
    if !regex::Regex::new(r"^\d+(?:\.\d+)?$")
        .unwrap()
        .is_match(duration)
    {
        return None;
    }
    let duration = duration
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)?;
    let date = date.replace("__", ":");
    let started = chrono::DateTime::parse_from_rfc3339(&date)
        .map(|d| d.timestamp_millis())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&date, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|d| d.and_utc().timestamp_millis())
        })
        .ok()?;
    let v: Value = serde_json::from_str(raw).ok()?;
    v["pid"].as_f64().filter(|n| n.is_finite())?;
    let pause = v["pauseElapsed"]
        .as_f64()
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let remaining = ((started as f64 + (duration + pause) * 1000.0 - now as f64) / 1000.0)
        .ceil()
        .max(0.0);
    Some(
        json!({"id":format!("timer:{name}"),"kind":"timer","title":v["name"].as_str().map(str::trim).filter(|s|!s.is_empty()).unwrap_or("Timer"),"detail":if remaining>0.0{"Timer running"}else{"Timer complete"},"startedAt":started,"remainingSeconds":remaining as u64,"extensionId":"timers","commandName":"manageTimers"}),
    )
}
