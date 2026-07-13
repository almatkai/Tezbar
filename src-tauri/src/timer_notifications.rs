use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSSet, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

const TIMER_CATEGORY: &str = "raymes.timer-complete";
const STOP_ACTION: &str = "raymes.timer.stop";
const SHOW_ACTION: &str = "raymes.timer.show";

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

fn native_notifications_available() -> bool {
    let bundle = NSBundle::mainBundle();
    let has_identifier = bundle.bundleIdentifier().is_some();
    let is_app_bundle = bundle
        .bundleURL()
        .path()
        .is_some_and(|path| path.to_string().ends_with(".app"));

    has_identifier && is_app_bundle
}

fn deliver_bundleless_notification(name: &str) {
    const SCRIPT: &str = r#"on run argv
display notification ("Timer \"" & item 1 of argv & "\" complete") with title "Ding!"
end run"#;

    let name = name.to_owned();
    std::thread::spawn(move || {
        match Command::new("/usr/bin/osascript")
            .args(["-e", SCRIPT, "--", &name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if !status.success() => {
                log::warn!("bundleless timer notification exited with {status}");
            }
            Err(error) => {
                log::warn!("failed to deliver bundleless timer notification: {error}");
            }
            _ => {}
        }
    });
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class has no ivars.
    #[unsafe(super = NSObject)]
    #[name = "RaymesTimerNotificationDelegate"]
    struct TimerNotificationDelegate;

    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for TimerNotificationDelegate {}

    // SAFETY: The method signatures below match UNUserNotificationCenterDelegate.
    unsafe impl UNUserNotificationCenterDelegate for TimerNotificationDelegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &DynBlock<dyn Fn()>,
        ) {
            let action = response.actionIdentifier().to_string();
            let timer_file = response.notification().request().identifier().to_string();

            if action == STOP_ACTION {
                stop_timer(&timer_file);
            } else if action == SHOW_ACTION
                || action
                    == unsafe {
                        objc2_user_notifications::UNNotificationDefaultActionIdentifier.to_string()
                    }
            {
                show_app();
            }
            completion_handler.call(());
        }

        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &objc2_user_notifications::UNNotification,
            completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl TimerNotificationDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: This calls NSObject's standard initializer on a newly allocated subclass.
        unsafe { msg_send![super(this), init] }
    }
}

fn show_app() {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("window-shown", serde_json::json!({ "resetUi": false }));
    }
}

fn stop_timer(timer_file: &str) {
    let pid = fs::read_to_string(timer_file)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|contents| contents.get("pid").and_then(Value::as_i64));

    let dismiss_file = timer_file
        .strip_suffix(".timer")
        .map(|path| format!("{path}.dismiss"));
    if let Some(dismiss_file) = dismiss_file {
        let _ = fs::remove_file(dismiss_file);
    }
    let _ = fs::remove_file(timer_file);

    if let Some(pid) = pid.filter(|pid| *pid > 0) {
        let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
    }
}

pub fn setup(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());

    // UserNotifications raises an Objective-C exception when a development
    // executable is launched directly from target/debug instead of a .app bundle.
    if !native_notifications_available() {
        log::info!(
            "native timer notifications are unavailable outside an app bundle; using AppleScript fallback"
        );
        return;
    }

    let stop = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(STOP_ACTION),
        &NSString::from_str("Stop"),
        UNNotificationActionOptions::Destructive,
    );
    let show = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str(SHOW_ACTION),
        &NSString::from_str("Show"),
        UNNotificationActionOptions::Foreground,
    );
    let actions = NSArray::from_slice(&[&*stop, &*show]);
    let intent_identifiers = NSArray::<NSString>::new();
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str(TIMER_CATEGORY),
        &actions,
        &intent_identifiers,
        UNNotificationCategoryOptions::empty(),
    );

    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.setNotificationCategories(&NSSet::from_slice(&[&*category]));

    let delegate = TimerNotificationDelegate::new();
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // UNUserNotificationCenter's delegate is weak. Keep this process-lifetime delegate alive.
    std::mem::forget(delegate);

    let authorization_callback = RcBlock::new(|granted: Bool, error: *mut NSError| {
        if !granted.as_bool() {
            log::warn!("timer notification permission was not granted: {:?}", error);
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert,
        &authorization_callback,
    );
}

pub fn deliver(name: &str, timer_file: &str) {
    if !native_notifications_available() {
        deliver_bundleless_notification(name);
        return;
    }

    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str("Ding!"));
    content.setBody(&NSString::from_str(&format!("Timer \"{name}\" complete")));
    content.setCategoryIdentifier(&NSString::from_str(TIMER_CATEGORY));

    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(timer_file),
        &content,
        None,
    );
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
}
