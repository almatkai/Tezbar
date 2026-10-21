//! Native port of the installed `speedtest` extension (package.json, .sc-build/index.js).
//! Ookla CLI 1.2.0 archive names and SHA-256 values are taken from src/lib/cli.ts in that bundle.
use crate::native_core::common::{CoreContext, CoreResult};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::v2_sdk;

const START: &str = "speedtest:start";
const AGAIN: &str = "speedtest:run-again";
const COPY: &str = "speedtest:copy-summary";
const OPEN: &str = "speedtest:open-result";
const DETAILS: &str = "speedtest:details";
const CLEAR: &str = "speedtest:clear-cli";
const MAX_ARCHIVE: usize = 32 * 1024 * 1024;
const MAX_OUTPUT: u64 = 2 * 1024 * 1024;

#[cfg(target_os = "macos")]
const PACKAGE: (&str, &str, &str) = (
    "macosx-universal.tgz",
    "c9f8192149ebc88f8699998cecab1ce144144045907ece6f53cf50877f4de66f",
    "speedtest",
);
#[cfg(target_os = "windows")]
const PACKAGE: (&str, &str, &str) = (
    "win64.zip",
    "13e3d888b845d301a556419e31f14ab9bff57e3f06089ef2fd3bdc9ba6841efa",
    "speedtest.exe",
);

pub fn commands() -> &'static [&'static str] {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        &["index"]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[]
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if command != "index" || commands().is_empty() {
        return Err("Speedtest is unavailable on this platform".into());
    }
    match operation {
        "extension:run-command" => {
            *state = json!({});
            Ok(render(state))
        }
        "extension:invoke-action" => {
            let action = payload["actionId"]
                .as_str()
                .ok_or("Missing Speedtest action")?;
            match action {
                START | AGAIN => {
                    // Await the bounded native process. The view returns only after a real CLI result.
                    match run(ctx).await {
                        Ok(result) => *state = json!({"result":result}),
                        Err(error) => *state = json!({"error":error}),
                    }
                    Ok(render(state))
                }
                COPY => {
                    let result = state.get("result").ok_or("Run a speed test first")?;
                    v2_sdk::copy(ctx, &summary(result)?).await?;
                    Ok(render(state))
                }
                OPEN => {
                    let url = state["result"]["result"]["url"]
                        .as_str()
                        .ok_or("No result URL")?;
                    if !url.starts_with("https://www.speedtest.net/result/") {
                        return Err("Unexpected Speedtest result URL".into());
                    }
                    v2_sdk::open(ctx, url).await?;
                    Ok(render(state))
                }
                DETAILS => {
                    let show = !state["details"].as_bool().unwrap_or(false);
                    state["details"] = json!(show);
                    Ok(render(state))
                }
                CLEAR => {
                    if state["confirmClear"] != true {
                        state["confirmClear"] = json!(true);
                        return Ok(render(state));
                    }
                    let path = cli_path(ctx)?;
                    match tokio::fs::remove_file(path).await {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            return Err(format!("Could not remove cached Speedtest CLI: {e}"))
                        }
                    }
                    state["confirmClear"] = json!(false);
                    Ok(render(state))
                }
                _ => Err("Unknown Speedtest action".into()),
            }
        }
        "extension:refresh-session" | "extension:search-text-changed" | "extension:load-more" => {
            Ok(render(state))
        }
        "extension:dispose-session" => Ok(v2_sdk::unchanged()),
        _ => Err("Unsupported Speedtest operation".into()),
    }
}

