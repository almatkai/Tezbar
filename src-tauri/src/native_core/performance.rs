//! Opt-in release benchmarks. These helpers are compiled only into the test binary.
use serde_json::{json, Value};
use std::time::Instant;

pub fn samples() -> usize {
    std::env::var("TEZBAR_BENCH_SAMPLES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(40usize)
        .clamp(10, 10_000)
}

pub fn measure(name: &str, mut run: impl FnMut() -> Value) -> Value {
    let cold = Instant::now();
    let reference = run();
    let first_ms = cold.elapsed().as_secs_f64() * 1000.;
    for _ in 0..3 {
        assert_eq!(run(), reference, "unstable fixture: {name}");
    }
    let mut elapsed = Vec::with_capacity(samples());
    for _ in 0..samples() {
        let start = Instant::now();
        let actual = run();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.);
        assert_eq!(actual, reference, "result changed: {name}");
        std::hint::black_box(actual);
    }
    elapsed.sort_by(f64::total_cmp);
    let percentile = |p: usize| elapsed[(elapsed.len() * p).div_ceil(100) - 1];
    let result = json!({
        "name":name, "samples":elapsed.len(), "first_ms":first_ms,
        "mean_ms":elapsed.iter().sum::<f64>() / elapsed.len() as f64,
        "p50_ms":percentile(50), "p95_ms":percentile(95), "p99_ms":percentile(99),
        "results":signature(&reference),
    });
    eprintln!("TEZBAR_PERF {result}");
    result
}

pub fn signature(rows: &Value) -> Value {
    json!(rows
        .as_array()
        .expect("result list")
        .iter()
        .map(|row| {
            json!({"id":row.get("id").or_else(||row.get("chunkId")),
            "sourceId":row.get("sourceId"), "score":row.get("score")})
        })
        .collect::<Vec<_>>())
}

#[cfg(unix)]
fn cpu_seconds() -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
        0
    );
    let usage = unsafe { usage.assume_init() };
    usage.ru_utime.tv_sec as f64
        + usage.ru_stime.tv_sec as f64
        + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1_000_000.
}

#[cfg(unix)]
fn rss_kib() -> u64 {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps for process RSS");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "30-second headless core idle CPU/RSS baseline; excludes WebView and models"]
async fn core_idle() {
    use super::{
        common::{write_json_atomic, CoreContext, CorePaths},
        NativeCore,
    };
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    let dir = tempfile::tempdir().unwrap();
    let paths = CorePaths {
        data_dir: dir.path().join("data"),
        config_dir: dir.path().join("config"),
        resource_dir: dir.path().join("resources"),
        temp_dir: dir.path().join("temp"),
        home_dir: dir.path().join("home"),
    };
    write_json_atomic(
        &paths.config_dir.join("config.json"),
        &json!({"clipboardWatchEnabled":false}),
    )
    .unwrap();
    let events = Arc::new(AtomicU64::new(0));
    let count = events.clone();
    let ctx = CoreContext::headless(paths).with_event_sink(Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
    }));
    let start = Instant::now();
    let core = NativeCore::new(ctx).unwrap();
    core.start().await.unwrap();
    let ready_ms = start.elapsed().as_secs_f64() * 1000.;
    core.request("knowledge:snapshot", &[]).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let rss_before = rss_kib();
    let events_before = events.load(Ordering::Relaxed);
    let cpu = cpu_seconds();
    let idle = Instant::now();
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    let wall = idle.elapsed().as_secs_f64();
    let used = cpu_seconds() - cpu;
    eprintln!(
        "TEZBAR_PERF {}",
        json!({"name":"core.idle", "ready_ms":ready_ms,
        "wall_seconds":wall, "cpu_seconds":used, "one_core_cpu_percent":used / wall * 100.,
        "rss_before_kib":rss_before, "rss_after_kib":rss_kib(),
        "events":events.load(Ordering::Relaxed)-events_before,
        "scope":"headless native core, empty isolated profile; no WebView/model/app enumeration/file watcher"})
    );
    core.shutdown().await;
}
