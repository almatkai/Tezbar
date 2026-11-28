//! Device-owner authentication: Touch ID with the Mac login password as the
//! system fallback, Windows Hello on Windows. The vault key is only read after
//! this succeeds.
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

/// Windows: Windows Hello (PIN, fingerprint or face) through `UserConsentVerifier`,
/// parented to the foreground Tezbar window so the prompt is modal to it.
#[cfg(windows)]
impl DeviceAuthenticator for SystemAuthenticator {
    fn authenticate(&self, reason: &str) -> AuthFuture {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let message = format!("Tezbar wants to {reason}.");
        // WinRT waits block; keep them off the async runtime and the UI thread.
        let spawned = std::thread::Builder::new()
            .name("vault-windows-hello".into())
            .spawn(move || {
                let _ = tx.send(windows_hello::verify(&message));
            });
        Box::pin(async move {
            if let Err(e) = spawned {
                return AuthOutcome::Failed(format!("Could not start Windows Hello: {e}"));
            }
            rx.await
                .unwrap_or_else(|_| AuthOutcome::Failed("Authentication was interrupted".into()))
        })
    }
}

#[cfg(windows)]
mod windows_hello {
    use super::AuthOutcome;
    use windows::core::HSTRING;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::System::WinRT::{
        IUserConsentVerifierInterop, RoInitialize, RO_INIT_MULTITHREADED,
    };

    pub(super) const SETUP: &str = "Set up Windows Hello (a PIN, fingerprint or face) in Settings > Accounts > Sign-in options to use Vault";

    pub(super) fn availability_outcome(
        availability: UserConsentVerifierAvailability,
    ) -> Option<AuthOutcome> {
        match availability {
            UserConsentVerifierAvailability::Available => None,
            UserConsentVerifierAvailability::DeviceNotPresent
            | UserConsentVerifierAvailability::NotConfiguredForUser => {
                Some(AuthOutcome::Failed(SETUP.into()))
            }
            UserConsentVerifierAvailability::DisabledByPolicy => Some(AuthOutcome::Failed(
                "Windows Hello is disabled by your organization's policy".into(),
            )),
            UserConsentVerifierAvailability::DeviceBusy => Some(AuthOutcome::Failed(
                "Windows Hello is busy. Try again in a moment".into(),
            )),
            _ => Some(AuthOutcome::Failed("Windows Hello is unavailable".into())),
        }
    }

    pub(super) fn result_outcome(result: UserConsentVerificationResult) -> AuthOutcome {
        match result {
            UserConsentVerificationResult::Verified => AuthOutcome::Granted,
            UserConsentVerificationResult::Canceled => AuthOutcome::Cancelled,
            UserConsentVerificationResult::DeviceNotPresent
            | UserConsentVerificationResult::NotConfiguredForUser => {
                AuthOutcome::Failed(SETUP.into())
            }
            UserConsentVerificationResult::DisabledByPolicy => AuthOutcome::Failed(
                "Windows Hello is disabled by your organization's policy".into(),
            ),
            UserConsentVerificationResult::DeviceBusy => {
                AuthOutcome::Failed("Windows Hello is busy. Try again in a moment".into())
            }
            UserConsentVerificationResult::RetriesExhausted => {
                AuthOutcome::Failed("Too many attempts. Try again later".into())
            }
            _ => AuthOutcome::Failed("Windows Hello could not verify you".into()),
        }
    }

    pub fn verify(message: &str) -> AuthOutcome {
        // Already-initialized apartments are fine; the call only has to succeed once.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let available = UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.get());
        match available {
            Ok(availability) => {
                if let Some(outcome) = availability_outcome(availability) {
                    return outcome;
                }
            }
            Err(e) => return AuthOutcome::Failed(format!("Windows Hello is unavailable: {e}")),
        }
        let owner = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        let message = HSTRING::from(message);
        let outcome = (|| -> windows::core::Result<UserConsentVerificationResult> {
            if !owner.is_null() {
                let interop = windows::core::factory::<
                    UserConsentVerifier,
                    IUserConsentVerifierInterop,
                >()?;
                let operation: windows_future::IAsyncOperation<UserConsentVerificationResult> =
                    unsafe {
                        interop.RequestVerificationForWindowAsync(
                            windows::Win32::Foundation::HWND(owner as _),
                            &message,
                        )?
                    };
                return operation.get();
            }
            UserConsentVerifier::RequestVerificationAsync(&message)?.get()
        })();
        match outcome {
            Ok(result) => result_outcome(result),
            Err(e) => AuthOutcome::Failed(format!("Windows Hello could not run: {e}")),
        }
    }
}

#[cfg(windows)]
#[cfg(test)]
mod windows_tests {
    use super::*;
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult as R, UserConsentVerifierAvailability as A,
    };
    #[test]
    fn windows_hello_outcomes_map_to_vault_outcomes() {
        assert_eq!(windows_hello::availability_outcome(A::Available), None);
        assert_eq!(
            windows_hello::availability_outcome(A::NotConfiguredForUser),
            Some(AuthOutcome::Failed(windows_hello::SETUP.into()))
        );
        assert_eq!(windows_hello::result_outcome(R::Verified), AuthOutcome::Granted);
        assert_eq!(windows_hello::result_outcome(R::Canceled), AuthOutcome::Cancelled);
        assert!(matches!(
            windows_hello::result_outcome(R::RetriesExhausted),
            AuthOutcome::Failed(_)
        ));
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
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
