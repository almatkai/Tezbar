//! Grounding and image request shapes ported from pi-antigravity 0.11.0,
//! MIT © Rahul Arya. See licenses/pi-antigravity-MIT.txt.
use super::super::*;
use base64::Engine;
use futures_util::StreamExt;
use runtime::transcript::Block;
use std::path::Path;
pub(in crate::ai) fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"google_search","description":"Research with Google grounding through the linked Antigravity account. Returns source URLs as untrusted evidence.","parameters":{"type":"object","properties":{"query":{"type":"string"},"instruction":{"type":"string"},"urls":{"type":"array","items":{"type":"string"},"maxItems":20},"thinking":{"type":"boolean"}},"required":["query"],"additionalProperties":false}}}),
        json!({"type":"function","function":{"name":"generate_image","description":"Generate an image through the linked Antigravity account. Saves files in generated-images under cwd.","parameters":{"type":"object","properties":{"prompt":{"type":"string"},"model":{"type":"string"},"path":{"type":"string"},"aspectRatio":{"type":"string","enum":["1:1","2:3","3:2","3:4","4:3","4:5","5:4","9:16","16:9","21:9"]}},"required":["prompt"],"additionalProperties":false}}}),
    ]
}
pub(in crate::ai) fn validate(name: &str, args: &Value) -> Result<(), String> {
    let allowed = if name == "google_search" {
        vec!["query", "instruction", "urls", "thinking"]
    } else {
        vec!["prompt", "aspectRatio", "model", "path"]
    };
    let object = args.as_object().ok_or("Tool arguments must be an object")?;
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Unknown tool argument".into());
    }
    let text = s(&args[if name == "google_search" {
        "query"
    } else {
        "prompt"
    }]);
    if text.trim().is_empty() || text.len() > 32000 {
        return Err("Enter a prompt or query under 32 KiB".into());
    }
    if name == "google_search" {
        if args["instruction"]
            .as_str()
            .is_some_and(|s| s.len() > 32000)
            || args["urls"].as_array().is_some_and(|urls| {
                urls.len() > 20
                    || urls.iter().any(|url| {
                        reqwest::Url::parse(s(url))
                            .ok()
                            .is_none_or(|u| !matches!(u.scheme(), "http" | "https"))
                    })
            })
        {
            return Err("Search instructions or web addresses exceed their limits".into());
        }
    }
    if name == "generate_image" {
        if let Some(model) = args["model"].as_str() {
            if model.len() > 80
                || !regex::Regex::new(
                    "(?i)^(gemini-[a-z0-9.+-]*image[a-z0-9.+-]*|imagen-[a-z0-9.+-]+)$",
                )
                .unwrap()
                .is_match(model)
            {
                return Err("Unsupported image model".into());
            }
        }
        if let Some(path) = args["path"].as_str() {
            if std::path::Path::new(path).components().count() != 1 || path == "." || path == ".." {
                return Err("Use a filename inside generated-images".into());
            }
        }
        let ratio = args["aspectRatio"].as_str().unwrap_or("1:1");
        if ![
            "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9",
        ]
        .contains(&ratio)
        {
            return Err("Unsupported image aspect ratio".into());
        }
    }
    Ok(())
}
impl AiCore {
    pub(in crate::ai) async fn native_antigravity_tool(
        &self,
        name: &str,
        args: Value,
        root: &Path,
        mode: Option<&str>,
        run: &str,
        cancel: &Cancel,
    ) -> Result<(Vec<Block>, bool), String> {
        let canonical_root = tokio::fs::canonicalize(root).await.map_err(err)?;
        let root = canonical_root.as_path();
        validate(name, &args)?;
        let cfg = self.read_config().await?;
        let mode = mode
            .or_else(|| cfg["agentApprovalMode"].as_str())
            .unwrap_or("default");
        let generate = name == "generate_image";
        let directory = native_tools::create_path(root, "generated-images")?;
        if generate && !runtime::policy::within(&directory, root) {
            return Err("Generated images directory points outside cwd".into());
        }
        let sensitive = runtime::policy::sensitive(
            &directory,
            &self.inner.ctx.paths.home_dir,
            &self.inner.ctx.paths.config_dir,
        );
        let ask = if generate {
            runtime::policy::file_asks(true, false, sensitive, mode)
        } else {
            mode == "review"
        };
        if ask
            && !self
                .approval_prompt(
                    run,
                    if generate {
                        "Allow image generation?"
                    } else {
                        "Allow Google search?"
                    },
                    &args.to_string(),
                    cancel,
                    false,
                )
                .await
        {
            return Err("Native permission: tool was denied".into());
        }
        if cancel.is_cancelled() {
            return Err("Operation cancelled".into());
        }
        let account = self.antigravity_account().await?;
        let token = s(&account["access"]);
        let project = s(&account["projectId"]);
        let model = if generate {
            args["model"].as_str().unwrap_or("gemini-3.1-flash-image")
        } else {
            "gemini-3.5-flash-lite"
        };
        let mut request = json!({"contents":[{"role":"user","parts":[{"text":args[if generate{"prompt"}else{"query"}]}]}]});
        if generate {
            request["generationConfig"] = json!({"imageConfig":{"aspectRatio":args["aspectRatio"].as_str().unwrap_or("1:1")},"candidateCount":1});
        } else {
            request["systemInstruction"] = json!({"role":"user","parts":[{"text":"Research using Google Search grounding. Prefer primary documentation. Return compact findings, gaps and next checks. Distinguish sourced evidence from inference."}]});
            request["tools"] = json!([{"googleSearch":{}}]);
        }
        if !generate {
            if let Some(instruction) = args["instruction"].as_str() {
                let old = s(&request["systemInstruction"]["parts"][0]["text"]);
                request["systemInstruction"]["parts"][0]["text"] =
                    json!(format!("{old}\n\nSearch instructions: {instruction}"));
            }
            if let Some(urls) = args["urls"].as_array().filter(|a| !a.is_empty()) {
                let text = s(&request["contents"][0]["parts"][0]["text"]);
                request["contents"][0]["parts"][0]["text"] = json!(format!(
                    "{text}\n\nURLs to analyze: {}",
                    urls.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
                request["tools"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"urlContext":{}}));
            }
            if args["thinking"] == true {
                request["generationConfig"]["thinkingConfig"] =
                    json!({"thinkingBudget":4096,"includeThoughts":false});
            }
        }
        let body = json!({"project":project,"model":model,"request":request,"requestType":if generate{"image_gen"}else{"agent"},"userAgent":"antigravity","requestId":format!("{}/{}/{}/1",if generate{"image_gen"}else{"agent"},now_ms(),uuid::Uuid::new_v4())});
        let ag_cfg = self.cfg(None, Some("antigravity")).await?;
        let base = self
            .antigravity_bases(&ag_cfg)?
            .into_iter()
            .next()
            .ok_or("No Antigravity endpoint")?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(err)?;
        let make = |url: String| {
            http.post(url)
                .header("User-Agent", super::antigravity::USER_AGENT)
                .bearer_auth(token)
                .json(&body)
                .timeout(Duration::from_secs(120))
        };
        let work = async {
            let mut response = make(format!("{base}/v1internal:generateContent"))
                .send()
                .await
                .map_err(|_| "Connection lost. The outcome of this call is unknown.")?;
            if response.status().as_u16() == 404 {
                response = make(format!("{base}/v1internal:streamGenerateContent?alt=sse"))
                    .send()
                    .await
                    .map_err(|_| "Connection lost. The outcome of this call is unknown.")?;
            }
            if !response.status().is_success() {
                return Err(format!(
                    "Antigravity tool request failed ({}). No automatic tool replay.",
                    response.status().as_u16()
                ));
            }
            let sse = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|t| t.contains("text/event-stream"));
            let mut stream = response.bytes_stream();
            let mut bytes = vec![];
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|_| "Connection lost. The outcome of this call is unknown.")?;
                if bytes.len() + chunk.len() > 48 * 1024 * 1024 {
                    return Err("Antigravity tool response exceeds 48 MiB".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            let mut values = vec![];
            if sse {
                let mut frame = process::Lines::default();
                let mut data = vec![];
                for line in frame.feed(&bytes, true)? {
                    if let Some(d) = line.strip_prefix("data:") {
                        data.push(d.trim_start().to_string())
                    } else if line.is_empty() && !data.is_empty() {
                        values.push(
                            serde_json::from_str::<Value>(&std::mem::take(&mut data).join("\n"))
                                .map_err(|_| "Invalid Antigravity tool stream")?,
                        );
                    }
                }
                if !data.is_empty() {
                    values.push(
                        serde_json::from_str(&data.join("\n"))
                            .map_err(|_| "Invalid Antigravity tool stream")?,
                    );
                }
            } else {
                values.push(
                    serde_json::from_slice(&bytes)
                        .map_err(|_| "Invalid Antigravity tool response")?,
                );
            }
            let mut text = String::new();
            let mut images = vec![];
            let mut sources = std::collections::BTreeSet::new();
            for value in values {
                let response = value.get("response").unwrap_or(&value);
                for candidate in response["candidates"].as_array().into_iter().flatten() {
                    for part in candidate["content"]["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if part["thought"] != true {
                            if let Some(t) = part["text"].as_str() {
                                text.push_str(t);
                            }
                        }
                        if part.get("inlineData").is_some() {
                            let normalized = legacy::normalize_images(&[
                                json!({"mimeType":part["inlineData"]["mimeType"],"data":part["inlineData"]["data"]}),
                            ])?;
                            if images.len() < 4 {
                                images.push(normalized[0].clone());
                            }
                        }
                    }
                    for chunk in candidate["groundingMetadata"]["groundingChunks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        let uri = s(&chunk["web"]["uri"]);
                        if reqwest::Url::parse(uri)
                            .ok()
                            .is_some_and(|u| matches!(u.scheme(), "http" | "https"))
                        {
                            sources.insert(format!("{}: {uri}", s(&chunk["web"]["title"])));
                        }
                    }
                }
            }
            for source in sources {
                text.push_str(&format!("\n{source}"));
            }
            let mut blocks = vec![];
            if generate {
                if images.is_empty() {
                    return Err("Antigravity returned no image. No file was written.".into());
                }
                let resolved = native_tools::create_path(root, "generated-images")?;
                if resolved != directory || !runtime::policy::within(&resolved, root) {
                    return Err("Generated images directory changed since approval".into());
                }
                let image_count = images.len();
                for (index, image) in images.into_iter().enumerate() {
                    if cancel.is_cancelled() {
                        return Err("Operation cancelled".into());
                    }
                    let ext = match s(&image["mimeType"]) {
                        "image/jpeg" => "jpg",
                        "image/webp" => "webp",
                        _ => "png",
                    };
                    let path = directory.join(
                        args["path"]
                            .as_str()
                            .map(|name| {
                                if image_count == 1 {
                                    name.to_string()
                                } else {
                                    let path = Path::new(name);
                                    let stem = path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("image");
                                    let suffix =
                                        path.extension().and_then(|s| s.to_str()).unwrap_or(ext);
                                    format!("{stem}-{}.{suffix}", index + 1)
                                }
                            })
                            .unwrap_or_else(|| format!("image-{}.{ext}", uuid::Uuid::new_v4())),
                    );
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(s(&image["data"]))
                        .map_err(|_| "Invalid image data")?;
                    let path_write = path.clone();
                    let cancel = cancel.clone();
                    tokio::task::spawn_blocking(move || {
                        use std::io::Write;
                        if cancel.is_cancelled() {
                            return Err("Operation cancelled".to_string());
                        }
                        cancel.effect(|| {
                            std::fs::create_dir_all(
                                path_write.parent().ok_or("No image directory")?,
                            )
                            .map_err(err)
                        })?;
                        let mut temp =
                            tempfile::NamedTempFile::new_in(path_write.parent().unwrap())
                                .map_err(err)?;
                        temp.write_all(&bytes).map_err(err)?;
                        temp.as_file().sync_all().map_err(err)?;
                        if cancel.is_cancelled() {
                            return Err("Operation cancelled".into());
                        }
                        cancel.effect(|| temp.persist(&path_write).map_err(err).map(|_| ()))?;
                        Ok(())
                    })
                    .await
                    .map_err(err)??;
                    text.push_str(&format!("\nSaved image: {}", path.display()));
                    blocks.push(Block::Image {
                        mime: s(&image["mimeType"]).into(),
                        data: s(&image["data"]).into(),
                    });
                }
            }
            blocks.insert(
                0,
                Block::Text {
                    text: cut(&redact_text(&text).replace(token, "[redacted]"), 64000),
                },
            );
            Ok((blocks, false))
        };
        tokio::select! {biased;_=cancel.cancelled()=>Err("Operation cancelled".into()),result=work=>result}
    }
}
