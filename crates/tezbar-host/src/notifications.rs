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
pub async fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    if !bundled() {
        return Err("Removable notifications require a packaged Tezbar application".into());
    }
    use block2::RcBlock;
    use objc2_user_notifications::UNUserNotificationCenter;
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
    submit(id, title, body).await
}
#[cfg(target_os = "macos")]
async fn submit(id: &str, title: &str, body: &str) -> Result<(), String> {
    use block2::RcBlock;
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
    };
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
pub async fn remove(id: &str) -> Result<(), String> {
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
fn tag(id: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(id.as_bytes()))[..16].to_string()
}
#[cfg(target_os = "windows")]
const APP_ID: &str = "Tezbar";
#[cfg(target_os = "windows")]
const GROUP: &str = "TezbarExtensions";
#[cfg(target_os = "windows")]
fn toast_xml(title: &str, body: &str) -> String {
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
    format!(
        "<toast><visual><binding template='ToastGeneric'><text>{}</text><text>{}</text></binding></visual></toast>",
        xml(title),
        xml(body)
    )
}
/// Toasts go through WinRT in-process (no PowerShell child).
#[cfg(target_os = "windows")]
pub async fn show(id: &str, title: &str, body: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    let (xml, tag) = (toast_xml(title, body), tag(id));
    tokio::task::spawn_blocking(move || -> windows::core::Result<()> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        toast.SetTag(&HSTRING::from(tag))?;
        toast.SetGroup(&HSTRING::from(GROUP))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|_| "Windows notification submission failed".to_string())
}
#[cfg(target_os = "windows")]
pub async fn remove(id: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::ToastNotificationManager;
    let tag = tag(id);
    tokio::task::spawn_blocking(move || -> windows::core::Result<()> {
        ToastNotificationManager::History()?.RemoveGroupedTagWithId(
            &HSTRING::from(tag),
            &HSTRING::from(GROUP),
            &HSTRING::from(APP_ID),
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|_| "Windows notification submission failed".to_string())
}
#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    #[test]
    fn toast_xml_escapes_user_text() {
        let xml = super::toast_xml("a <b> & 'c'", "\"d\"");
        assert!(xml.contains("a &lt;b&gt; &amp; &apos;c&apos;"));
        assert!(xml.contains("&quot;d&quot;"));
        windows::Data::Xml::Dom::XmlDocument::new()
            .unwrap()
            .LoadXml(&windows::core::HSTRING::from(xml))
            .unwrap();
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub async fn show(_id: &str, _title: &str, _body: &str) -> Result<(), String> {
    Err("Notifications unsupported on this platform".into())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub async fn remove(_id: &str) -> Result<(), String> {
    Err("Notifications unsupported on this platform".into())
}

#[cfg(target_os = "macos")]
pub async fn show_if_authorized(id: &str, title: &str, body: &str) -> Result<(), String> {
    if !bundled() {
        return Err("Terminal feedback requires a packaged Tezbar application".into());
    }
    use block2::RcBlock;
    use objc2_user_notifications::{
        UNAuthorizationStatus, UNNotificationSetting, UNNotificationSettings,
        UNUserNotificationCenter,
    };
    let rx = {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Mutex::new(Some(tx));
        let callback = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
            let settings = unsafe { settings.as_ref() };
            let allowed = settings.authorizationStatus() == UNAuthorizationStatus::Authorized
                && settings.alertSetting() == UNNotificationSetting::Enabled;
            if let Ok(mut tx) = tx.lock() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(allowed);
                }
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .getNotificationSettingsWithCompletionHandler(&callback);
        rx
    };
    let allowed = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .map_err(|_| "Notification settings timed out")?
        .map_err(|_| "Notification settings callback closed")?;
    if !allowed {
        return Err(
            "Terminal feedback requires notification alerts enabled in system settings".into(),
        );
    }
    submit(id, title, body).await
}
#[cfg(target_os = "windows")]
pub async fn show_if_authorized(id: &str, title: &str, body: &str) -> Result<(), String> {
    use windows::{core::HSTRING, UI::Notifications::{NotificationSetting, ToastNotificationManager}};
    let setting = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
        .and_then(|notifier| notifier.Setting()).map_err(|e| e.to_string())?;
    if setting != NotificationSetting::Enabled {
        return Err("Notification alerts are disabled".into());
    }
    show(id, title, body).await
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub async fn show_if_authorized(id: &str, title: &str, body: &str) -> Result<(), String> {
    show(id, title, body).await
}
