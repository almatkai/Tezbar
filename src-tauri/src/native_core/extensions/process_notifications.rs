//! Generic OS notification identity; no extension domain logic.
#[cfg(target_os = "macos")]
fn bundled() -> bool {
    use objc2_foundation::NSBundle;
    let b = NSBundle::mainBundle();
    b.bundleIdentifier().is_some()
        && b.bundleURL()
            .path()
            .is_some_and(|p| p.to_string().ends_with(".app"))
}
#[cfg(target_os = "macos")]
pub(super) async fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    if !bundled() {
        return Err("Removable notifications require a packaged Tezbar application".into());
    }
    use block2::RcBlock;
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
    };
    // Permission is requested on explicit use, never app startup/catalog access.
    let permission = {
        use objc2::runtime::Bool;
        use objc2_user_notifications::UNAuthorizationOptions;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let callback = RcBlock::new(
            move |granted: Bool, error: *mut objc2_foundation::NSError| {
                let result = if !granted.as_bool() {
                    Err("Notification permission was not granted".to_string())
                } else if let Some(e) = unsafe { error.as_ref() } {
                    Err(e.localizedDescription().to_string())
                } else {
                    Ok(())
                };
                if let Ok(mut tx) = tx.lock() {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(result);
                    }
                }
            },
        );
        UNUserNotificationCenter::currentNotificationCenter()
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert,
                &callback,
            );
        rx
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), permission)
        .await
        .map_err(|_| "Notification permission timed out")?
        .map_err(|_| "Notification permission callback closed")??;
    let rx = {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(id),
            &content,
            None,
        );
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let callback = RcBlock::new(move |error: *mut objc2_foundation::NSError| {
            let outcome = if let Some(e) = unsafe { error.as_ref() } {
                Err(e.localizedDescription().to_string())
            } else {
                Ok(())
            };
            if let Ok(mut tx) = tx.lock() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(outcome);
                }
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&callback));
        rx
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .map_err(|_| "Notification timed out")?
        .map_err(|_| "Notification callback closed")?
}
#[cfg(target_os = "macos")]
pub(super) async fn remove(id: &str) -> Result<(), String> {
    if !bundled() {
        return Err("Removable notifications require a packaged Tezbar application".into());
    }
    use objc2_foundation::{NSArray, NSString};
    use objc2_user_notifications::UNUserNotificationCenter;
    let id = NSString::from_str(id);
    let ids = NSArray::from_slice(&[&*id]);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removePendingNotificationRequestsWithIdentifiers(&ids);
    center.removeDeliveredNotificationsWithIdentifiers(&ids);
    Ok(())
}
#[cfg(target_os = "windows")]
async fn powershell(script: String) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), cmd.output())
        .await
        .map_err(|_| "Notification command timed out")?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err("Windows notification submission failed".into())
    }
}
#[cfg(target_os = "windows")]
fn tag(id: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(id.as_bytes()))[..16].to_string()
}
#[cfg(target_os = "windows")]
pub(super) async fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
    let xml=format!("<toast><visual><binding template='ToastGeneric'><text>{}</text><text>{}</text></binding></visual></toast>",xml(title),xml(body)).replace('\'',"''");
    powershell(format!("$ErrorActionPreference='Stop';[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null;[Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom,ContentType=WindowsRuntime]>$null;$xml=New-Object Windows.Data.Xml.Dom.XmlDocument;$xml.LoadXml('{}');$toast=[Windows.UI.Notifications.ToastNotification]::new($xml);$toast.Tag='{}';$toast.Group='TezbarExtensions';[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Tezbar').Show($toast)",xml,tag(id))).await
}
#[cfg(target_os = "windows")]
pub(super) async fn remove(id: &str) -> Result<(), String> {
    powershell(format!("$ErrorActionPreference='Stop';[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null;[Windows.UI.Notifications.ToastNotificationManager]::History.Remove('{}','TezbarExtensions','Tezbar')",tag(id))).await
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) async fn show(_id: &str, _title: &str, _body: &str) -> Result<(), String> {
    Err("Notifications unsupported on this platform".into())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) async fn remove(_id: &str) -> Result<(), String> {
    Err("Notifications unsupported on this platform".into())
}