fn render(state: &Value) -> Value {
    let result = state.get("result");
    let mut actions = vec![
        json!({"id":if result.is_some() { AGAIN } else { START },"title":if result.is_some() { "Run Again" } else { "Start Speed Test" }}),
    ];
    if result.is_some() {
        actions.push(json!({"id":COPY,"title":"Copy Summary"}));
        if state["result"]["result"]["url"]
            .as_str()
            .is_some_and(|s| s.starts_with("https://www.speedtest.net/result/"))
        {
            actions.push(json!({"id":OPEN,"title":"Open Results in Browser"}));
        }
        actions.push(json!({"id":DETAILS,"title":if state["details"] == true {"Hide Details"} else {"Show Details"}}));
    }
    actions.push(json!({"id":CLEAR,"title":if state["confirmClear"] == true { "Confirm Clear Cached CLI" } else { "Clear Cached CLI" },"style":"destructive"}));
    let mut rows = Vec::new();
    if let Some(error) = state["error"].as_str() {
        rows.push(row("error", "Speedtest failed", error));
    }
    if let Some(r) = result {
        for (id, title, value) in [
            ("isp", "ISP", r["isp"].as_str().unwrap_or("").to_string()),
            (
                "server",
                "Server",
                r["server"]["name"].as_str().unwrap_or("").to_string(),
            ),
            (
                "ping",
                "Ping",
                metric(r["ping"]["latency"].as_f64(), "ms", 1),
            ),
            (
                "download",
                "Download",
                speed(r["download"]["bandwidth"].as_f64()),
            ),
            ("upload", "Upload", speed(r["upload"]["bandwidth"].as_f64())),
            (
                "loss",
                "Packet Loss",
                metric(r["packetLoss"].as_f64(), "%", 1),
            ),
            (
                "link",
                "Result Link",
                r["result"]["url"].as_str().unwrap_or("").to_string(),
            ),
        ] {
            rows.push(row(id, title, &value));
        }
        if state["details"] == true {
            for (id, title, value) in [
                ("ping-detail", "Ping details", &r["ping"]),
                ("download-detail", "Download details", &r["download"]),
                ("upload-detail", "Upload details", &r["upload"]),
                ("server-detail", "Server details", &r["server"]),
                ("interface-detail", "Interface details", &r["interface"]),
            ] {
                rows.push(row(id, title, &value.to_string()));
            }
        }
    } else if state.get("error").is_none() {
        rows.push(row(
            "ready",
            "Ready",
            "Start a test to measure your connection with Ookla Speedtest CLI.",
        ));
    }
    v2_sdk::view(
        "Test Internet Speed",
        json!({"type":"List","props":{"searchBarPlaceholder":"Speedtest by Ookla"},"children":rows}),
        actions,
    )
}

