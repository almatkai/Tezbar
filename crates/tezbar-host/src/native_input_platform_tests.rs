use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn pointer_selects_display_with_negative_origin_and_shared_edges() {
    assert!(contains_point(-1920.0, -200.0, 1920.0, 1080.0, -1.0, 20.0));
    assert!(!contains_point(-1920.0, -200.0, 1920.0, 1080.0, 0.0, 20.0));
    assert!(contains_point(0.0, 0.0, 2560.0, 1440.0, 0.0, 20.0));
    assert!(!contains_point(0.0, 0.0, 2560.0, 1440.0, 5.0, 1440.0));
}

#[tokio::test]
async fn cancellation_drops_protection_guard() {
    let count = Arc::new(AtomicUsize::new(0));
    let restored = count.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _guard = RestoreOnDrop::new(move || {
            restored.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        send.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    receive.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn explicit_restoration_is_once_and_failed_restoration_retries_on_drop() {
    let count = AtomicUsize::new(0);
    {
        let mut guard = RestoreOnDrop::new(|| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        guard.restore().unwrap();
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    {
        let mut guard = RestoreOnDrop::new(|| {
            if count.fetch_add(1, Ordering::SeqCst) == 1 {
                Err("temporary failure".into())
            } else {
                Ok(())
            }
        });
        assert!(guard.restore().is_err());
    }
    assert_eq!(count.load(Ordering::SeqCst), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_capture_request_keeps_blocking_task_cleanup_alive() {
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let (restored, cleanup) = tokio::sync::oneshot::channel();
    let request = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            let mut restored = Some(restored);
            let _guard = RestoreOnDrop::new(move || {
                if let Some(send) = restored.take() {
                    let _ = send.send(());
                }
                Ok(())
            });
            started.send(()).unwrap();
            wait.recv().unwrap();
        })
        .await
        .unwrap();
    });
    ready.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), cleanup)
        .await
        .unwrap()
        .unwrap();
}
