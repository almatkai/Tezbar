//! Canonical history preview and deletion races through the public AI channel.
use super::tests::fixture;
use super::{
    runtime::{
        store,
        transcript::{Block, Message},
    },
    *,
};
use sha2::{Digest, Sha256};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
async fn append(core: &AiCore, session: &str, id: &str, role: &str) {
    let reply=core.handle("chat:append",&[json!({"session":{"id":session,"title":"fixture","createdAt":1,"updatedAt":1},"turn":{"id":id,"role":role,"text":"fixture","createdAt":1,"attachments":[{"kind":"image","mimeType":"image/png","name":"fixture"}]}})]).await.unwrap().unwrap();
    assert_eq!(reply["ok"], true);
}
async fn preview(core: &AiCore, session: &str, turn: &str, index: u64) -> Value {
    core.handle(
        "chat:image",
        &[json!({"sessionId":session,"turnId":turn,"index":index})],
    )
    .await
    .unwrap()
    .unwrap()
}
#[tokio::test]
async fn history_image_is_canonical_user_owned_and_keeps_missing_blob_indices() {
    let (_dir, core, _) = fixture();
    append(&core, "images", "u", "user").await;
    append(&core, "images", "assistant", "assistant").await;
    append(&core, "images", "legacy", "user").await;
    let mut user = Message::text("user", "images");
    user.blocks.extend([
        Block::Image {
            mime: "image/png".into(),
            data: "YQ==".into(),
        },
        Block::Image {
            mime: "image/png".into(),
            data: "Yg==".into(),
        },
    ]);
    let root = &core.inner.ctx.paths.data_dir;
    store::save(root, "images", "u", &[user.clone()]).unwrap();
    // An assistant transcript may contain a user's image; it must never be used
    // as though the assistant turn owned that attachment.
    store::save(
        root,
        "images",
        "assistant",
        &[user, Message::text("assistant", "answer")],
    )
    .unwrap();
    assert_eq!(
        preview(&core, "images", "u", 0).await,
        json!({"mimeType":"image/png","data":"YQ=="})
    );
    let blob = root
        .join("llm/transcripts")
        .join(hash(b"images"))
        .join("blobs")
        .join(hash(b"a"));
    std::fs::remove_file(blob).unwrap();
    assert!(preview(&core, "images", "u", 0).await.is_null());
    assert_eq!(preview(&core, "images", "u", 1).await["data"], "Yg==");
    for (session, turn) in [
        ("other", "u"),
        ("images", "assistant"),
        ("images", "legacy"),
    ] {
        assert!(preview(&core, session, turn, 0).await.is_null());
    }
    assert!(core
        .handle(
            "chat:image",
            &[json!({"sessionId":"images","turnId":"u","index":8})]
        )
        .await
        .unwrap()
        .is_err());
    core.handle("chat:delete", &[json!("images")])
        .await
        .unwrap()
        .unwrap();
    assert!(preview(&core, "images", "u", 1).await.is_null());
    core.shutdown().await;
}
#[tokio::test]
async fn history_image_clear_tombstones_owned_preparing_sessions_before_late_append() {
    let (_dir, core, _) = fixture();
    let reply = core
        .handle(
            "chat:clear",
            &[Value::Null, json!(["not-yet-appended", "not-yet-appended"])],
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reply["ok"], true);
    let late=core.handle("chat:append",&[json!({"session":{"id":"not-yet-appended","title":"fixture","createdAt":1,"updatedAt":1},"turn":{"id":"u","role":"user","text":"late","createdAt":1}})]).await.unwrap().unwrap();
    assert_eq!(late["ok"], false);
    assert!(core
        .handle("chat:get", &[json!("not-yet-appended")])
        .await
        .unwrap()
        .unwrap()
        .is_null());
    append(&core, "keep", "u", "user").await;
    assert!(core
        .handle("chat:clear", &[Value::Null, json!([17])])
        .await
        .unwrap()
        .is_err());
    assert!(!core
        .handle("chat:get", &[json!("keep")])
        .await
        .unwrap()
        .unwrap()
        .is_null());
    core.shutdown().await;
}
