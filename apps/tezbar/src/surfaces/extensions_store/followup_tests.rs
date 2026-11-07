use super::*;
use ::core::prelude::v1::test;
struct FailedRun;
impl ExtensionTransport for FailedRun {
    fn send(&self, _: ExtensionRequest) -> ExtensionFuture {
        Box::pin(async {
            Ok(json!({"ok":false,"message":"The fixture needs its missing account setting."}))
        })
    }
    fn drain(&self) -> ExtensionFuture {
        Box::pin(async { Ok(json!(true)) })
    }
}
#[gpui::test]
fn review_m9_followup_store_preserves_the_workers_failure_message(cx: &mut TestAppContext) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
    let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(Arc::new(FailedRun), w, cx));
    view.update(cx, |v, cx| {
        v.visible = true;
        v.run("fixture".into(), "fail".into(), cx);
    });
    rt.block_on(async {
        for _ in 0..128 {
            cx.run_until_parked();
            tokio::task::yield_now().await;
        }
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.error.clone()).as_deref(),
        Some("The fixture needs its missing account setting.")
    );
}
