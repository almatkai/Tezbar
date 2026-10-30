//! Device-owner authentication: Touch ID with the Mac login password as the
//! system fallback. The vault key is only read after this succeeds.
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    Granted,
    /// The user (or the system) dismissed the prompt.
    Cancelled,
    /// Authentication could not run or did not succeed.
    Failed(String),
}

pub type AuthFuture = Pin<Box<dyn Future<Output = AuthOutcome> + Send>>;

pub trait DeviceAuthenticator: Send + Sync {
    fn authenticate(&self, reason: &str) -> AuthFuture;
}

pub struct SystemAuthenticator;

#[cfg(target_os = "macos")]
impl DeviceAuthenticator for SystemAuthenticator {
    fn authenticate(&self, reason: &str) -> AuthFuture {
        let (tx, rx) = tokio::sync::oneshot::channel();
        // The Objective-C objects are not Send: create and hand them to the
        // framework synchronously, and only await the reply channel.
        macos::evaluate(reason, tx);
        Box::pin(async move {
            rx.await
                .unwrap_or_else(|_| AuthOutcome::Failed("Authentication was interrupted".into()))
        })
    }
}

#[cfg(not(target_os = "macos"))]
impl DeviceAuthenticator for SystemAuthenticator {
    fn authenticate(&self, _reason: &str) -> AuthFuture {
        Box::pin(async { AuthOutcome::Failed("Vault is available on macOS for now".into()) })
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::AuthOutcome;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAError, LAPolicy};
    use std::sync::Mutex;
    use tokio::sync::oneshot::Sender;

    fn outcome_for(error: &NSError) -> AuthOutcome {
        let code = error.code();
        let cancelled = [
            LAError::UserCancel.0,
            LAError::SystemCancel.0,
            LAError::AppCancel.0,
            LAError::UserFallback.0,
        ];
        if cancelled.contains(&code) {
            AuthOutcome::Cancelled
        } else {
            AuthOutcome::Failed(error.localizedDescription().to_string())
        }
    }

    pub fn evaluate(reason: &str, tx: Sender<AuthOutcome>) {
        let context = unsafe { LAContext::new() };
        let policy = LAPolicy::DeviceOwnerAuthentication;
        if let Err(error) = unsafe { context.canEvaluatePolicy_error(policy) } {
            let _ = tx.send(outcome_for(&error));
            return;
        }
        let tx = Mutex::new(Some(tx));
        // The block keeps the context alive until the reply arrives; a context
        // released mid-evaluation cancels the prompt.
        let keep_alive = context.clone();
        let reply = RcBlock::new(move |success: Bool, error: *mut NSError| {
            let _context = &keep_alive;
            let outcome = if success.as_bool() {
                AuthOutcome::Granted
            } else if let Some(error) = unsafe { error.as_ref() } {
                outcome_for(error)
            } else {
                AuthOutcome::Failed("Authentication failed".into())
            };
            if let Some(tx) = tx.lock().ok().and_then(|mut slot| slot.take()) {
                let _ = tx.send(outcome);
            }
        });
        unsafe {
            context.evaluatePolicy_localizedReason_reply(
                policy,
                &NSString::from_str(reason),
                &reply,
            )
        };
    }
}
