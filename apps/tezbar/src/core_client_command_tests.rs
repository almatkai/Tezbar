use super::*;
use crate::surfaces::extensions_store::request::{ExtensionRequest, ExtensionTransport};
#[path = "../../../tests/fixtures/extensions/native_harness.rs"]
mod native_harness;
#[test]
fn review_m9_followup_command_quit_refusal_is_specific_and_non_destructive() {
    // A current-thread runtime makes the interval deterministic: send eagerly
    // admits/spawns the real core request before any worker task is polled.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let prepared = std::path::PathBuf::from(std::env::var_os("TEZBAR_M09_PREPARED").unwrap());
    let bun = std::path::PathBuf::from(std::env::var_os("TEZBAR_M09_BUN").unwrap());
    rt.block_on(async {
        let f = native_harness::Fixture::isolated(&prepared, bun, rt.handle().clone())
            .await
            .unwrap();
        let client = CoreClient::new(f.transport.core.clone());
        client.bind_extension_runtime(rt.handle().clone());
        let pending = ExtensionTransport::send(
            &client,
            ExtensionRequest::Run {
                extension: "raycast.m09-native-fixtures".into(),
                command: "invalidation".into(),
                arguments: Default::default(),
            },
        );
        let error = ExtensionTransport::drain(&client).await.unwrap_err();
        assert_eq!(
            error,
            "Extension command still running. Wait for it to finish, then quit."
        );
        client.resume_extensions();
        let result = pending.await.unwrap();
        assert_eq!(result["ok"], true);
        ExtensionTransport::drain(&client).await.unwrap();
        f.transport.shutdown().await;
    });
}