fn row(id: &str, title: &str, subtitle: &str) -> Value {
    json!({"type":"List.Item","props":{"id":id,"title":title,"subtitle":subtitle}})
}
fn metric(n: Option<f64>, unit: &str, decimals: usize) -> String {
    match n.filter(|n| n.is_finite() && *n >= 0.0) {
        Some(n) => format!("{n:.decimals$} {unit}"),
        None => "Unavailable".into(),
    }
}
fn speed(n: Option<f64>) -> String {
    metric(n.map(|v| v * 8.0 / 1_000_000.0), "Mbps", 2)
}
fn summary(r: &Value) -> Result<String, String> {
    let isp = r["isp"].as_str().ok_or("Missing ISP")?;
    let server = r["server"]["name"].as_str().ok_or("Missing server")?;
    Ok(format!("ISP: {isp}; Server: {server}; Ping: {}; Download: {}; Upload: {}; Packet Loss: {}; Result: {}",
        metric(r["ping"]["latency"].as_f64(), "ms", 1), speed(r["download"]["bandwidth"].as_f64()),
        speed(r["upload"]["bandwidth"].as_f64()), metric(r["packetLoss"].as_f64(), "%", 1),
        r["result"]["url"].as_str().unwrap_or("")))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn cli_path(ctx: &CoreContext) -> Result<PathBuf, String> {
    Ok(ctx
        .paths
        .data_dir
        .join("native-extensions-v2")
        .join("speedtest")
        .join("cli")
        .join(PACKAGE.2))
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn cli_path(_: &CoreContext) -> Result<PathBuf, String> {
    Err("Speedtest CLI is unavailable on this platform".into())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn ensure_cli(ctx: &CoreContext) -> Result<PathBuf, String> {
    let cli = cli_path(ctx)?;
    if tokio::fs::metadata(&cli).await.is_ok() {
        return Ok(cli);
    }
    let url = format!(
        "https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-{}",
        PACKAGE.0
    );
    let archive = download_archive(&url, PACKAGE.1).await?;
    let parent = cli.parent().ok_or("Invalid Speedtest CLI path")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;
    let temp = tempfile::tempdir_in(parent).map_err(|e| e.to_string())?;
    let archive_path = temp.path().join(PACKAGE.0);
    tokio::fs::write(&archive_path, archive)
        .await
        .map_err(|e| e.to_string())?;
    // Extract one exact member to stdout. No archive path is ever interpreted as a filesystem path.
    let bytes = command_output(
        "tar",
        &[
            "-xOf",
            archive_path.to_str().ok_or("Invalid archive path")?,
            PACKAGE.2,
        ],
        Duration::from_secs(30),
        MAX_ARCHIVE as u64,
    )
    .await?;
    if bytes.is_empty() {
        return Err("Speedtest CLI archive contains no executable".into());
    }
    let staged = temp.path().join(PACKAGE.2);
    tokio::fs::write(&staged, bytes)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(|e| e.to_string())?;
    }
    match tokio::fs::rename(&staged, &cli).await {
        Ok(()) => Ok(cli),
        Err(_) if tokio::fs::metadata(&cli).await.is_ok() => Ok(cli),
        Err(e) => Err(format!("Could not install Speedtest CLI: {e}")),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn download_archive(url: &str, expected_hash: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Speedtest CLI download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Speedtest CLI download failed: {e}"))?;
    let mut stream = response.bytes_stream();
    let mut archive = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Speedtest CLI download failed: {e}"))?;
        if archive.len().saturating_add(chunk.len()) > MAX_ARCHIVE {
            return Err("Speedtest CLI archive is too large".into());
        }
        archive.extend_from_slice(&chunk);
    }
    verify_archive(&archive, expected_hash)?;
    Ok(archive)
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn ensure_cli(_: &CoreContext) -> Result<PathBuf, String> {
    Err("Speedtest CLI is unavailable on this platform".into())
}

fn verify_archive(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = Sha256::digest(bytes);
    let hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
    if hex == expected {
        Ok(())
    } else {
        Err("Speedtest CLI archive SHA-256 mismatch".into())
    }
}

async fn command_output(
    program: impl AsRef<std::ffi::OsStr>,
    args: &[&str],
    timeout: Duration,
    max: u64,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start Speedtest process: {e}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("Missing Speedtest stdout")?
        .take(max + 1);
    let mut stderr = child
        .stderr
        .take()
        .ok_or("Missing Speedtest stderr")?
        .take(16_385);
    let work = async {
        let (out, err, status) = tokio::join!(
            async {
                let mut b = Vec::new();
                stdout.read_to_end(&mut b).await.map(|_| b)
            },
            async {
                let mut b = Vec::new();
                stderr.read_to_end(&mut b).await.map(|_| b)
            },
            child.wait()
        );
        let out = out.map_err(|e| e.to_string())?;
        let err = err.map_err(|e| e.to_string())?;
        let status = status.map_err(|e| e.to_string())?;
        if out.len() as u64 > max || err.len() > 16_384 {
            return Err("Speedtest process output exceeded limit".into());
        }
        if !status.success() {
            let detail = String::from_utf8_lossy(&err);
            return Err(if detail.contains("NoNetworkConnection") {
                "The Internet connection appears to be offline.".into()
            } else {
                format!("Speedtest CLI failed ({}): {}", status, detail.trim())
            });
        }
        Ok(out)
    };
    tokio::time::timeout(timeout, work)
        .await
        .map_err(|_| "Speedtest process timed out".to_string())?
}

async fn run(ctx: &CoreContext) -> Result<Value, String> {
    let cli = ensure_cli(ctx).await?;
    let output = command_output(
        &cli,
        &[
            "--format",
            "json",
            "--progress",
            "--accept-license",
            "--accept-gdpr",
        ],
        Duration::from_secs(180),
        MAX_OUTPUT,
    )
    .await?;
    parse_events(&output)
}

fn parse_events(output: &[u8]) -> Result<Value, String> {
    let mut result = None;
    let mut failure = None;
    let mut progress = json!({});
    for line in output
        .split(|b| *b == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
    {
        let event: Value = serde_json::from_slice(line)
            .map_err(|_| "Could not read data from Speedtest CLI".to_string())?;
        match event["type"].as_str() {
            Some("result") => result = Some(event),
            Some("error") => {
                failure = Some(
                    event["message"]
                        .as_str()
                        .unwrap_or("Speedtest CLI reported an error")
                        .to_string(),
                )
            }
            Some(kind @ ("ping" | "download" | "upload")) => {
                if let Some(value) = event[kind]["progress"]
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                {
                    progress[kind] = json!(value);
                }
            }
            Some("testStart") => {}
            _ => {}
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    let mut result = result.ok_or("Speedtest CLI did not report a result")?;
    for field in ["ping", "download", "upload", "server", "result"] {
        if result.get(field).is_none() {
            return Err(format!("Speedtest CLI result missing {field}"));
        }
    }
    for (field, metric) in [
        ("ping", "latency"),
        ("download", "bandwidth"),
        ("upload", "bandwidth"),
    ] {
        if !result[field][metric]
            .as_f64()
            .is_some_and(|n| n.is_finite() && n >= 0.0)
        {
            return Err(format!("Speedtest CLI result has invalid {field} {metric}"));
        }
    }
    if result["server"]["name"].as_str().is_none() {
        return Err("Speedtest CLI result missing server name".into());
    }
    result["_nativeProgress"] = progress;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_ndjson_shape_without_fabricated_metrics() {
        let bytes = br#"{"type":"testStart","isp":"Fixture ISP","server":{"name":"Fixture"}}
{"type":"ping","ping":{"latency":12.3}}
{"type":"download","download":{"bandwidth":1000000,"progress":0.5}}
{"type":"result","isp":"Fixture ISP","server":{"name":"Fixture"},"ping":{"latency":12.3},"download":{"bandwidth":1000000},"upload":{"bandwidth":500000},"packetLoss":0,"result":{"url":"https://www.speedtest.net/result/123"}}
"#;
        let result = parse_events(bytes).unwrap();
        assert_eq!(speed(result["download"]["bandwidth"].as_f64()), "8.00 Mbps");
        assert_eq!(result["_nativeProgress"]["download"], 0.5);
        assert!(summary(&result).unwrap().contains("Packet Loss: 0.0 %"));
        assert!(parse_events(br#"{"type":"ping","ping":{"latency":1}}"#).is_err());
    }
    #[test]
    fn rejects_corrupt_archive_and_cli_error() {
        assert!(verify_archive(b"untrusted", "0000").is_err());
        assert_eq!(
            parse_events(br#"{"type":"error","message":"NoNetworkConnection"}"#).unwrap_err(),
            "NoNetworkConnection"
        );
    }
    #[test]
    fn initial_view_has_start_and_no_made_up_results() {
        let view = render(&json!({}));
        assert_eq!(view["actions"][0]["id"], START);
        assert_eq!(view["root"]["children"].as_array().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_cli_exercises_process_boundary_without_network_test() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-speedtest");
        std::fs::write(&cli, "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"result\",\"isp\":\"Fixture\",\"server\":{\"name\":\"Fixture\"},\"ping\":{\"latency\":5},\"download\":{\"bandwidth\":1000000},\"upload\":{\"bandwidth\":500000},\"result\":{\"url\":\"https://www.speedtest.net/result/1\"}}'\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = command_output(&cli, &["--format", "json"], Duration::from_secs(2), 4096)
            .await
            .unwrap();
        assert_eq!(parse_events(&output).unwrap()["ping"]["latency"], 5);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[tokio::test]
    async fn local_http_fixture_checks_hash_before_install() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = b"archive fixture";
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\narchive fixture")
                .await
                .unwrap();
        });
        let url = format!("http://{address}/fixture");
        let digest: String = Sha256::digest(fixture)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(download_archive(&url, &digest).await.unwrap(), fixture);
        server.await.unwrap();
    }
}
