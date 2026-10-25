//! Headless entry point for external Rust extension discovery and installation.
use crate::native_core::common::{CoreContext, CorePaths};
use crate::native_core::extensions::{v2_cli_install, v2_cli_list};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Action {
    List { json: bool },
    Install { id: String },
    Import { manifest: PathBuf, zip: PathBuf },
    Invalid(String),
}

fn parse(args: &[String]) -> Option<Action> {
    match args.first()?.as_str() {
        "--extensions-v2-list" => match &args[1..] {
            [] => Some(Action::List { json: false }),
            [flag] if flag == "--json" => Some(Action::List { json: true }),
            _ => Some(Action::Invalid(
                "Usage: --extensions-v2-list [--json]".into(),
            )),
        },
        "--extensions-v2-install" => match &args[1..] {
            [id] if id == "--all" || id.starts_with("com.tezbar.native.") => {
                Some(Action::Install { id: id.clone() })
            }
            _ => Some(Action::Invalid(
                "Usage: --extensions-v2-install <canonical-id|--all>".into(),
            )),
        },
        "--extensions-v3-import" => match &args[1..] {
            [manifest, zip] => Some(Action::Import {
                manifest: manifest.into(),
                zip: zip.into(),
            }),
            _ => Some(Action::Invalid(
                "Usage: --extensions-v3-import <trusted-manifest.json> <package.zip>".into(),
            )),
        },
        _ => None,
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn paths_from<F>(
    get: F,
    platform: &str,
    executable: &Path,
    temp_dir: PathBuf,
) -> Result<CorePaths, String>
where
    F: Fn(&str) -> Option<PathBuf>,
{
    let home_dir = get(if platform == "windows" {
        "USERPROFILE"
    } else {
        "HOME"
    })
    .ok_or("Cannot determine home directory")?;
    let data_dir = if platform == "windows" {
        get("TEZBAR_DATA_DIR")
            .or_else(|| get("LOCALAPPDATA").map(|p| p.join("com.tezbar.app")))
            .unwrap_or_else(|| home_dir.join("AppData/Local/com.tezbar.app"))
    } else if platform == "macos" {
        get("TEZBAR_DATA_DIR")
            .unwrap_or_else(|| home_dir.join("Library/Application Support/com.tezbar.app"))
    } else {
        get("TEZBAR_DATA_DIR")
            .or_else(|| get("XDG_DATA_HOME").map(|p| p.join("com.tezbar.app")))
            .unwrap_or_else(|| home_dir.join(".local/share/com.tezbar.app"))
    };
    let config_dir = get("TEZBAR_CONFIG_DIR").unwrap_or_else(|| home_dir.join(".openray"));
    let executable_dir = executable
        .parent()
        .ok_or("Cannot determine executable directory")?;
    let resource_dir = if platform == "macos"
        && executable_dir.file_name() == Some(OsStr::new("MacOS"))
        && executable_dir.parent().and_then(Path::file_name) == Some(OsStr::new("Contents"))
    {
        executable_dir.parent().unwrap().join("Resources")
    } else {
        executable_dir.to_path_buf()
    };
    Ok(CorePaths {
        data_dir,
        config_dir,
        resource_dir,
        temp_dir,
        home_dir,
    })
}

fn cli_context() -> Result<CoreContext, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let paths = paths_from(
        env_path,
        std::env::consts::OS,
        &executable,
        std::env::temp_dir(),
    )?;
    Ok(CoreContext::new(paths, None))
}

// Same offline file/manifest/asset verification used by UI and launch. This
// path never constructs a Tokio runtime, starts a child or performs networking.
fn list(ctx: &CoreContext) -> Result<Value, String> {
    v2_cli_list(ctx)
}

async fn install(ctx: &CoreContext, id: &str) -> (Value, bool) {
    let ids: Vec<String> = if id == "--all" {
        match list(ctx) {
            Ok(items) => items
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item["id"].as_str().map(str::to_owned))
                .collect(),
            Err(error) => {
                return (
                    json!({"operation":"install","results":[],"error":error}),
                    false,
                )
            }
        }
    } else {
        vec![id.to_owned()]
    };
    let mut all_ok = true;
    let mut results = Vec::new();
    for id in ids {
        results.push(match v2_cli_install(ctx, &id).await {
            Ok(value) => json!({"id":id,"ok":true,"result":value}),
            Err(error) => {
                all_ok = false;
                json!({"id":id,"ok":false,"error":error})
            }
        });
    }
    (json!({"operation":"install","results":results}), all_ok)
}

