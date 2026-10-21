//! Headless entry point for bundled Rust extension discovery and installation.
use crate::native_core::common::{CoreContext, CorePaths};
use crate::native_core::extensions::{v2_cli_install, v2_cli_list};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Action {
    List { json: bool },
    Install { id: String },
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

// The core's installed flag is intentionally lightweight. The CLI reports an
// installation only after both files prove ownership of this canonical ID.
fn installation_status(data_dir: &Path, item: &Value) -> &'static str {
    let Some(id) = item["id"].as_str() else {
        return "invalid";
    };
    let root = data_dir.join("extensions/packages").join(id);
    let package_path = root.join("package.json");
    let marker_path = root.join("native-v2.json");
    if !package_path.exists() && !marker_path.exists() {
        return "not-installed";
    }
    let valid = [package_path, marker_path].iter().all(|path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| {
                value["id"] == id
                    && value["schemaVersion"] == 2
                    && value["runtime"] == "rust"
                    && value["version"] == item["version"]
            })
    });
    if valid {
        "installed"
    } else {
        "invalid"
    }
}

fn list(ctx: &CoreContext) -> Result<Value, String> {
    let mut items = v2_cli_list(ctx)?;
    let items_array = items
        .as_array_mut()
        .ok_or("Native V2 catalog is not an array")?;
    for item in items_array {
        let state = installation_status(&ctx.paths.data_dir, item);
        item["installed"] = json!(state == "installed");
        item["installationStatus"] = json!(state);
    }
    Ok(items)
}

fn install(ctx: &CoreContext, id: &str) -> (Value, bool) {
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
    let results: Vec<Value> = ids
        .into_iter()
        .map(|id| match v2_cli_install(ctx, &id) {
            Ok(value) => json!({"id":id,"ok":true,"result":value}),
            Err(error) => {
                all_ok = false;
                json!({"id":id,"ok":false,"error":error})
            }
        })
        .collect();
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
            let (result, ok) = install(&ctx, &id);
            println!("{result}");
            if ok {
                0
            } else {
                1
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
    fn installed_metadata_requires_matching_package_and_marker() {
        let root = std::env::temp_dir().join(format!(
            "tezbar-v2-cli-test-{}-{}",
            std::process::id(),
            crate::native_core::common::now_ms()
        ));
        let id = "com.tezbar.native.test";
        let package = root.join("extensions/packages").join(id);
        std::fs::create_dir_all(&package).unwrap();
        let item = json!({"id":id,"version":"1.0"});
        assert_eq!(installation_status(&root, &item), "not-installed");
        let metadata =
            json!({"id":id,"schemaVersion":2,"runtime":"rust","version":"1.0"}).to_string();
        std::fs::write(package.join("package.json"), &metadata).unwrap();
        assert_eq!(installation_status(&root, &item), "invalid");
        std::fs::write(package.join("native-v2.json"), &metadata).unwrap();
        assert_eq!(installation_status(&root, &item), "installed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
