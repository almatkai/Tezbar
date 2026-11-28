//! Text-to-speech for Read Aloud. Voice input (dictation, speech-to-text models and their
//! downloads) was removed from the native app; only speech output remains.
use super::*;
#[derive(Clone)]
pub struct Voice {
    inner: Arc<VoiceInner>,
}
struct VoiceInner {
    ctx: CoreContext,
    state: Mutex<VoiceState>,
    stop: Cancel,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    login_path: tokio::sync::OnceCell<String>,
}
#[derive(Default)]
struct VoiceState {
    speech: Option<Cancel>,
}
impl Voice {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            inner: Arc::new(VoiceInner {
                ctx,
                state: Mutex::new(VoiceState::default()),
                stop: Cancel::new(),
                tasks: Mutex::new(Vec::new()),
                login_path: tokio::sync::OnceCell::new(),
            }),
        }
    }
    fn spawn(&self, f: impl std::future::Future<Output = ()> + Send + 'static) {
        let task = tokio::spawn(f);
        let mut tasks = self.inner.tasks.lock().unwrap();
        tasks.retain(|t| !t.is_finished());
        tasks.push(task);
    }
    pub async fn shutdown(&self) {
        self.inner.stop.cancel();
        self.stop_speech();
        let tasks = std::mem::take(&mut *self.inner.tasks.lock().unwrap());
        for mut task in tasks {
            if tokio::time::timeout(Duration::from_secs(5), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
    async fn path(&self) -> &str {
        self.inner
            .login_path
            .get_or_init(|| async {
                process::login_path(&self.inner.ctx.paths.home_dir, &self.inner.stop).await
            })
            .await
    }
    async fn run(
        &self,
        bin: &str,
        args: &[&str],
        timeout: u64,
        cancel: &Cancel,
    ) -> Result<String, String> {
        let path = tokio::select! {
            _ = cancel.cancelled() => return Err("Operation cancelled".into()),
            path = self.path() => path,
        };
        process::output(
            process::command(bin).args(args).env("PATH", path),
            Duration::from_secs(timeout),
            cancel,
        )
        .await
    }
    async fn has(&self, bin: &str) -> bool {
        if cfg!(windows) {
            self.run("where.exe", &[bin], 5, &self.inner.stop)
                .await
                .is_ok()
        } else {
            self.run(
                "sh",
                &["-c", "command -v \"$1\"", "tezbar-probe", bin],
                5,
                &self.inner.stop,
            )
            .await
            .is_ok()
        }
    }
    fn stop_speech(&self) {
        if let Some(cancel) = self.inner.state.lock().unwrap().speech.take() {
            cancel.cancel();
        }
    }
    async fn speak(&self, text: &str) -> CoreResult {
        self.stop_speech();
        if text.trim().is_empty() {
            return Ok(json!({"ok":true}));
        }
        let cancel = Cancel::new();
        self.inner.state.lock().unwrap().speech = Some(cancel.clone());
        // Windows speaks through SAPI in-process; cancellation purges the queue.
        #[cfg(windows)]
        {
            let text = text.trim().to_owned();
            let (voice_cancel, stop) = (cancel.clone(), self.inner.stop.clone());
            self.spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    crate::windows_shell::speak(&text, || voice_cancel.is_cancelled() || stop.is_cancelled())
                })
                .await;
            });
            return Ok(json!({"ok":true}));
        }
        #[allow(unreachable_code)]
        let binary = "say";
        let available = self.has(binary).await;
        if cancel.is_cancelled() {
            return Ok(json!({"ok":true}));
        }
        if !available {
            return Err(format!("Text-to-speech runtime is unavailable: {binary}"));
        }
        let mut command = process::command("say");
        command.arg(text.trim());
        let voice = self.clone();
        self.spawn(async move{let _=tokio::select!{r=process::output(&mut command,Duration::from_secs(3600),&cancel)=>r,_=voice.inner.stop.cancelled()=>{cancel.cancel();Err("Voice shut down".into())}};});
        Ok(json!({"ok":true}))
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> CoreResult {
        let a = arg(args, 0);
        match channel {
            "voice:tts:speak" => {
                self.speak(a["text"].as_str().ok_or("text must be a string")?)
                    .await
            }
            "voice:tts:stop" => {
                self.stop_speech();
                Ok(json!({"ok":true}))
            }
            _ => Err(format!("Unknown voice channel: {channel}")),
        }
    }
}

#[cfg(test)]
mod subprocess_tests {
    use super::*;
    #[tokio::test]
    #[cfg(unix)]
    async fn stopping_speech_during_path_probe_prevents_late_playback() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let voice = Voice::new(CoreContext::headless(common::CorePaths {
            data_dir: root.path().join("data"),
            config_dir: root.path().join("config"),
            resource_dir: root.path().join("resources"),
            temp_dir: root.path().join("tmp"),
            home_dir: root.path().join("home"),
        }));
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let marker = root.path().join("played");
        let say = bin.join("say");
        std::fs::write(&say, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&say, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let owner = voice.clone();
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let initializing = tokio::spawn(async move {
            owner
                .inner
                .login_path
                .get_or_init(|| async move {
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    path
                })
                .await;
        });
        entered_rx.await.unwrap();
        let owner = voice.clone();
        let speech = tokio::spawn(async move { owner.speak("must remain stopped").await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while voice.inner.state.lock().unwrap().speech.is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        voice.stop_speech();
        release_tx.send(()).unwrap();
        initializing.await.unwrap();
        assert_eq!(speech.await.unwrap().unwrap()["ok"], true);
        voice.shutdown().await;
        assert!(!marker.exists());
    }
    #[tokio::test]
    async fn voice_input_channels_are_gone() {
        let root = tempfile::tempdir().unwrap();
        let voice = Voice::new(CoreContext::headless(common::CorePaths {
            data_dir: root.path().join("data"),
            config_dir: root.path().join("config"),
            resource_dir: root.path().join("resources"),
            temp_dir: root.path().join("tmp"),
            home_dir: root.path().join("home"),
        }));
        for channel in ["voice:stt:transcribe", "voice:stt:modes", "voice:models:list", "voice:model:download"] {
            assert!(voice.handle(channel, &[]).await.is_err(), "{channel}");
        }
        assert_eq!(voice.handle("voice:tts:stop", &[]).await.unwrap()["ok"], true);
        voice.shutdown().await;
    }
}
