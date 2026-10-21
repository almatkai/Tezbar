//! Shared compiled-in Native V2 SDK. Storage never touches legacy extension storage.
use crate::native_core::common::{write_json_atomic, CoreContext, CoreResult};
use serde_json::{json, Value};
use std::sync::Mutex;

// One blocking transaction lane prevents lost updates across SDK callers/contexts.
static STORE_LOCK: Mutex<()> = Mutex::new(());

pub fn view(title: &str, root: Value, actions: Vec<Value>) -> Value {
    json!({"ok":true,"mode":"view","title":title,"root":root,"actions":actions})
}
pub fn no_view(message: &str) -> Value {
    json!({"ok":true,"mode":"no-view","message":message})
}
pub fn unchanged() -> Value {
    json!({"ok":true,"mode":"unchanged"})
}
pub async fn open(ctx: &CoreContext, target: &str) -> CoreResult {
    ctx.request("shell:open", vec![json!(target)]).await
}
pub async fn copy(ctx: &CoreContext, text: &str) -> CoreResult {
    ctx.request("clipboard:write", vec![json!(text)]).await
}
fn store_path(ctx: &CoreContext, slug: &str) -> Result<std::path::PathBuf, String> {
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid native extension storage slug".into());
    }
    Ok(ctx
        .paths
        .data_dir
        .join("native-extensions-v2")
        .join(slug)
        .join("storage.json"))
}
fn read(path: &std::path::Path) -> CoreResult {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if !value.is_object() {
                return Err("Native extension storage must be an object".into());
            }
            Ok(value)
        }
    }
}
pub async fn read_store(ctx: &CoreContext, slug: &str) -> CoreResult {
    let path = store_path(ctx, slug)?;
    tokio::task::spawn_blocking(move || {
        let _guard = STORE_LOCK.lock().map_err(|e| e.to_string())?;
        read(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}
pub async fn update_store<F>(ctx: &CoreContext, slug: &str, change: F) -> CoreResult
where
    F: FnOnce(&mut Value) -> Result<(), String> + Send + 'static,
{
    let path = store_path(ctx, slug)?;
    tokio::task::spawn_blocking(move || {
        let _guard = STORE_LOCK.lock().map_err(|e| e.to_string())?;
        let mut value = read(&path)?;
        change(&mut value)?;
        if !value.is_object() {
            return Err("Native extension storage must remain an object".into());
        }
        write_json_atomic(&path, &value)?;
        Ok(value)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn encode(text: &str, form: bool) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"*-._".contains(&b) || (!form && b"!~'()".contains(&b)) {
            out.push(b as char);
        } else if form && b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}
pub fn encode_form_query(text: &str) -> String {
    encode(text, true)
}
pub fn encode_component(text: &str) -> String {
    encode(text, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_core::common::CorePaths;
    use std::sync::Arc;
    fn context(dir: &std::path::Path) -> CoreContext {
        CoreContext::headless(CorePaths {
            data_dir: dir.join("data"),
            config_dir: dir.join("config"),
            resource_dir: dir.join("resources"),
            temp_dir: dir.join("tmp"),
            home_dir: dir.into(),
        })
    }
    #[tokio::test]
    async fn store_serializes_updates_preserves_unknown_keys_and_rejects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        assert_eq!(read_store(&ctx, "fixture").await.unwrap(), json!({}));
        update_store(&ctx, "fixture", |v| {
            *v = json!({"unknown":{"keep":true},"count":0});
            Ok(())
        })
        .await
        .unwrap();
        let mut tasks = Vec::new();
        for _ in 0..30 {
            let ctx = ctx.clone();
            tasks.push(tokio::spawn(async move {
                update_store(&ctx, "fixture", |v| {
                    v["count"] = json!(v["count"].as_u64().unwrap() + 1);
                    Ok(())
                })
                .await
                .unwrap()
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(
            read_store(&ctx, "fixture").await.unwrap(),
            json!({"unknown":{"keep":true},"count":30})
        );
        assert!(update_store(&ctx, "fixture", |v| {
            v["count"] = json!(99);
            Err("rejected".into())
        })
        .await
        .is_err());
        assert_eq!(read_store(&ctx, "fixture").await.unwrap()["count"], 30);
        let path = store_path(&ctx, "fixture").unwrap();
        std::fs::write(&path, b"{broken").unwrap();
        assert!(read_store(&ctx, "fixture").await.is_err());
        assert!(update_store(&ctx, "fixture", |_| Ok(())).await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{broken");
        for slug in ["", "..", "../escape", "/absolute", "a/b", "a\\b"] {
            assert!(read_store(&ctx, slug).await.is_err());
        }
    }
    #[tokio::test]
    async fn routes_actual_operations_and_preserves_failure() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        ctx.set_router(Arc::new(|channel, args| {
            Box::pin(async move {
                assert_eq!(args, vec![json!("Қазақ")]);
                match channel.as_str() {
                    "shell:open" => Err("open failure".into()),
                    "clipboard:write" => Err("copy failure".into()),
                    _ => panic!("Unexpected channel: {channel}"),
                }
            })
        }))
        .unwrap();
        assert_eq!(open(&ctx, "Қазақ").await.unwrap_err(), "open failure");
        assert_eq!(copy(&ctx, "Қазақ").await.unwrap_err(), "copy failure");
    }
    #[test]
    fn encoders_match_browser_utf8_rules() {
        assert_eq!(
            encode_form_query("日本 😀 +~!*'()"),
            "%E6%97%A5%E6%9C%AC+%F0%9F%98%80+%2B%7E%21*%27%28%29"
        );
        assert_eq!(
            encode_component("日本 😀 +~!*'()"),
            "%E6%97%A5%E6%9C%AC%20%F0%9F%98%80%20%2B~!*'()"
        );
    }
}