fn print_list(items: &Value) {
    println!("ID\tRUNTIME\tCOMMANDS\tSTATUS\tINSTALL\tPLATFORMS\tCHECKED");
    for item in items.as_array().into_iter().flatten() {
        let commands = item["commands"].as_array();
        let covered = commands
            .into_iter()
            .flatten()
            .filter(|c| c["runtime"] == "rust")
            .count();
        let total = commands.map_or(0, Vec::len);
        let platforms = item["platforms"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let checked = item["verification"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| {
                format!(
                    "{}:{}",
                    v["platform"].as_str().unwrap_or("?"),
                    v["state"].as_str().unwrap_or("unknown")
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{}\t{}\t{covered}/{total}\t{}\t{}\t{platforms}\t{checked}",
            item["id"].as_str().unwrap_or("?"),
            item["runtime"].as_str().unwrap_or("?"),
            item["status"].as_str().unwrap_or("?"),
            item["installationStatus"].as_str().unwrap_or("?")
        );
    }
}

fn run(action: Action, ctx: Result<CoreContext, String>) -> i32 {
    if let Action::Invalid(error) = &action {
        println!("{}", json!({"error":error}));
        return 2;
    }
    let ctx = match ctx {
        Ok(ctx) => ctx,
        Err(error) => {
            println!("{}", json!({"error":error}));
            return 1;
        }
    };
    match action {
        Action::List { json: as_json } => match list(&ctx) {
            Ok(items) => {
                if as_json {
                    println!("{items}");
                } else {
                    print_list(&items);
                }
                0
            }
            Err(error) => {
                println!("{}", json!({"error":error}));
                1
            }
        },
        Action::Install { id } => {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    println!("{}", json!({"error":e.to_string()}));
                    return 1;
                }
            };
            let (result, ok) = runtime.block_on(install(&ctx, &id));
            println!("{result}");
            if ok {
                0
            } else {
                1
            }
        }
        Action::Import { manifest, zip } => {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    println!("{}", json!({"error":e.to_string()}));
                    return 1;
                }
            };
            match runtime.block_on(crate::native_core::extensions::v3_cli_import(
                &ctx, manifest, zip,
            )) {
                Ok(value) => {
                    println!("{}", json!({"ok":true,"result":value}));
                    0
                }
                Err(error) => {
                    println!("{}", json!({"ok":false,"error":error}));
                    1
                }
            }
        }
        Action::Invalid(_) => unreachable!(),
    }
}

