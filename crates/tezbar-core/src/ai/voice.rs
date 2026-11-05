use super::*;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
#[derive(Clone)]
pub struct Voice {
    inner: Arc<VoiceInner>,
}
struct VoiceInner {
    ctx: CoreContext,
    http: reqwest::Client,
    state: Mutex<VoiceState>,
    stop: Cancel,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    login_path: tokio::sync::OnceCell<String>,
}
#[derive(Default)]
struct VoiceState {
    downloads: HashMap<String, Value>,
    runtimes: HashMap<String, (u64, Value)>,
    speech: Option<Cancel>,
}
fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("voice_catalog.json")).expect("static voice catalog")
}
fn model(id: &str) -> Result<Value, String> {
    catalog()
        .into_iter()
        .find(|m| m["id"] == id)
        .ok_or_else(|| format!("Unknown voice model: {id}"))
}
fn bytes(v: &Value) -> Result<Vec<u8>, String> {
    if let Some(a) = v.as_array() {
        if a.len() > 100 * 1024 * 1024 {
            return Err("Audio exceeds 100 MiB".into());
        }
        return a
            .iter()
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n <= 255)
                    .map(|n| n as u8)
                    .ok_or("Invalid audio byte".into())
            })
            .collect();
    }
    if let Some(object) = v.as_object() {
        if object.len() > 100 * 1024 * 1024 {
            return Err("Audio exceeds 100 MiB".into());
        }
        let mut result = Vec::with_capacity(object.len());
        for i in 0..object.len() {
            let n = object
                .get(&i.to_string())
                .and_then(Value::as_u64)
                .filter(|n| *n <= 255)
                .ok_or("Audio byte object must have contiguous numeric keys")?;
            result.push(n as u8);
        }
        return Ok(result);
    }
    Err("audioBytes must be an array or numeric-key object".into())
}
impl Voice {
    pub fn new(ctx: CoreContext) -> Self {
        Self {
            inner: Arc::new(VoiceInner {
                ctx,
                http: reqwest::Client::new(),
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
        process::output(
            process::command(bin)
                .args(args)
                .env("PATH", self.path().await),
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
    async fn runtime(&self, kind: &str, refresh: bool) -> Value {
        if !refresh {
            if let Some((at, probe)) = self.inner.state.lock().unwrap().runtimes.get(kind) {
                if now_ms().saturating_sub(*at) < 300000 {
                    return probe.clone();
                }
            }
        }
        let probe = if kind == "whisper-cpp" {
            json!({"label":"whisper.cpp","ready":self.has("whisper-cli").await||self.has("whisper-cpp").await,"installCommand":"brew install whisper-cpp"})
        } else {
            let python = self.has("python3").await;
            let ready = python
                && self
                    .run(
                        "python3",
                        &["-c", "import moonshine_voice"],
                        15,
                        &self.inner.stop,
                    )
                    .await
                    .is_ok();
            let mut probe = json!({"label":"Moonshine (Python)","ready":ready,"installCommand":"python3 -m pip install --user moonshine-voice onnxruntime"});
            if !python {
                probe["message"] = json!("python3 was not found on your PATH.");
            }
            probe
        };
        self.inner
            .state
            .lock()
            .unwrap()
            .runtimes
            .insert(kind.into(), (now_ms(), probe.clone()));
        probe
    }
    fn dir(&self, id: &str) -> std::path::PathBuf {
        self.inner.ctx.paths.data_dir.join("voice-models").join(id)
    }
    async fn disk(&self, m: &Value) -> (bool, u64) {
        let mut full = true;
        let mut size = 0;
        for a in m["assets"].as_array().unwrap() {
            match tokio::fs::metadata(self.dir(s(&m["id"])).join(s(&a["fileName"]))).await {
                Ok(meta) if meta.len() > 0 => size += meta.len(),
                _ => full = false,
            }
        }
        (full, size)
    }
    async fn selected(&self) -> Result<String, String> {
        let ctx = self.inner.ctx.clone();
        let raw = tokio::task::spawn_blocking(move || ctx.config.read())
            .await
            .map_err(err)??;
        let id = s(&raw["voiceSttModelId"]);
        Ok(if model(id).is_ok() {
            id
        } else {
            "moonshine-base-en"
        }
        .into())
    }
    async fn select(&self, id: &str) -> Result<String, String> {
        model(id)?;
        let ctx = self.inner.ctx.clone();
        let id = id.to_string();
        let result = id.clone();
        tokio::task::spawn_blocking(move || ctx.config.patch(&json!({"voiceSttModelId":id})))
            .await
            .map_err(err)??;
        Ok(result)
    }
    async fn view(&self, m: &Value, selected: &str) -> Value {
        let runtime = self.runtime(s(&m["runtime"]), false).await;
        let (full, size) = self.disk(m).await;
        let active = self
            .inner
            .state
            .lock()
            .unwrap()
            .downloads
            .get(s(&m["id"]))
            .cloned();
        let mut result = m.clone();
        result.as_object_mut().unwrap().remove("assets");
        result["runtime"] = runtime.clone();
        result["selected"] = json!(m["id"] == selected);
        let status = if full && runtime["ready"] == true {
            json!({"status":"downloaded","stage":"idle","progress":1,"downloadedBytes":size,"totalBytes":size})
        } else if let Some(active) = active {
            active
        } else {
            json!({"status":"not-downloaded","stage":"idle","progress":0,"downloadedBytes":size,"totalBytes":null})
        };
        for (k, v) in status.as_object().unwrap() {
            result[k] = v.clone();
        }
        result
    }
    async fn list(&self) -> CoreResult {
        self.cleanup_legacy().await?;
        let selected = self.selected().await?;
        let mut list = Vec::new();
        for m in catalog() {
            list.push(self.view(&m, &selected).await);
        }
        Ok(json!(list))
    }
    async fn cleanup_legacy(&self) -> Result<(), String> {
        for m in catalog().into_iter().filter(|m| m["family"] == "whisper") {
            for name in [
                "model.safetensors",
                "config.json",
                "generation_config.json",
                "merges.txt",
                "normalizer.json",
                "preprocessor_config.json",
                "special_tokens_map.json",
                "tokenizer.json",
                "tokenizer_config.json",
                "vocab.json",
            ] {
                let path = self.dir(s(&m["id"])).join(name);
                match tokio::fs::remove_file(path).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(err(e)),
                }
            }
        }
        Ok(())
    }
    fn update_download(&self, id: &str, state: Value) {
        self.inner
            .state
            .lock()
            .unwrap()
            .downloads
            .insert(id.into(), state.clone());
        self.inner
            .ctx
            .emit("voice:model:progress", json!({"modelId":id,"state":state}));
    }
    async fn download(&self, id: &str) -> CoreResult {
        let m = model(id)?;
        let launch = {
            let mut state = self.inner.state.lock().unwrap();
            if state
                .downloads
                .get(id)
                .is_some_and(|s| s["status"] == "downloading")
            {
                false
            } else {
                state.downloads.insert(id.into(),json!({"status":"downloading","stage":"installing-runtime","downloadedBytes":0,"totalBytes":null,"progress":null}));
                true
            }
        };
        if launch {
            let voice = self.clone();
            let m = m.clone();
            self.spawn(async move{let id=s(&m["id"]);if let Err(error)=voice.provision(&m).await{let (_,size)=voice.disk(&m).await;voice.update_download(id,json!({"status":"error","stage":"downloading-weights","downloadedBytes":size,"totalBytes":null,"progress":null,"errorMessage":error}));}});
        }
        Ok(self.view(&m, &self.selected().await?).await)
    }
    async fn provision(&self, m: &Value) -> Result<(), String> {
        let id = s(&m["id"]);
        let kind = s(&m["runtime"]);
        if self.runtime(kind, true).await["ready"] != true {
            if kind == "whisper-cpp" {
                if !self.has("brew").await {
                    return Err("Homebrew is required to install whisper.cpp automatically. Install whisper.cpp manually: brew install whisper-cpp".into());
                }
                self.run("brew", &["install", "whisper-cpp"], 600, &self.inner.stop)
                    .await?;
            } else {
                if !self.has("python3").await {
                    return Err("python3 was not found. Install Python first and try again.".into());
                }
                self.run(
                    "python3",
                    &[
                        "-m",
                        "pip",
                        "install",
                        "--user",
                        "--upgrade",
                        "moonshine-voice",
                        "onnxruntime",
                    ],
                    600,
                    &self.inner.stop,
                )
                .await?;
            }
            let probe = self.runtime(kind, true).await;
            if probe["ready"] != true {
                return Err(format!("Installed runtime but could not detect it afterwards ({}). Try running manually: {}",s(&probe["label"]),s(&probe["installCommand"])));
            }
        }
        tokio::fs::create_dir_all(self.dir(id)).await.map_err(err)?;
        let mut baseline = self.disk(m).await.1;
        for asset in m["assets"].as_array().unwrap() {
            let destination = self.dir(id).join(s(&asset["fileName"]));
            if tokio::fs::metadata(&destination)
                .await
                .is_ok_and(|meta| meta.len() > 0)
            {
                continue;
            }
            let partial = destination.with_extension(format!(
                "{}.part",
                destination
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
            let result=async{let response=tokio::select!{_=self.inner.stop.cancelled()=>return Err("Download cancelled".to_string()),r=self.inner.http.get(s(&asset["url"])).timeout(Duration::from_secs(1800)).send()=>r.map_err(err)?};if !response.status().is_success(){return Err(format!("Download failed ({})",response.status()))}let total=response.content_length();let mut file=tokio::fs::File::create(&partial).await.map_err(err)?;let mut stream=response.bytes_stream();let mut downloaded=0u64;loop{let next=tokio::select!{_=self.inner.stop.cancelled()=>return Err("Download cancelled".to_string()),next=stream.next()=>next};let Some(chunk)=next else{break};let chunk=chunk.map_err(err)?;file.write_all(&chunk).await.map_err(err)?;downloaded+=chunk.len()as u64;self.update_download(id,json!({"status":"downloading","stage":"downloading-weights","downloadedBytes":baseline+downloaded,"totalBytes":total,"progress":total.filter(|n|*n>0).map(|n|(downloaded as f64/n as f64).min(0.999))}));}if downloaded==0||total.is_some_and(|n|n!=downloaded){return Err("Model download was empty or incomplete".into())}file.sync_all().await.map_err(err)?;drop(file);tokio::fs::rename(&partial,&destination).await.map_err(err)?;Ok::<(),String>(())}.await;
            if result.is_err() {
                let _ = tokio::fs::remove_file(&partial).await;
            }
            result?;
            baseline = self.disk(m).await.1;
        }
        self.inner.state.lock().unwrap().downloads.remove(id);
        self.runtime(kind, true).await;
        Ok(())
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
        let binary = if cfg!(windows) {
            "powershell.exe"
        } else {
            "say"
        };
        let cancel = Cancel::new();
        self.inner.state.lock().unwrap().speech = Some(cancel.clone());
        let available = self.has(binary).await;
        if cancel.is_cancelled() {
            return Ok(json!({"ok":true}));
        }
        if !available {
            return Err(format!("Text-to-speech runtime is unavailable: {binary}"));
        }
        let mut command = if cfg!(windows) {
            let mut cmd = process::command("powershell.exe");
            cmd.args(["-NoLogo","-NoProfile","-NonInteractive","-Command","Add-Type -AssemblyName System.Speech; $voice=New-Object System.Speech.Synthesis.SpeechSynthesizer; try { $voice.Speak($env:TEZBAR_SPEECH_TEXT) } finally { $voice.Dispose() }"]).env("TEZBAR_SPEECH_TEXT",text.trim());
            cmd
        } else {
            let mut cmd = process::command("say");
            cmd.arg(text.trim());
            cmd
        };
        let voice = self.clone();
        self.spawn(async move{let _=tokio::select!{r=process::output(&mut command,Duration::from_secs(3600),&cancel)=>r,_=voice.inner.stop.cancelled()=>{cancel.cancel();Err("Voice shut down".into())}};});
        Ok(json!({"ok":true}))
    }
    pub async fn handle(&self, channel: &str, args: &[Value]) -> CoreResult {
        let a = arg(args, 0);
        match channel {
            "voice:models:list" => self.list().await,
            "voice:model:get-selected" => Ok(json!({"modelId":self.selected().await?})),
            "voice:model:set-selected" => Ok(
                json!({"modelId":self.select(a["modelId"].as_str().ok_or("modelId is required")?).await?}),
            ),
            "voice:model:download" => {
                self.download(a["modelId"].as_str().ok_or("modelId is required")?)
                    .await
            }
            "voice:model:delete" => {
                let id = a["modelId"].as_str().ok_or("modelId is required")?;
                model(id)?;
                if self
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .downloads
                    .get(id)
                    .is_some_and(|s| s["status"] == "downloading")
                {
                    return Err("Wait for this model download to finish before deleting it.".into());
                }
                match tokio::fs::remove_dir_all(self.dir(id)).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(err(e)),
                }
                self.inner.state.lock().unwrap().downloads.remove(id);
                let mut selected = self.selected().await?;
                if selected == id {
                    let mut next = None;
                    for m in catalog() {
                        if m["id"] != id && self.disk(&m).await.0 {
                            next = Some(s(&m["id"]).to_string());
                            break;
                        }
                    }
                    selected = self
                        .select(&next.unwrap_or_else(|| {
                            catalog()
                                .iter()
                                .find(|m| m["id"] != id)
                                .map(|m| s(&m["id"]).to_string())
                                .unwrap_or(id.into())
                        }))
                        .await?;
                }
                Ok(json!({"modelId":selected}))
            }
            "voice:tts:speak" => {
                self.speak(a["text"].as_str().ok_or("text must be a string")?)
                    .await
            }
            "voice:tts:stop" => {
                self.stop_speech();
                Ok(json!({"ok":true}))
            }
            "voice:stt:transcribe" => {
                let audio = bytes(&a["audioBytes"])?;
                if audio.is_empty() {
                    return Err("audioBytes is empty".into());
                }
                Ok(match self.transcribe(a, audio).await {
                    Ok(v) => v,
                    Err(error) => json!({"ok":false,"error":error}),
                })
            }
            "voice:stt:modes" => {
                let list = self.list().await?;
                let mut modes = Vec::new();
                if list
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["status"] == "downloaded")
                {
                    modes.push("local-model-assets");
                }
                if self.has("whisper-cli").await {
                    modes.push("local-whisper-cli");
                } else if self.has("whisper-cpp").await {
                    modes.push("local-whisper-cpp");
                }
                if self.runtime("moonshine-python", false).await["ready"] == true {
                    modes.push("local-moonshine-python");
                }
                Ok(json!(modes))
            }
            _ => Err(format!("Unknown voice channel: {channel}")),
        }
    }
    async fn transcribe(&self, request: &Value, audio: Vec<u8>) -> CoreResult {
        let root = self.inner.ctx.paths.temp_dir.clone();
        let work = tokio::task::spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix("tezbar-voice-")
                .tempdir_in(root)
                .map_err(err)
        })
        .await
        .map_err(err)??;
        let result = self.transcribe_in(work.path(), request, audio).await;
        tokio::task::spawn_blocking(move || drop(work))
            .await
            .map_err(err)?;
        result
    }
    async fn transcribe_in(
        &self,
        root: &std::path::Path,
        request: &Value,
        audio: Vec<u8>,
    ) -> CoreResult {
        let mime = s(&request["mimeType"]);
        let ext = if mime.contains("wav") {
            "wav"
        } else if mime.contains("webm") {
            "webm"
        } else if mime.contains("ogg") {
            "ogg"
        } else if mime.contains("mp4") || mime.contains("m4a") || mime.contains("aac") {
            "m4a"
        } else if mime.contains("mpeg") || mime.contains("mp3") {
            "mp3"
        } else if mime.contains("flac") {
            "flac"
        } else {
            "bin"
        };
        let source = root.join(format!("input.{ext}"));
        let wav = root.join("input.wav");
        tokio::fs::write(&source, audio).await.map_err(err)?;
        if ext != "wav" {
            if !self.has("ffmpeg").await {
                return Ok(
                    json!({"ok":false,"error":"Audio arrived in a compressed format but ffmpeg is not installed.","hint":"Install ffmpeg (`brew install ffmpeg`) or enable Web Audio encoding in the renderer."}),
                );
            }
            self.run(
                "ffmpeg",
                &[
                    "-y",
                    "-i",
                    &source.to_string_lossy(),
                    "-ac",
                    "1",
                    "-ar",
                    "16000",
                    "-f",
                    "wav",
                    &wav.to_string_lossy(),
                ],
                60,
                &self.inner.stop,
            )
            .await?;
        }
        let selected = self.selected().await?;
        let selected_model = model(&selected)?;
        let language = request["language"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(
                if selected_model["family"] == "whisper"
                    && selected_model["language"] == "multilingual"
                {
                    "auto"
                } else {
                    "en"
                },
            );
        let mut first_failure = None;
        let binary = if self.has("whisper-cli").await {
            Some("whisper-cli")
        } else if self.has("whisper-cpp").await {
            Some("whisper-cpp")
        } else {
            None
        };
        if let Some(binary) = binary {
            if let Some(model) = self.whisper_model(&selected).await {
                let threads = std::env::var("RAYMES_WHISPER_THREADS")
                    .ok()
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|n| *n > 0)
                    .unwrap_or_else(|| {
                        std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(4)
                            .clamp(4, 12)
                    })
                    .to_string();
                let output_base = root.join("transcript");
                let result = self
                    .run(
                        binary,
                        &[
                            "-m",
                            &model.to_string_lossy(),
                            "-f",
                            &wav.to_string_lossy(),
                            "-l",
                            language,
                            "-t",
                            &threads,
                            "-bo",
                            "1",
                            "-bs",
                            "1",
                            "-nt",
                            "-np",
                            "-nf",
                            "-sns",
                            "-otxt",
                            "-of",
                            &output_base.to_string_lossy(),
                        ],
                        300,
                        &self.inner.stop,
                    )
                    .await;
                match result {
                    Ok(_) => {
                        let text = tokio::fs::read_to_string(output_base.with_extension("txt"))
                            .await
                            .unwrap_or_default();
                        if !text.trim().is_empty() {
                            return Ok(
                                json!({"ok":true,"text":text.trim(),"engine":"whisper-cli"}),
                            );
                        }
                        first_failure =
                            Some(("whisper.cpp", "whisper-cli returned no text.".to_string()));
                    }
                    Err(error) => first_failure = Some(("whisper.cpp", error)),
                }
            }
        }
        if self.runtime("moonshine-python", false).await["ready"] == true {
            let mut models = catalog();
            models.sort_by_key(|m| m["id"] != selected);
            let mut directory = None;
            for m in models {
                if m["family"] == "moonshine" && self.disk(&m).await.0 {
                    directory = Some(self.dir(s(&m["id"])));
                    break;
                }
            }
            let result = if let Some(directory) = directory {
                self.run(
                    "python3",
                    &[
                        "-c",
                        include_str!("moonshine.py"),
                        &wav.to_string_lossy(),
                        &directory.to_string_lossy(),
                        language,
                    ],
                    300,
                    &self.inner.stop,
                )
                .await
            } else {
                Err("Moonshine model files are not downloaded yet. Download a Moonshine model first.".into())
            };
            match result {
                Ok(text) if !text.trim().is_empty() => {
                    return Ok(json!({"ok":true,"text":text.trim(),"engine":"moonshine-python"}))
                }
                Ok(_) => {
                    if first_failure.is_none() {
                        first_failure = Some(("Moonshine", "Moonshine returned no text.".into()));
                    }
                }
                Err(error) => {
                    if first_failure.is_none() {
                        first_failure = Some(("Moonshine", error));
                    }
                }
            }
        }
        if let Some((engine, message)) = first_failure {
            return Ok(
                json!({"ok":false,"error":format!("{engine} failed to transcribe the recording."),"hint":message}),
            );
        }
        Ok(
            json!({"ok":false,"error":"No local speech-to-text engine is available.","hint":"Open Settings → Voice models and click \"Install & download\" on a model. Tezbar will install the required runtime (whisper.cpp via Homebrew or Moonshine via pip) and the weights in one step."}),
        )
    }
    async fn whisper_model(&self, selected: &str) -> Option<std::path::PathBuf> {
        if let Some(path) = std::env::var_os("RAYMES_WHISPER_MODEL") {
            let path = std::path::PathBuf::from(path);
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                return Some(path);
            }
        }
        let mut models = catalog();
        models.sort_by_key(|m| m["id"] != selected);
        for m in models.into_iter().filter(|m| m["family"] == "whisper") {
            for a in m["assets"].as_array().unwrap() {
                let path = self.dir(s(&m["id"])).join(s(&a["fileName"]));
                if tokio::fs::metadata(&path).await.is_ok_and(|m| m.len() > 0) {
                    return Some(path);
                }
            }
        }
        for path in [
            "/opt/homebrew/share/whisper-cpp/ggml-base.en.bin",
            "/opt/homebrew/share/whisper-cpp/ggml-base.bin",
            "/usr/local/share/whisper-cpp/ggml-base.en.bin",
            "/usr/local/share/whisper-cpp/ggml-base.bin",
        ] {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                return Some(path.into());
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_array_json_forms_are_accepted_without_truncation() {
        assert_eq!(bytes(&json!([0, 128, 255])).unwrap(), vec![0, 128, 255]);
        assert_eq!(
            bytes(&json!({"2":255,"0":0,"1":128})).unwrap(),
            vec![0, 128, 255]
        );
        assert!(bytes(&json!({"1":1})).is_err());
        assert!(bytes(&json!([256])).is_err());
    }
    #[test]
    fn catalog_has_real_runtime_assets() {
        let models = catalog();
        assert_eq!(models.len(), 8);
        for m in models {
            assert!(!m["assets"].as_array().unwrap().is_empty());
            for asset in m["assets"].as_array().unwrap() {
                assert!(s(&asset["url"]).starts_with("https://"));
                if m["family"] == "whisper" {
                    assert!(s(&asset["fileName"]).starts_with("ggml-"));
                }
            }
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
    #[cfg(unix)]
    async fn wav_runs_whisper_with_temp_profile_and_cleans_artifacts() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let paths = common::CorePaths {
            data_dir: root.path().join("data"),
            config_dir: root.path().join("config"),
            resource_dir: root.path().join("resources"),
            temp_dir: root.path().join("tmp"),
            home_dir: root.path().join("home"),
        };
        for p in [
            &paths.data_dir,
            &paths.config_dir,
            &paths.resource_dir,
            &paths.temp_dir,
            &paths.home_dir,
        ] {
            std::fs::create_dir_all(p).unwrap();
        }
        let ctx = CoreContext::headless(paths);
        ctx.config
            .patch(&json!({"voiceSttModelId":"whisper-tiny"}))
            .unwrap();
        let voice = Voice::new(ctx.clone());
        let bin = root.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let mock = bin.join("whisper-cli");
        std::fs::write(&mock,"#!/bin/sh\nwhile [ $# -gt 0 ]; do\nif [ \"$1\" = -of ]; then shift; output=$1; fi\nshift\ndone\nprintf 'Native transcript 🦀' > \"$output.txt\"\n").unwrap();
        std::fs::set_permissions(mock, std::fs::Permissions::from_mode(0o755)).unwrap();
        voice
            .inner
            .login_path
            .set(format!("{}:/usr/bin:/bin", bin.display()))
            .unwrap();
        let model = voice.dir("whisper-tiny");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("ggml-tiny.bin"), b"fixture weights").unwrap();
        let result = voice
            .handle(
                "voice:stt:transcribe",
                &[json!({"audioBytes":{"0":82,"1":73,"2":70,"3":70},"mimeType":"audio/wav"})],
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["text"], "Native transcript 🦀");
        assert_eq!(result["engine"], "whisper-cli");
        assert_eq!(std::fs::read_dir(&ctx.paths.temp_dir).unwrap().count(), 0);
        voice.shutdown().await;
    }
}
