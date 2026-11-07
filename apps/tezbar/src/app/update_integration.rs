//! M11 draft, intentionally unexported. One actor on the EXISTING app runtime.
//! No render effects, installer execution, second policy/parser/certificate verifier or runtime.
use crate::surfaces::command_bar::navigation::{UpdateAction, UpdateInbox};
use std::{
    future::{Future, pending},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tezbar_ui_model::{ui_state::UiState, update_policy};
use tezbar_update::{
    AppUpdateStatus as Status,
    native::{Config, Updater, VerifiedArtifact},
    release_channels::ReleaseChannel,
};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Preview,
    Production,
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub generation: u64,
    pub status: Status,
    pub channel: Option<ReleaseChannel>,
}
pub enum Command {
    Action {
        nonce: u64,
        action: UpdateAction,
    },
    Channel {
        nonce: u64,
        channel: ReleaseChannel,
    },
    AutoUpdate(bool),
    Cancel,
    Shutdown,
    Quiesce,
    Resume,
    /// Internal App-only return when quit/restart confirmation is declined. Never IPC.
    InstallDeferred {
        generation: u64,
        artifact: VerifiedArtifact,
    },
    InstallFailed {
        generation: u64,
    },
}
/// Effects are consumed by App, never by render. UI-state mutations go to Launcher owner.
/// Install is a verified artifact handoff to the existing shutdown/relaunch coordinator;
/// it MUST call portable stage/install APIs, preserve rollback and use the current argv.
pub enum Effect {
    Quiesced,
    CheckedAt(f64),
    AutoUpdate(bool),
    ClearLastCheck,
    ChannelFinished {
        nonce: u64,
        success: bool,
    },
    OpenInstaller {
        url: String,
    },
    Install {
        generation: u64,
        artifact: VerifiedArtifact,
    },
    Rejected {
        nonce: u64,
        reason: &'static str,
    },
}
#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Command>,
}
impl Client {
    pub fn send(&self, command: Command) -> Result<(), &'static str> {
        self.tx
            .try_send(command)
            .map_err(|_| "Update queue is busy; retry shortly")
    }
    pub async fn control(&self, command: Command) -> Result<(), &'static str> {
        self.tx.send(command).await.map_err(|_| "Updater closed")
    }
    pub async fn return_install(
        &self,
        generation: u64,
        artifact: VerifiedArtifact,
    ) -> Result<(), &'static str> {
        self.tx
            .send(Command::InstallDeferred {
                generation,
                artifact,
            })
            .await
            .map_err(|_| "Updater closed")
    }
    /// Drain only as much as the actor can admit. Taking first then failing must not lose an intent.
    pub fn drain_inbox(&self, inbox: &mut UpdateInbox) -> Result<(), &'static str> {
        while !inbox.is_empty() {
            let Ok(permit) = self.tx.try_reserve() else {
                return Err("Update queue is busy; remaining intents retained");
            };
            let Some(intent) = inbox.take() else {
                return Ok(());
            };
            permit.send(Command::Action {
                nonce: intent.nonce,
                action: intent.action,
            });
        }
        Ok(())
    }
}
enum Done {
    Check(Result<Status, String>),
    Download(Result<VerifiedArtifact, String>),
    Channel(Result<(), String>),
}
type Job = Pin<Box<dyn Future<Output = Done> + Send>>;
pub struct Actor {
    updater: Arc<Updater>,
    mode: Mode,
    rx: mpsc::Receiver<Command>,
    effects: mpsc::Sender<Effect>,
    frames: watch::Sender<Frame>,
    progress: watch::Receiver<(u64, Status)>,
    generation: Arc<AtomicU64>,
    frame: Frame,
    auto: bool,
    last: Option<f64>,
    job: Option<Job>,
    ready: Option<VerifiedArtifact>,
    last_nonce: u64,
    stopped: bool,
    paused: bool,
    installing: bool,
    channel_nonce: Option<u64>,
}
impl Actor {
    /// Inert constructor. Parent validates production identity before constructing config.
    /// Returns a future owner, not a Task: parent holds exactly one Task on RuntimeOwner.
    pub fn new(
        config: Config,
        mode: Mode,
        state: &UiState,
    ) -> (Self, Client, watch::Receiver<Frame>, mpsc::Receiver<Effect>) {
        let generation = Arc::new(AtomicU64::new(0));
        let g = generation.clone();
        let (progress_tx, progress) = watch::channel((0, Status::Idle));
        let updater = Arc::new(Updater::new(
            config,
            Arc::new(move |status| {
                progress_tx.send_replace((g.load(Ordering::Acquire), status));
            }),
        ));
        let frame = Frame {
            generation: 0,
            status: Status::Idle,
            channel: None,
        };
        let (frames, frame_rx) = watch::channel(frame.clone());
        let (tx, rx) = mpsc::channel(16);
        let (effects, effect_rx) = mpsc::channel(16);
        (
            Self {
                updater,
                mode,
                rx,
                effects,
                frames,
                progress,
                generation,
                frame,
                auto: state.auto_update_enabled,
                last: state.last_update_check,
                job: None,
                ready: None,
                last_nonce: 0,
                stopped: false,
                paused: false,
                installing: false,
                channel_nonce: None,
            },
            Client { tx },
            frame_rx,
            effect_rx,
        )
    }
    fn publish(&mut self, status: Status) {
        self.frame.status = match status {
            Status::Error { .. } => Status::Error {
                message: "Couldn’t complete the update. Check again to retry.".into(),
            },
            other => other,
        };
        self.frame.generation = self.generation.load(Ordering::Acquire);
        self.frames.send_replace(self.frame.clone());
    }
    fn next(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
    fn check(&mut self) {
        if self.paused
            || self.job.is_some()
            || self.ready.is_some()
            || self.installing
            || self.mode == Mode::Preview
        {
            return;
        }
        self.next();
        let u = self.updater.clone();
        self.job = Some(Box::pin(async move { Done::Check(u.check().await) }));
    }
    fn download(&mut self) {
        self.next();
        let u = self.updater.clone();
        self.job = Some(Box::pin(async move { Done::Download(u.download().await) }));
    }
    async fn reject(&mut self, nonce: u64, reason: &'static str) {
        let _ = self.effects.send(Effect::Rejected { nonce, reason }).await;
    }
    async fn command(&mut self, c: Command) {
        match c {
            Command::Quiesce => {
                self.paused = true;
                if let Some(nonce) = self.channel_nonce.take() {
                    let _ = self
                        .effects
                        .send(Effect::ChannelFinished {
                            nonce,
                            success: false,
                        })
                        .await;
                }
                if self.job.take().is_some() {
                    self.next();
                    self.updater.reset(true);
                    self.publish(self.updater.status());
                }
                let _ = self.effects.send(Effect::Quiesced).await;
            }
            Command::Resume => {
                self.paused = false;
            }
            Command::Shutdown => {
                self.job.take();
                self.ready.take();
                self.next();
                self.stopped = true;
            }
            Command::InstallDeferred {
                generation,
                artifact,
            } => {
                if self.installing && generation == self.generation.load(Ordering::Acquire) {
                    self.installing = false;
                    self.ready = Some(artifact);
                }
            }
            Command::InstallFailed { generation } => {
                if self.installing && generation == self.generation.load(Ordering::Acquire) {
                    self.installing = false;
                    self.next();
                    self.updater.reset(true);
                    self.publish(self.updater.status());
                }
            }
            Command::Cancel if self.installing => {
                self.reject(0, "Restart is being prepared").await;
            }
            Command::Cancel => {
                if let Some(nonce) = self.channel_nonce.take() {
                    let _ = self
                        .effects
                        .send(Effect::ChannelFinished {
                            nonce,
                            success: false,
                        })
                        .await;
                }
                self.job.take();
                self.ready.take();
                self.next();
                self.updater.reset(true);
                self.publish(self.updater.status());
            }
            Command::AutoUpdate(v) => {
                self.auto = v;
                let _ = self.effects.send(Effect::AutoUpdate(v)).await;
            }
            Command::Action { nonce, action } => {
                if nonce <= self.last_nonce {
                    return;
                }
                self.last_nonce = nonce;
                if self.mode == Mode::Preview {
                    self.reject(nonce, "Updates are disabled in this isolated preview")
                        .await;
                    return;
                }
                if self.paused || self.job.is_some() || self.installing {
                    self.reject(nonce, "An update operation is already running")
                        .await;
                    return;
                }
                match action {
                    UpdateAction::Check if self.ready.is_none() => self.check(),
                    UpdateAction::Download
                        if matches!(self.frame.status, Status::Available { .. }) =>
                    {
                        self.download()
                    }
                    UpdateAction::Restart => {
                        if let Some(artifact) = self.ready.take() {
                            // Existing coordinator owns confirmation, 8s shutdown, swap + relaunch.
                            // No install API is called by this module or a view.
                            let generation = self.generation.load(Ordering::Acquire);
                            self.installing = true;
                            if let Err(error) = self
                                .effects
                                .send(Effect::Install {
                                    generation,
                                    artifact,
                                })
                                .await
                            {
                                if let Effect::Install { artifact, .. } = error.0 {
                                    self.ready = Some(artifact);
                                    self.installing = false;
                                }
                            }
                        } else {
                            self.reject(nonce, "Check and download the update first")
                                .await;
                        }
                    }
                    UpdateAction::Installer(url) => {
                        // Do not trust a URL supplied through IPC/navigation. It must equal the
                        // portable manualAvailable result, whose selection policy remains upstream.
                        if matches!(&self.frame.status,Status::ManualAvailable {installer_url,..} if installer_url==&url)
                        {
                            let _ = self.effects.send(Effect::OpenInstaller { url }).await;
                        } else {
                            self.reject(
                                nonce,
                                "Installer link no longer belongs to the selected update",
                            )
                            .await;
                        }
                    }
                    _ => {
                        self.reject(nonce, "Check for updates before downloading")
                            .await
                    }
                }
            }
            Command::Channel { nonce, channel } => {
                // Channel nonce is a separate Settings queue domain, not UpdateInbox nonce.
                if self.mode == Mode::Preview
                    || self.paused
                    || self.job.is_some()
                    || self.ready.is_some()
                    || self.installing
                {
                    let _ = self
                        .effects
                        .send(Effect::ChannelFinished {
                            nonce,
                            success: false,
                        })
                        .await;
                    self.reject(nonce, "Finish the current update before changing channels")
                        .await;
                    return;
                }
                self.next();
                self.channel_nonce = Some(nonce);
                let u = self.updater.clone();
                self.job = Some(Box::pin(async move {
                    Done::Channel(u.set_channel(channel).await)
                }));
            }
        }
    }
    async fn done(&mut self, done: Done, now: f64) {
        if matches!(done, Done::Channel(_)) {
            if let Some(nonce) = self.channel_nonce.take() {
                let _ = self
                    .effects
                    .send(Effect::ChannelFinished {
                        nonce,
                        success: matches!(done, Done::Channel(Ok(()))),
                    })
                    .await;
            }
        }
        match done {
            Done::Check(Ok(status)) => {
                if !matches!(status, Status::Error { .. }) {
                    self.last = Some(now);
                    let _ = self.effects.send(Effect::CheckedAt(now)).await;
                }
                let download = self.auto && matches!(status, Status::Available { .. });
                self.publish(status);
                if download {
                    self.download();
                }
            }
            Done::Check(Err(_)) | Done::Download(Err(_)) | Done::Channel(Err(_)) => {
                // Do not surface arbitrary network/credential-bearing diagnostic text.
                self.updater.reset(true);
                self.publish(self.updater.status());
            }
            Done::Download(Ok(artifact)) => {
                let version = artifact.version().to_string();
                self.ready = Some(artifact);
                self.publish(Status::Ready { version });
            }
            Done::Channel(Ok(())) => {
                self.last = None;
                self.frame.channel = self.updater.channel().ok();
                let _ = self.effects.send(Effect::ClearLastCheck).await;
                self.publish(Status::Idle);
                self.check();
            }
        }
    }
    /// wall_clock_ms is supplied by App; monotonic timers are separate from persisted epoch ms.
    pub async fn run(mut self, wall_clock_ms: impl Fn() -> f64) -> Result<(), &'static str> {
        // Preview does not read channel preferences or schedule network, even if auto=true.
        if self.mode == Mode::Production {
            self.frame.channel = self.updater.channel().ok();
        }
        self.frames.send_replace(self.frame.clone());
        let start = Instant::now();
        let mut host = Some(start + Duration::from_secs(3));
        let mut renderer = Some(start + Duration::from_secs(5));
        let mut periodic = start + Duration::from_millis(update_policy::INTERVAL as u64);
        while !self.stopped {
            let deadline = host
                .into_iter()
                .chain(renderer)
                .chain(Some(periodic))
                .min()
                .unwrap();
            tokio::select! {
                command=self.rx.recv()=>match command {Some(c)=>self.command(c).await,None=>break},
                done=async {match self.job.as_mut() {Some(job)=>job.await,None=>pending().await}}=>{
                    self.job=None;self.done(done,wall_clock_ms()).await;
                },
                changed=self.progress.changed()=>{
                    if changed.is_ok() {
                        let (generation,status)=self.progress.borrow_and_update().clone();
                        if generation==self.generation.load(Ordering::Acquire) && self.job.is_some() {self.publish(status);}
                    }
                },
                _=tokio::time::sleep_until(deadline)=>{
                    let now=Instant::now();let mut check=false;
                    if host.is_some_and(|d|d<=now) {host=None;check=true;}
                    if renderer.is_some_and(|d|d<=now) {renderer=None;check|=update_policy::auto_check(self.last,wall_clock_ms());}
                    if periodic<=now {periodic=now+Duration::from_millis(update_policy::INTERVAL as u64);check=true;}
                    if check {self.check();}
                },
            }
        }
        // Dropping the one job cancels its await; upstream RAII removes temporary downloads.
        self.job.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor(
        mode: Mode,
    ) -> (
        Actor,
        Client,
        watch::Receiver<Frame>,
        mpsc::Receiver<Effect>,
    ) {
        Actor::new(
            Config {
                current: "0.0.1-alpha.1".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: "/synthetic/m11/do-not-open".into(),
                cache_dir: "/synthetic/m11/do-not-open".into(),
                public_key: String::new(),
                targets: vec!["darwin-aarch64-app".into(), "darwin-aarch64".into()],
            },
            mode,
            &UiState::default(),
        )
    }
    #[tokio::test]
    async fn preview_rejects_network_install_channel_and_retains_real_status() {
        let (mut a, _, frames, mut effects) = actor(Mode::Preview);
        for (nonce, action) in [
            (1, UpdateAction::Check),
            (2, UpdateAction::Download),
            (3, UpdateAction::Restart),
            (4, UpdateAction::Installer("https://example.com".into())),
        ] {
            a.command(Command::Action { nonce, action }).await;
            assert!(matches!(effects.try_recv(), Ok(Effect::Rejected { .. })));
        }
        a.command(Command::Channel {
            nonce: 1,
            channel: ReleaseChannel::Beta,
        })
        .await;
        assert!(matches!(
            effects.try_recv(),
            Ok(Effect::ChannelFinished {
                nonce: 1,
                success: false
            })
        ));
        assert!(matches!(effects.try_recv(), Ok(Effect::Rejected { .. })));
        assert!(a.job.is_none());
        assert_eq!(frames.borrow().status, Status::Idle);
        assert_eq!(a.updater.status(), Status::Idle);
    }
    #[tokio::test]
    async fn one_unpolled_operation_repeated_nonces_backpressure_and_cancellation_generation() {
        let (mut a, client, _, mut effects) = actor(Mode::Production);
        // Construction/check planning only. The job is never polled: no real network.
        a.command(Command::Action {
            nonce: 1,
            action: UpdateAction::Check,
        })
        .await;
        assert!(a.job.is_some());
        let generation = a.generation.load(Ordering::Acquire);
        a.command(Command::Action {
            nonce: 1,
            action: UpdateAction::Check,
        })
        .await;
        assert!(effects.try_recv().is_err());
        a.command(Command::Action {
            nonce: 2,
            action: UpdateAction::Check,
        })
        .await;
        assert!(matches!(effects.try_recv(), Ok(Effect::Rejected { .. })));
        a.command(Command::Cancel).await;
        assert!(a.job.is_none());
        assert!(a.generation.load(Ordering::Acquire) > generation);
        for _ in 0..16 {
            client.send(Command::Cancel).unwrap();
        }
        assert!(client.send(Command::Cancel).is_err());
    }
    #[tokio::test]
    async fn installer_url_requires_exact_portable_manual_status_and_preferences_use_ui_owner() {
        let (mut a, _, _, mut effects) = actor(Mode::Production);
        a.frame.status = Status::ManualAvailable {
            version: "1".into(),
            notes: String::new(),
            release_url: "https://github.com/fixture".into(),
            installer_url: "https://github.com/fixture/setup.exe".into(),
        };
        a.command(Command::Action {
            nonce: 1,
            action: UpdateAction::Installer("https://example.com/evil".into()),
        })
        .await;
        assert!(matches!(effects.try_recv(), Ok(Effect::Rejected { .. })));
        a.command(Command::Action {
            nonce: 2,
            action: UpdateAction::Installer("https://github.com/fixture/setup.exe".into()),
        })
        .await;
        assert!(matches!(
            effects.try_recv(),
            Ok(Effect::OpenInstaller { .. })
        ));
        a.command(Command::AutoUpdate(true)).await;
        assert!(a.auto);
        assert!(matches!(effects.try_recv(), Ok(Effect::AutoUpdate(true))));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Write;
    fn artifact() -> VerifiedArtifact {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(include_bytes!(
            "../../../../crates/tezbar-update/tests/fixtures/fixture.tar.gz"
        ))
        .unwrap();
        VerifiedArtifact::verify(file,tezbar_update::native::Release {
            version:"0.3.0-beta.8".parse().unwrap(),notes:String::new(),
            url:"https://github.com/almatkai/Tezbar/releases/download/v0.3.0-beta.8/Tezbar.app.tar.gz".parse().unwrap(),
            signature:STANDARD.encode(include_bytes!("../../../../crates/tezbar-update/tests/fixtures/fixture.tar.gz.minisig")),
        },&STANDARD.encode(include_bytes!("../../../../crates/tezbar-update/tests/fixtures/fixture.pub"))).unwrap()
    }
    fn setup(
        root: &std::path::Path,
    ) -> (
        Actor,
        Client,
        watch::Receiver<Frame>,
        mpsc::Receiver<Effect>,
    ) {
        Actor::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: root.into(),
                cache_dir: root.into(),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Production,
            &UiState::default(),
        )
    }
    #[tokio::test]
    async fn verified_artifact_decline_cancel_failure_and_stale_generation_are_finite() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _, frames, mut effects) = setup(d.path());
        a.next();
        a.done(Done::Download(Ok(artifact())), 1.0).await;
        a.command(Command::Action {
            nonce: 1,
            action: UpdateAction::Restart,
        })
        .await;
        let Effect::Install {
            generation,
            artifact,
        } = effects.recv().await.unwrap()
        else {
            panic!()
        };
        a.command(Command::InstallDeferred {
            generation,
            artifact,
        })
        .await;
        assert!(a.ready.is_some());
        assert!(!a.installing);
        a.command(Command::Cancel).await;
        assert!(a.ready.is_none());
        assert!(matches!(frames.borrow().status, Status::Error { .. }));
        a.command(Command::InstallDeferred {
            generation,
            artifact: super::integration_tests::artifact(),
        })
        .await;
        assert!(a.ready.is_none());
        a.next();
        a.done(
            Done::Download(Ok(super::integration_tests::artifact())),
            2.0,
        )
        .await;
        a.command(Command::Action {
            nonce: 2,
            action: UpdateAction::Restart,
        })
        .await;
        let Effect::Install { generation, .. } = effects.recv().await.unwrap() else {
            panic!()
        };
        a.command(Command::InstallFailed {
            generation: generation - 1,
        })
        .await;
        assert!(a.installing);
        a.command(Command::InstallFailed { generation }).await;
        assert!(!a.installing);
        assert!(matches!(a.updater.status(), Status::Error { .. }));
        a.command(Command::Action {
            nonce: 3,
            action: UpdateAction::Check,
        })
        .await;
        assert!(a.job.is_some()); // unpolled: no network
    }
    #[tokio::test]
    async fn checked_timestamp_is_one_effect_per_success_error_never_records_and_latest_watch_survives()
     {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _, frames, mut effects) = setup(d.path());
        a.done(
            Done::Check(Ok(Status::UpToDate {
                version: "0.3.0-beta.7".into(),
            })),
            42.0,
        )
        .await;
        assert!(matches!(effects.try_recv(), Ok(Effect::CheckedAt(42.0))));
        assert!(effects.try_recv().is_err());
        a.done(
            Done::Check(Ok(Status::Error {
                message: "fixture".into(),
            })),
            43.0,
        )
        .await;
        assert!(effects.try_recv().is_err());
        assert_eq!(a.last, Some(42.0));
        let reopened = frames.clone();
        assert!(matches!(reopened.borrow().status, Status::Error { .. }));
    }
}
#[cfg(test)]
mod channel_durability_tests {
    use super::*;
    #[tokio::test]
    async fn channel_cancel_returns_the_exact_durability_ticket_and_clears_busy_state() {
        let d = tempfile::tempdir().unwrap();
        let (mut actor, _, _, mut effects) = Actor::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: d.path().into(),
                cache_dir: d.path().into(),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Production,
            &UiState::default(),
        );
        actor
            .command(Command::Channel {
                nonce: 31,
                channel: ReleaseChannel::Alpha,
            })
            .await;
        assert_eq!(actor.channel_nonce, Some(31));
        actor.command(Command::Cancel).await;
        assert!(matches!(
            effects.try_recv(),
            Ok(Effect::ChannelFinished {
                nonce: 31,
                success: false
            })
        ));
        assert!(actor.channel_nonce.is_none());
        assert!(actor.job.is_none());
    }
}
#[cfg(test)]
mod admission_regressions {
    use super::*;
    fn setup(
        root: &std::path::Path,
    ) -> (
        Actor,
        Client,
        watch::Receiver<Frame>,
        mpsc::Receiver<Effect>,
    ) {
        Actor::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: root.into(),
                cache_dir: root.into(),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Production,
            &UiState::default(),
        )
    }
    #[tokio::test]
    async fn action_rejection_cannot_complete_the_separate_channel_nonce_domain() {
        let d = tempfile::tempdir().unwrap();
        let (mut actor, _, _, mut effects) = setup(d.path());
        actor
            .command(Command::Channel {
                nonce: 1,
                channel: ReleaseChannel::Alpha,
            })
            .await;
        actor
            .command(Command::Action {
                nonce: 1,
                action: UpdateAction::Check,
            })
            .await;
        assert!(matches!(
            effects.try_recv(),
            Ok(Effect::Rejected { nonce: 1, .. })
        ));
        assert!(effects.try_recv().is_err());
        assert_eq!(actor.channel_nonce, Some(1));
        actor.command(Command::Cancel).await;
        assert!(matches!(
            effects.try_recv(),
            Ok(Effect::ChannelFinished {
                nonce: 1,
                success: false
            })
        ));
    }
    #[tokio::test]
    async fn exact_queue_capacity_admits_every_intent_without_false_busy_or_loss() {
        let d = tempfile::tempdir().unwrap();
        let (_actor, client, _, _effects) = setup(d.path());
        let mut inbox = UpdateInbox::default();
        for i in 0..16 {
            inbox
                .put(UpdateAction::Installer(format!(
                    "https://example.invalid/{i}"
                )))
                .unwrap();
        }
        assert!(client.drain_inbox(&mut inbox).is_ok());
        assert!(inbox.is_empty());
        inbox.put(UpdateAction::Check).unwrap();
        assert!(client.drain_inbox(&mut inbox).is_err());
        assert!(!inbox.is_empty());
    }
}
#[cfg(test)]
mod quit_fence_tests {
    use super::*;
    #[tokio::test]
    async fn quiescence_fences_completion_effects_before_save_drain_and_resume_reopens_admission() {
        let d = tempfile::tempdir().unwrap();
        let (mut a, _, _, mut effects) = Actor::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: d.path().into(),
                cache_dir: d.path().into(),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Production,
            &UiState::default(),
        );
        a.done(
            Done::Check(Ok(Status::UpToDate {
                version: "fixture".into(),
            })),
            1.0,
        )
        .await;
        a.command(Command::Quiesce).await;
        assert!(matches!(effects.recv().await, Some(Effect::CheckedAt(1.0))));
        assert!(matches!(effects.recv().await, Some(Effect::Quiesced)));
        a.check();
        assert!(a.job.is_none());
        a.command(Command::Action {
            nonce: 1,
            action: UpdateAction::Check,
        })
        .await;
        assert!(matches!(
            effects.recv().await,
            Some(Effect::Rejected { nonce: 1, .. })
        ));
        a.command(Command::Resume).await;
        a.check();
        assert!(a.job.is_some()); // never polled
        a.command(Command::Quiesce).await;
        assert!(a.job.is_none());
        assert!(matches!(effects.recv().await, Some(Effect::Quiesced)));
    }
}
#[cfg(test)]
mod loop_tests {
    use super::*;
    #[tokio::test]
    async fn real_preview_loop_owns_commands_effects_watch_and_shutdown_without_profile_or_http() {
        let d = tempfile::tempdir().unwrap();
        let missing = d.path().join("must-not-be-created");
        let (actor, client, frames, mut effects) = Actor::new(
            Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.preview.native".into(),
                data_dir: missing.clone(),
                cache_dir: missing.clone(),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Preview,
            &UiState::default(),
        );
        let task = tokio::spawn(actor.run(|| 42.0));
        client
            .control(Command::Action {
                nonce: 1,
                action: UpdateAction::Check,
            })
            .await
            .unwrap();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(1), effects.recv())
                .await
                .unwrap(),
            Some(Effect::Rejected { nonce: 1, .. })
        ));
        assert!(frames.borrow().channel.is_none());
        assert!(!missing.exists());
        client.control(Command::Quiesce).await.unwrap();
        assert!(matches!(effects.recv().await, Some(Effect::Quiesced)));
        client.control(Command::Shutdown).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!missing.exists());
    }
}