/// Returns None for unrelated arguments so normal Tauri startup can proceed.
pub fn dispatch() -> Option<i32> {
    let args = std::env::args_os()
        .skip(1)
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let action = parse(&args)?;
    Some(run(action, cli_context()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_private_zip_cli_import_and_remote_reinstall_refusal() {
        let Some(manifest) = std::env::var_os("TEZBAR_HOST_TEST_PRIVATE_MANIFEST") else {
            eprintln!("Published private CLI fixture not supplied");
            return;
        };
        let zip = std::env::var_os("TEZBAR_HOST_TEST_PRIVATE_ZIP")
            .expect("Published private ZIP required");
        let trusted = crate::native_core::common::read_json(Path::new(&manifest)).unwrap();
        let id = trusted["id"].as_str().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: temp.path().join("data"),
            config_dir: temp.path().join("config"),
            resource_dir: temp.path().join("resources"),
            temp_dir: temp.path().join("tmp"),
            home_dir: temp.path().into(),
        });
        let before = list(&ctx).unwrap();
        let item = before
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap();
        assert_eq!(item["acquisition"]["type"], "local-import-only");
        assert_eq!(item["status"], "unavailable");
        assert_eq!(
            run(
                Action::Import {
                    manifest: manifest.clone().into(),
                    zip: zip.into()
                },
                Ok(ctx.clone())
            ),
            0
        );
        let items = list(&ctx).unwrap();
        let item = items
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap();
        assert_eq!(item["installationStatus"], "installed");
        assert_eq!(item["status"], "complete");
        assert!(item["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["runtime"] == "rust"));
        assert_eq!(item["acquisition"]["type"], "local-import-only");
        let package = ctx.paths.data_dir.join("extensions/packages").join(id);
        let pointer = std::fs::read(package.join("active.json")).unwrap();
        assert_eq!(run(Action::Install { id: id.into() }, Ok(ctx.clone())), 1);
        let core = crate::native_core::extensions::ExtensionsCore::new(ctx);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut references = vec![id, trusted["repository"].as_str().unwrap()];
                references.extend(
                    trusted["replaces"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap()),
                );
                for reference in references {
                    let error = core
                        .handle("extensions:reinstall", &[json!(reference)])
                        .await
                        .unwrap()
                        .unwrap_err();
                    assert!(error.contains("--extensions-v3-import"));
                }
                assert!(!core.worker_running().await);
            });
        assert_eq!(std::fs::read(package.join("active.json")).unwrap(), pointer);
        eprintln!("CLI acceptance: final published private ZIP imported, discovered with full coverage, and remote reinstall refused; reference {}",Path::new(&manifest).display());
    }

    #[test]
    fn parser_claims_only_native_cli_arguments() {
        assert_eq!(parse(&["--other".into()]), None);
        assert_eq!(
            parse(&["--extensions-v2-list".into(), "--json".into()]),
            Some(Action::List { json: true })
        );
        assert!(matches!(
            parse(&["--extensions-v2-install".into(), "raycast.timers".into()]),
            Some(Action::Invalid(_))
        ));
        assert_eq!(
            parse(&["--extensions-v2-install".into(), "--all".into()]),
            Some(Action::Install { id: "--all".into() })
        );
    }

    #[test]
    fn platform_paths_and_overrides_are_isolated() {
        let get = |key: &str| match key {
            "HOME" => Some(PathBuf::from("/tmp/test-home")),
            "USERPROFILE" => Some(PathBuf::from("C:/Users/test")),
            "LOCALAPPDATA" => Some(PathBuf::from("C:/Users/test/AppData/Local")),
            "TEZBAR_CONFIG_DIR" => Some(PathBuf::from("/tmp/test-config")),
            _ => None,
        };
        let mac = paths_from(
            get,
            "macos",
            Path::new("/Applications/Tezbar.app/Contents/MacOS/tezbar"),
            PathBuf::from("/tmp"),
        )
        .unwrap();
        assert_eq!(
            mac.resource_dir,
            PathBuf::from("/Applications/Tezbar.app/Contents/Resources")
        );
        assert_eq!(mac.config_dir, PathBuf::from("/tmp/test-config"));
        let win = paths_from(
            get,
            "windows",
            Path::new("C:/Tezbar/tezbar.exe"),
            PathBuf::from("C:/Temp"),
        )
        .unwrap();
        assert_eq!(
            win.data_dir,
            PathBuf::from("C:/Users/test/AppData/Local/com.tezbar.app")
        );
    }

    #[test]
    fn legacy_matching_markers_are_not_executable_installations() {
        let root = std::env::temp_dir().join(format!(
            "tezbar-v2-cli-test-{}-{}",
            std::process::id(),
            crate::native_core::common::now_ms()
        ));
        let id = "com.tezbar.native.perplexity";
        let package = root.join("extensions/packages").join(id);
        std::fs::create_dir_all(&package).unwrap();
        let ctx = CoreContext::headless(CorePaths {
            data_dir: root.clone(),
            config_dir: root.join("config"),
            resource_dir: root.join("resources"),
            temp_dir: root.join("tmp"),
            home_dir: root.clone(),
        });
        let status = || {
            list(&ctx)
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["id"] == id)
                .unwrap()["installed"]
                .clone()
        };
        assert_eq!(status(), false);
        let metadata =
            json!({"id":id,"schemaVersion":2,"runtime":"rust","version":"1.0"}).to_string();
        std::fs::write(package.join("package.json"), &metadata).unwrap();
        assert_eq!(status(), false);
        std::fs::write(package.join("native-v2.json"), &metadata).unwrap();
        assert_eq!(status(), false);
        std::fs::remove_dir_all(root).unwrap();
    }
}
