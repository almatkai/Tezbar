//! Defer ordinary AppKit termination until the application's owned resources have drained.
//! Adds one absent method to the pinned GPUI delegate; never replaces its delegate or callbacks.
use crate::events::{Event, Relay};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Running,
    Draining,
    Drained,
}
#[derive(Default)]
pub struct State {
    pub phase: Phase,
    pub native_pending: bool,
}
impl State {
    pub fn request(&mut self) -> (u64, bool) {
        match self.phase {
            Phase::Running => {
                self.phase = Phase::Draining;
                self.native_pending = true;
                (2, true)
            }
            Phase::Draining => {
                self.native_pending = true;
                (2, false)
            }
            Phase::Drained => (1, false),
        }
    }
    pub fn complete(&mut self, success: bool) -> bool {
        self.phase = if success {
            Phase::Drained
        } else {
            Phase::Running
        };
        std::mem::take(&mut self.native_pending)
    }
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use objc2::{
        MainThreadMarker,
        runtime::{AnyClass, AnyObject, Bool, Sel},
        sel,
    };
    use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
    use std::{cell::RefCell, sync::Weak};
    type WeakRelay = Weak<crate::events::Mailbox>;
    thread_local! {static OWNER:RefCell<Option<(State,WeakRelay)>>=const{RefCell::new(None)};}
    #[link(name = "objc")]
    unsafe extern "C" {
        fn class_addMethod(
            class: *const AnyClass,
            selector: Sel,
            implementation: unsafe extern "C-unwind" fn(),
            types: *const std::ffi::c_char,
        ) -> Bool;
    }
    unsafe extern "C-unwind" fn should_terminate(
        _delegate: &AnyObject,
        _selector: Sel,
        _app: &NSApplication,
    ) -> NSApplicationTerminateReply {
        if MainThreadMarker::new().is_none() {
            return NSApplicationTerminateReply::TerminateCancel;
        }
        OWNER.with(|owner| {
            let mut owner = owner.borrow_mut();
            let Some((state, relay)) = owner.as_mut() else {
                return NSApplicationTerminateReply::TerminateCancel;
            };
            let (reply, enqueue) = state.request();
            if enqueue {
                let sent = relay
                    .upgrade()
                    .and_then(|relay| Relay::from_mailbox(relay).send(Event::Quit).ok());
                if sent.is_none() {
                    state.complete(false);
                    return NSApplicationTerminateReply::TerminateCancel;
                }
            }
            NSApplicationTerminateReply(reply as _)
        })
    }
    pub fn install(relay: &Relay) -> Result<(), String> {
        let mtm = MainThreadMarker::new()
            .ok_or("Termination adapter must be installed on GUI main thread")?;
        let app = NSApplication::sharedApplication(mtm);
        let delegate = app
            .delegate()
            .ok_or("GPUI application delegate unavailable")?;
        let delegate_object = unsafe { &*(&*delegate as *const _ as *const AnyObject) };
        let class = delegate_object.class();
        if class.name() != c"GPUIApplicationDelegate" {
            return Err("Unexpected GPUI delegate class".into());
        }
        let selector = sel!(applicationShouldTerminate:);
        if class.instance_method(selector).is_some() {
            return Err(
                "applicationShouldTerminate: already exists; refusing to overwrite it".into(),
            );
        }
        let imp: unsafe extern "C-unwind" fn() = unsafe {
            std::mem::transmute(
                should_terminate
                    as unsafe extern "C-unwind" fn(
                        &AnyObject,
                        Sel,
                        &NSApplication,
                    )
                        -> NSApplicationTerminateReply,
            )
        };
        if !unsafe { class_addMethod(class, selector, imp, c"Q@:@".as_ptr()) }.as_bool() {
            return Err("Failed to add applicationShouldTerminate:".into());
        }
        let method = class
            .instance_method(selector)
            .ok_or("Termination selector missing after installation")?;
        if method.return_type().as_ref() != c"Q"
            || method.argument_type(2).as_ref().map(|s| s.as_ref()) != Some(c"@")
        {
            return Err("Unexpected termination selector ABI".into());
        }
        OWNER.with(|owner| *owner.borrow_mut() = Some((State::default(), relay.downgrade())));
        Ok(())
    }
    pub fn complete(success: bool) -> bool {
        let pending = OWNER.with(|owner| {
            owner
                .borrow_mut()
                .as_mut()
                .map(|(state, _)| state.complete(success))
                .unwrap_or(false)
        });
        if pending {
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(success);
            }
        }
        pending
    }
}
#[cfg(target_os = "macos")]
pub use mac::{complete, install};
#[cfg(not(target_os = "macos"))]
pub fn install(_: &Relay) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub fn complete(_: bool) -> bool {
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn two_native_quits_queue_once_and_reentry_is_immediate_after_drain() {
        let mut state = State::default();
        assert_eq!(state.request(), (2, true));
        assert_eq!(state.request(), (2, false));
        assert!(state.complete(true));
        assert_eq!(state.request(), (1, false));
    }
    #[::core::prelude::v1::test]
    fn save_failure_replies_no_and_can_retry() {
        let mut state = State::default();
        state.request();
        assert!(state.complete(false));
        assert_eq!(state.phase, Phase::Running);
        assert_eq!(state.request(), (2, true));
    }
}

#[cfg(all(feature = "bench-control", target_os = "macos"))]
pub fn request_native() -> Result<(), String> {
    objc2::MainThreadMarker::new().ok_or("Native termination requires GUI main thread")?;
    // A Cocoa run-loop timer is intentional: NSTerminateLater can run a nested AppKit loop.
    // Calling terminate inside a GCD main-queue block prevents that same queue from reentering.
    let mtm =
        objc2::MainThreadMarker::new().ok_or("Native termination requires GUI main thread")?;
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    unsafe {
        let _: () = objc2::msg_send![&*app,performSelector:objc2::sel!(terminate:),withObject:std::ptr::null::<objc2::runtime::AnyObject>(),afterDelay:0.0f64];
    }
    Ok(())
}
#[cfg(all(feature = "bench-control", not(target_os = "macos")))]
pub fn request_native() -> Result<(), String> {
    Err("Native termination fixture requires macOS".into())
}
