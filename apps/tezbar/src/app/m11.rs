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
            serde_json::from_str(include_str!("../../../../src-tauri/tauri.conf.json"))
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
                    self.quit(cx);
                }
            }
        }
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
            .and_then(|exe| install(artifact, exe));
        if let Err(error) = result {
            if let Some(client) = &self.updater {
                let _ = client.send(Command::InstallFailed { generation });
            }
            let _ = self
                .log
                .write(format!("Update install/relaunch failed: {error}\n").as_bytes());
            if let Some(executable) = &self.installed_executable {
                let _ = std::process::Command::new(executable)
                    .args(std::env::args_os().skip(1))
                    .spawn();
            }
        }
    }
}
fn install(
    artifact: tezbar_update::native::VerifiedArtifact,
    executable: &std::path::Path,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let bundle = executable
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or("Invalid bundle")?;
        let parent = bundle.parent().ok_or("Invalid bundle parent")?;
        let version = artifact.version().to_string();
        let staged = artifact.stage_macos(parent)?;
        production_launch::verify_staged(&staged.bundle, &version)?;
        let installed = staged.install_over(bundle)?;
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if std::process::Command::new(executable)
            .args(&args)
            .spawn()
            .is_err()
        {
            installed.rollback()?;
            return Err("Update relaunch failed; previous version restored".into());
        }
        // No unbounded sibling copies in /Applications. A successful spawn ends
        // the legacy updater's rollback window; GUI readiness is a separate gate.
        if let Err(error) = installed.confirm_relaunch() {
            log::warn!("Update launched, but old-bundle cleanup failed: {error}");
        }
        Ok(())
    }
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
