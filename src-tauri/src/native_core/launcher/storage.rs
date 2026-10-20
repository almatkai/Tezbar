use super::db::Database;
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};
use std::path::Path;
fn size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
pub async fn breakdown(ctx: CoreContext) -> CoreResult {
    tokio::task::spawn_blocking(move||{let root=ctx.paths.data_dir;let search=root.join("search");let mut items=vec![];for(id,label,paths)in[("clipboard-images","Clipboard images",vec![search.join("clipboard-images")]),("search-db","Search index & history",vec![search.join("index.sqlite3"),search.join("index.sqlite3-wal"),search.join("index.sqlite3-shm"),search.join("clipboard.json")]),("knowledge-index","Knowledge index",vec![root.join("knowledge")]),("voice-models","Voice models",vec![root.join("voice-models")]),("bun","Extension installer (Bun)",vec![root.join("bun")]),("extensions","Installed extensions",vec![root.join("extensions")]),("chromium-cache","Chromium cache",vec![root.join("Cache"),root.join("Code Cache")])]{let bytes=paths.iter().map(|p|size(p)).sum::<u64>();items.push(json!({"id":id,"label":label,"bytes":bytes,"paths":if id=="search-db"{vec![search.clone()]}else{paths}}));}let total=items.iter().filter_map(|i|i["bytes"].as_u64()).sum::<u64>();Ok(json!({"items":items,"totalBytes":total}))}).await.map_err(|e|e.to_string())?
}
pub async fn vacuum(db: Database, ctx: CoreContext) -> CoreResult {
    db.call(move |db| {
        let wal = ctx.paths.data_dir.join("search/index.sqlite3-wal");
        let before = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
            .map_err(|e| e.to_string())?;
        let after = std::fs::metadata(wal).map(|m| m.len()).unwrap_or(0);
        Ok(json!({"beforeBytes":before,"afterBytes":after}))
    })
    .await
}
pub fn clipboard_config(raw: &Value) -> Value {
    json!({"watchEnabled":raw["clipboardWatchEnabled"]!=false,"captureImages":raw["clipboardCaptureImages"]==true,"maxImageMegapixels":raw["clipboardMaxImageMegapixels"].as_f64().filter(|n|*n>0.).unwrap_or(2.)})
}
