//! One actor, one retained main-thread receiver, and the existing durable quit coordinator.
use super::update_integration::{Actor, Command, Effect, Frame, Mode};
use super::*;
use crate::surfaces::command_bar::navigation::UpdateInbox;
struct ActorTask(tokio::task::JoinHandle<Result<(), &'static str>>);
impl Drop for ActorTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl Owner {
    pub(super) fn mount_updater(
        &mut self,
        plan: Option<&production_launch::LaunchPlan>,
        cx: &mut Context<Self>,
    ) {
        if self
            .launcher
            .update(cx, |view, _, _| view.migration_pending())
            .unwrap_or(true)
        {
            return;
        }
        let Some(plan) = plan else {
            return;
        };
        let Ok(state) = self.launcher.update(cx, |view, _, _| view.state.clone()) else {
            return;
        };
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../../packaging/legacy-app-policy.json"))
                .expect("locked config");
        let targets = if cfg!(target_os = "macos") {
            vec!["darwin-aarch64-app", "darwin-aarch64"]
        } else {
            vec!["windows-x86_64-nsis", "windows-x86_64"]
        };
        let (actor, client, mut frames, mut effects) = Actor::new(
            tezbar_update::native::Config {
                current: env!("CARGO_PKG_VERSION").parse().expect("build version"),
                identifier: production_launch::IDENTITY.into(),
                data_dir: plan.profile.data.clone(),
                cache_dir: plan.profile.cache.join("updater-temp"),
                public_key: config["plugins"]["updater"]["pubkey"]
                    .as_str()
                    .expect("locked public key")
                    .into(),
                targets: targets.into_iter().map(str::to_owned).collect(),
            },
            Mode::Production,
            &state,
        );
        self.updater = Some(client);
        // Actor and coalesced receiver share RuntimeOwner; no runtime or HTTP client in App.
        let mut task = ActorTask(gpui_tokio::Tokio::handle(cx).spawn(actor.run(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64
        })));
        self.tasks.push(cx.spawn(async move |this, cx| {
            loop {
                tokio::select! {
                    r = frames.changed() => {
                        if r.is_err() { break; }
                        let frame = frames.borrow_and_update().clone();
                        if this.update(cx, |owner, cx| owner.update_frame(frame, cx)).is_err() { break; }
                    },
                    effect = effects.recv() => {
                        let Some(effect) = effect else { break; };
                        if this.update(cx, |owner, cx| owner.update_effect(effect, cx)).is_err() { break; }
                    },
                    _ = &mut task.0 => break,
                }
            }
            task.0.abort();
        }));
    }
    pub(super) fn mount_updater_after_recovery(&mut self, cx: &mut Context<Self>) {
        let plan = self.production_plan.clone();
        self.mount_updater(plan.as_ref(), cx);
    }
    pub(super) fn drain_updates(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<UpdateInbox>() {
            return;
        }
        let error = if let Some(client) = &self.updater {
            client.drain_inbox(cx.global_mut::<UpdateInbox>()).err()
        } else {
            while cx.global_mut::<UpdateInbox>().take().is_some() {}
            Some("Updates are disabled in this isolated preview")
        };
        if let Some(error) = error {
            let _ = self.launcher.update(cx, |view, _, cx| {
                view.error = Some(error.into());
                cx.notify();
            });
        }
    }
    pub(super) fn update_frame(&mut self, frame: Frame, cx: &mut Context<Self>) {
        if let Some(channel) = frame.channel {
            self.m7.channel = channel.name().into();
            self.m7.channel_loaded = true;
        }
        // Same event path updates Launcher rows and its task indicator, with no timestamp writes.
        self.events.publish(
            CoreEvent::decode(
                "update-status",
                serde_json::to_value(frame.status).expect("status serialization"),
            ),
            cx,
        );
        self.m7_refresh_content(cx);
        self.drain_updates(cx);
    }
    pub(super) fn update_effect(&mut self, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::Quiesced => {
                self.updater_quiescing = false;
                self.updater_paused = true;
                self.quit(cx);
            }
            Effect::ChannelFinished { nonce, success } => self.finish_channel(nonce, success, cx),
            Effect::CheckedAt(time) => {
                let _ = self
                    .launcher
                    .update(cx, |view, _, cx| view.updater_checked(Some(time), cx));
            }
            Effect::ClearLastCheck => {
                let _ = self
                    .launcher
                    .update(cx, |view, _, cx| view.updater_checked(None, cx));
            }
            Effect::AutoUpdate(_) => {
                self.m7_refresh_content(cx);
            }
            Effect::OpenInstaller { url } => cx.open_url(&url),
            Effect::Rejected { reason, .. } => {
                let _ = self.launcher.update(cx, |view, _, cx| {
                    view.error = Some(reason.into());
                    cx.notify();
                });
            }
            Effect::Install {
                generation,
                artifact,
            } => {
                self.pending_update = Some((generation, artifact));
                if self.lifecycle.quitting {
                    self.defer_update(cx);
                } else {
                    #[cfg(target_os = "macos")]
                    if let Err(error) = self
                        .installed_executable
                        .as_ref()
                        .and_then(|exe| exe.parent()?.parent()?.parent())
                        .ok_or_else(|| "Installed bundle is unavailable".to_string())
                        .and_then(tezbar_update::native::preflight_macos_install)
                    {
                        self.defer_update(cx);
                        let _ = self.launcher.update(cx, |view, _, cx| {
                            view.error = Some(error);
                            cx.notify();
                        });
                        return;
                    }
                    #[cfg(target_os = "macos")]
                    {
                        self.prepare_update(cx);
                        return;
                    }
                    #[cfg(not(target_os = "macos"))]
                    self.quit(cx);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    fn prepare_update(&mut self, cx: &mut Context<Self>) {
        let Some((generation, mut artifact)) = self.pending_update.take() else {
            return;
        };
        let executable = self.installed_executable.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let prepared = gpui_tokio::Tokio::spawn(cx, async move {
                tokio::task::spawn_blocking(move || {
                    let staged = executable
                        .as_deref()
                        .ok_or_else(|| "Installed executable unavailable".to_string())
                        .and_then(|exe| prepare_install(&mut artifact, exe));
                    (artifact, staged)
                })
                .await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = this.update(cx, |owner, cx| {
                match prepared {
                    Ok((artifact, staged)) => {
                        owner.pending_update = Some((generation, artifact));
                        match staged {
                            Ok(staged) if !owner.lifecycle.quitting => {
                                owner.staged_update = Some(staged);
                                // No runs, windows or durable owners are stopped
                                // until staged payload admission has succeeded.
                                owner.quit(cx);
                            }
                            Ok(_) => owner.defer_update(cx),
                            Err(error) => {
                                owner.defer_update(cx);
                                owner.update_refused(error, cx);
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(client) = &owner.updater {
                            let _ = client.send(Command::InstallFailed { generation });
                        }
                        owner.update_refused(format!("Could not prepare update: {error}"), cx);
                    }
                }
            });
        }));
    }
    fn update_refused(&mut self, error: String, cx: &mut Context<Self>) {
        let _ = self.launcher.update(cx, |view, _, cx| {
            view.error = Some(format!("Update was not installed: {error}"));
            cx.notify();
        });
    }
    fn finish_channel(&mut self, nonce: u64, success: bool, cx: &mut Context<Self>) {
        if let Some((id, ticket)) = self.m7.updater_channel {
            if ticket == nonce {
                self.m7.updater_channel = None;
                if success {
                    self.m7.failed_channel = None;
                }
                self.m7_finish_effect(
                    id,
                    "update-channel",
                    ticket,
                    if success {
                        Ok(())
                    } else {
                        Err("Could not save update channel. Retry.".into())
                    },
                    cx,
                );
                self.m7_refresh_content(cx);
            }
        }
    }
    pub(super) fn defer_update(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        self.staged_update.take();
        if let (Some(client), Some((generation, artifact))) =
            (self.updater.clone(), self.pending_update.take())
        {
            self.tasks.push(cx.spawn(async move |_, cx| {
                let _ = gpui_tokio::Tokio::spawn(cx, async move {
                    client.return_install(generation, artifact).await
                })
                .await;
            }));
        }
    }
    pub(super) fn finish_update(&mut self) {
        let Some((generation, artifact)) = self.pending_update.take() else {
            return;
        };
        let result = self
            .installed_executable
            .as_ref()
            .ok_or("Installed executable unavailable".to_string())
            .and_then(|exe| {
                #[cfg(target_os = "macos")]
                {
                    let _ = artifact;
                    install_prepared(
                        self.staged_update.take().ok_or("Update was not prepared")?,
                        exe,
                    )
                }
                #[cfg(not(target_os = "macos"))]
                install(artifact, exe)
            });
        if let Err(error) = result {
            if let Some(client) = &self.updater {
                let _ = client.send(Command::InstallFailed { generation });
            }
            let _ = self
                .log
                .write(format!("Update install/relaunch failed: {error}\n").as_bytes());
            if let Some(executable) = &self.installed_executable {
                if let Err(relaunch) = std::process::Command::new(executable)
                    .args(std::env::args_os().skip(1))
                    .spawn()
                {
                    let _ = self.log.write(
                        format!("Previous version relaunch failed: {relaunch}\n").as_bytes(),
                    );
                }
            }
        }
    }
}
#[cfg(target_os = "macos")]
fn prepare_install(
    artifact: &mut tezbar_update::native::VerifiedArtifact,
    executable: &std::path::Path,
) -> Result<tezbar_update::native::StagedMac, String> {
    let bundle = executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or("Invalid bundle")?;
    let parent = bundle.parent().ok_or("Invalid bundle parent")?;
    let staged = artifact.prepare_macos(parent)?;
    production_launch::verify_staged(&staged.bundle, &artifact.version().to_string())?;
    Ok(staged)
}
#[cfg(target_os = "macos")]
fn install_prepared(
    staged: tezbar_update::native::StagedMac,
    executable: &std::path::Path,
) -> Result<(), String> {
    let bundle = executable
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or("Invalid bundle")?;
    let installed = staged.install_over(bundle)?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Err(error) = std::process::Command::new(executable).args(&args).spawn() {
        installed.rollback().map_err(|rollback| {
            format!("Update relaunch failed: {error}; rollback failed: {rollback}")
        })?;
        return Err(format!(
            "Update relaunch failed: {error}; previous version restored"
        ));
    }
    if let Err(error) = installed.confirm_relaunch() {
        log::warn!("Update launched, but old-bundle cleanup failed: {error}");
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn install(
    artifact: tezbar_update::native::VerifiedArtifact,
    executable: &std::path::Path,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = executable;
        artifact
            .stage_windows(&std::env::temp_dir())?
            .launch_os(&std::env::args_os().skip(1).collect::<Vec<_>>())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (artifact, executable);
        Err("Unsupported installation platform".into())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod prequit_update_tests {
    use super::*;
    use crate::surfaces::command_bar::chat_integration::tests::{mount, settle};
    use ::core::prelude::v1::test;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Write;

    fn artifact() -> tezbar_update::native::VerifiedArtifact {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(include_bytes!(
            "../../../../crates/tezbar-update/tests/fixtures/fixture.tar.gz"
        ))
        .unwrap();
        tezbar_update::native::VerifiedArtifact::verify(
            file,
            tezbar_update::native::Release {
                version: "0.3.0-beta.8".parse().unwrap(),
                notes: String::new(),
                url: "https://example.invalid/fixture.tar.gz".parse().unwrap(),
                signature: STANDARD.encode(include_bytes!(
                    "../../../../crates/tezbar-update/tests/fixtures/fixture.tar.gz.minisig"
                )),
            },
            &STANDARD.encode(include_bytes!(
                "../../../../crates/tezbar-update/tests/fixtures/fixture.pub"
            )),
        )
        .unwrap()
    }

    #[gpui::test]
    fn refused_staged_update_never_enters_real_owner_quit(cx: &mut TestAppContext) {
        let (rt, root, backend, view, cx) = mount(cx);
        let owner = super::super::m8::tests::owner(cx, &rt, &root, &backend);
        let bundle = root.path().join("Tezbar.app");
        let executable = bundle.join("Contents/MacOS/tezbar");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"unchanged installed fixture").unwrap();
        let (actor, client, _, _) = Actor::new(
            tezbar_update::native::Config {
                current: "0.3.0-beta.7".parse().unwrap(),
                identifier: "com.tezbar.app".into(),
                data_dir: root.path().join("update-data"),
                cache_dir: root.path().join("cache"),
                public_key: String::new(),
                targets: vec![],
            },
            Mode::Production,
            &tezbar_ui_model::ui_state::UiState::default(),
        );
        owner.update(&mut cx.cx, |owner, cx| {
            owner.installed_executable = Some(executable.clone());
            owner.updater = Some(client);
            owner.update_effect(
                Effect::Install {
                    generation: 0,
                    artifact: artifact(),
                },
                cx,
            );
            assert!(!owner.lifecycle.quitting);
        });
        rt.block_on(async {
            let until = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                cx.run_until_parked();
                if view.read_with(cx, |view, _| {
                    view.error
                        .as_deref()
                        .is_some_and(|e| e.starts_with("Update was not installed:"))
                }) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < until,
                    "staged refusal was not surfaced"
                );
                tokio::task::yield_now().await;
            }
        });
        settle(&rt, cx);
        owner.read_with(&cx.cx, |owner, _| {
            assert!(!owner.lifecycle.quitting);
            assert!(owner.staged_update.is_none());
            assert!(owner.pending_update.is_none());
        });
        assert_eq!(
            std::fs::read(&executable).unwrap(),
            b"unchanged installed fixture"
        );
        assert!(
            !backend
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(channel, _)| *channel == "agent:cancel")
        );
        assert!(
            !std::fs::read_dir(root.path())
                .unwrap()
                .flatten()
                .any(|p| p.file_name().to_string_lossy().starts_with("tezbar-stage-"))
        );
        drop(actor);
        rt.block_on(backend.core.0.shutdown());
    }

    #[test]
    fn staged_refusal_retains_download_for_repeated_preparation() {
        let root = tempfile::tempdir().unwrap();
        let mut artifact = artifact();
        for _ in 0..2 {
            let staged = artifact.prepare_macos(root.path()).unwrap();
            assert!(staged.bundle.join("Contents/MacOS/tezbar").is_file());
            assert!(production_launch::verify_staged(&staged.bundle, "0.3.0-beta.8").is_err());
            drop(staged);
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }
}
