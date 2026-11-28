//! Included by the native runtime's registered real-SDK tests.
//! Caller constructs a real NativeCore with explicit WorkerLaunch and an isolated
//! profile. This file never starts a replacement worker or dispatches desktop OS APIs.
use crate::surfaces::extensions_store::request::{
    ExtensionFuture, ExtensionRequest, ExtensionTransport,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tezbar_core::{NativeCore, host::TestHost};
#[derive(Clone)]
pub struct CoreTransport {
    pub core: Arc<NativeCore>,
    runtime: tokio::runtime::Handle,
    active: Arc<AtomicUsize>,
    last_reply: Arc<Mutex<Value>>,
    settled: Arc<tokio::sync::Notify>,
    calls: Arc<Mutex<Vec<(&'static str, Vec<Value>)>>>,
}
struct Admitted {
    active: Arc<AtomicUsize>,
    settled: Arc<tokio::sync::Notify>,
}
impl Drop for Admitted {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
        self.settled.notify_waiters();
    }
}
impl CoreTransport {
    pub fn new(core: Arc<NativeCore>, runtime: tokio::runtime::Handle) -> Self {
        Self {
            core,
            runtime,
            active: Default::default(),
            last_reply: Default::default(),
            settled: Default::default(),
            calls: Default::default(),
        }
    }
    pub fn diagnostics_count(&self) -> usize {
        self.last_reply.lock().unwrap()["effects"]
            .as_array()
            .map_or(0, Vec::len)
    }
    pub fn count(&self, channel: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| *c == channel)
            .count()
    }
    pub fn last(&self, channel: &str) -> Option<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(c, _)| *c == channel)
            .and_then(|(_, a)| a.first().cloned())
    }
    pub async fn shutdown(&self) {
        let _ = self.drain().await;
        self.core.shutdown().await;
    }
    pub async fn run(&self, command: &str) -> Result<Value, String> {
        self.send(ExtensionRequest::Run {
            extension: "raycast.m09-native-fixtures".into(),
            command: command.into(),
            arguments: if command == "form" {
                [("original".into(), "kept".into())].into()
            } else {
                BTreeMap::new()
            },
        })
        .await
    }
}
impl ExtensionTransport for CoreTransport {
    fn send(&self, r: ExtensionRequest) -> ExtensionFuture {
        let (channel, args) = r.wire();
        self.calls.lock().unwrap().push((channel, args.clone()));
        self.active.fetch_add(1, Ordering::AcqRel);
        let permit = Admitted {
            active: self.active.clone(),
            settled: self.settled.clone(),
        };
        let core = self.core.clone();
        // Admission and job ownership precede polling. Cancellation of GPUI interest
        // cannot cancel an already admitted core mutation or leak its drain barrier.
        let last_reply = self.last_reply.clone();
        let task = self.runtime.spawn(async move {
            let _permit = permit;
            let result = core.request(channel, &args).await;
            if let Ok(value) = &result {
                *last_reply.lock().unwrap() = value.clone();
            }
            result
        });
        Box::pin(async move {
            task.await
                .map_err(|_| "Fixture core task interrupted".to_owned())?
        })
    }
    fn drain(&self) -> ExtensionFuture {
        let active = self.active.clone();
        let settled = self.settled.clone();
        Box::pin(async move {
            loop {
                let notify = settled.notified();
                tokio::pin!(notify);
                notify.as_mut().enable();
                if active.load(Ordering::Acquire) == 0 {
                    return Ok(json!(true));
                }
                notify.await;
            }
        })
    }
}
pub struct DesktopRecords(Arc<TestHost>);
impl DesktopRecords {
    pub fn count(&self, channel: &str) -> usize {
        use tezbar_core::host::DesktopEffect::*;
        self.0
            .desktop_effects
            .lock()
            .unwrap()
            .iter()
            .filter(|effect| {
                matches!(
                    (channel, *effect),
                    ("clipboard:write", WriteText(_))
                        | ("shell:open", Open(_))
                        | ("core:platform:reveal-file", Reveal(_))
                )
            })
            .count()
    }
    pub fn copied_lengths(&self) -> Vec<usize> {
        self.0
            .desktop_effects
            .lock()
            .unwrap()
            .iter()
            .filter_map(|effect| {
                if let tezbar_core::host::DesktopEffect::WriteText(text) = effect {
                    Some(text.len())
                } else {
                    None
                }
            })
            .collect()
    }
}
pub struct Fixture {
    profile: Option<Arc<tempfile::TempDir>>,
    pub transport: Arc<CoreTransport>,
    pub host: Arc<TestHost>,
    pub desktop: DesktopRecords,
}
impl Fixture {
    /// Build the REAL routed core using the parent's explicit worker constructor.
    /// Caller has already run prepare.mjs after the build gate and owns its output.
    /// No fixture is executed here, and desktop-action execution remains gated on
    /// the separate finite fake OS seam documented in integration.md.
    pub async fn isolated(
        prepared: &std::path::Path,
        bun: std::path::PathBuf,
        runtime: tokio::runtime::Handle,
    ) -> Result<Self, String> {
        use tezbar_core::{
            StartupOptions,
            common::{CoreContext, CorePaths},
            extension_worker::WorkerLaunch,
        };
        if std::env::var_os("SUPERCMD_EXTENSION_PATHS").is_some() {
            return Err("Fixture refuses external extension search paths".into());
        }
        let prepared: Value = serde_json::from_slice(
            &std::fs::read(prepared).map_err(|_| "Prepared fixture inputs unavailable")?,
        )
        .map_err(|_| "Invalid prepared fixture inputs")?;
        let bundle = std::path::PathBuf::from(
            prepared["bundle"]
                .as_str()
                .ok_or("Missing fixture bundle")?,
        );
        let seed =
            std::path::PathBuf::from(prepared["seed"].as_str().ok_or("Missing fixture seed")?);
        let profile = Arc::new(tempfile::tempdir().map_err(|_| "Fixture profile unavailable")?);
        fn copy_dir(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
            std::fs::create_dir_all(target).map_err(|_| "Fixture target unavailable")?;
            for e in std::fs::read_dir(source).map_err(|_| "Fixture seed unavailable")? {
                let e = e.map_err(|_| "Fixture seed entry unavailable")?;
                let kind = e
                    .file_type()
                    .map_err(|_| "Fixture seed metadata unavailable")?;
                if kind.is_symlink() {
                    return Err("Fixture seed symlink denied".into());
                }
                let target = target.join(e.file_name());
                if kind.is_dir() {
                    copy_dir(&e.path(), &target)?;
                } else if kind.is_file() {
                    std::fs::copy(e.path(), target).map_err(|_| "Fixture seed copy failed")?;
                }
            }
            Ok(())
        }
        copy_dir(&seed, profile.path())?;
        // Additive status probe overlay in this fixture's OWN profile. The
        // pre-existing prepared worker/seed and its evidence stay byte-unchanged.
        let package = profile.path().join("extensions/m09-native-fixtures");
        std::fs::write(
            package.join("src/probe-status-icons.tsx"),
            include_str!("src/probe-status-icons.tsx"),
        )
        .map_err(|_| "Status probe source unavailable")?;
        let manifest_path = package.join("package.json");
        let mut manifest: Value = serde_json::from_slice(
            &std::fs::read(&manifest_path).map_err(|_| "Fixture manifest unavailable")?,
        )
        .map_err(|_| "Fixture manifest invalid")?;
        let commands = manifest["commands"]
            .as_array_mut()
            .ok_or("Fixture commands unavailable")?;
        if !commands.iter().any(|c| c["name"] == "probe-status-icons") {
            commands.push(json!({"name":"probe-status-icons","title":"Declared status icons","mode":"menu-bar",
                "arguments":[{"name":"icon","type":"text","required":false,"placeholder":"Icon source"}]}));
        }
        std::fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap())
            .map_err(|_| "Fixture manifest write failed")?;
        std::fs::write(package.join("assets/status.svg"), br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><path fill="#e02040" d="M0 0h4v4H0z"/></svg>"##)
            .map_err(|_| "Status probe asset unavailable")?;
        let host = Arc::new(TestHost {
            allow_interaction: true,
            ..Default::default()
        });
        let paths = CorePaths {
            data_dir: profile.path().into(),
            config_dir: profile.path().join("config"),
            resource_dir: bundle.parent().ok_or("Missing fixture resources")?.into(),
            temp_dir: profile.path().join("temp"),
            home_dir: profile.path().join("home"),
        };
        let ctx = CoreContext::new(paths, Some(host.clone()));
        let launch = WorkerLaunch::new(bun, bundle)?;
        let core = NativeCore::new_with_extension_worker(ctx, launch)?;
        let weak = Arc::downgrade(&core);
        *host.status_icon_resolver.lock().unwrap() = Some(Arc::new(move |extension, icon| {
            crate::asset_transport::resolve_status_icon(weak.clone(), extension, icon)
        }));
        core.start_with_options(StartupOptions::ISOLATED_PREVIEW)
            .await?;
        Ok(Self {
            profile: Some(profile),
            transport: Arc::new(CoreTransport::new(core, runtime)),
            desktop: DesktopRecords(host.clone()),
            host,
        })
    }
}

// Parent factory keeps this runtime alive across the scenario. Panic cleanup
// still shuts down the ONE actual core-owned worker; no process-name sweep.
impl Drop for Fixture {
    fn drop(&mut self) {
        let core = self.transport.core.clone();
        let profile = self.profile.take();
        self.transport.runtime.spawn(async move {
            core.shutdown().await;
            drop(profile);
        });
    }
}
