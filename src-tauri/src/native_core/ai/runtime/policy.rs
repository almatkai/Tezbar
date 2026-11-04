//! Approval heuristics are prompts, not a shell parser or an OS sandbox.
//! Unknown shell syntax is checked before remembered rules and Turbo shortcuts.
use serde_json::Value;
use std::path::Path;
fn matches(p: &str, s: &str) -> bool {
    regex::Regex::new(p)
        .expect("static policy regex")
        .is_match(s)
}
pub fn complex(s: &str) -> bool {
    s.contains([';', '|', '<', '>', '`', '(', ')', '\r', '\n', '$', '&'])
}
pub fn destructive(s: &str) -> bool {
    matches(r"(?i)\b(rm|srm|unlink|remove-item|rd|rmdir)\b", s)
        || matches(
            r"(?i)\b(DROP\s+(DATABASE|SCHEMA|TABLE)|TRUNCATE(\s+TABLE)?|mkfs(\.[a-z0-9]+)?|fdisk|parted|gdisk|shutdown|reboot|poweroff|halt|init\s+[06])\b",
            s,
        )
        || matches(
            r"(?i)\bdiskutil\s+(eraseDisk|eraseVolume|partitionDisk|zeroDisk|randomDisk|apfs\s+deleteContainer)\b",
            s,
        )
        || matches(
            r"(?i)\bdd\b.*\bof=/dev/|\b(chmod|chown)\b.*(/|~)|:\(\)\s*\{",
            s,
        )
        || matches(
            r"(?i)\b(git\s+(reset|clean)|format(-volume)?|clear-disk|stop-computer|restart-computer)\b",
            s,
        )
}
pub fn auto(s: &str) -> bool {
    let s = s.trim();
    if complex(s) || destructive(s) {
        return false;
    }
    s == "pwd"
        || s == "whoami"
        || matches(r"^git\s+status(\s+[^;&|<>`$()\n]+)*$", s)
        || matches(
            r#"^ls(\s+-[A-Za-z0-9@]+)*(\s+("[^"]+"|'[^']+'|[~./A-Za-z0-9_ -]+))*$"#,
            s,
        )
        || matches(r"^(which\s+|command\s+-v\s+)[-A-Za-z0-9_ .+/]+$", s)
        || matches(r"(?i)^(get-location|get-date|whoami)$", s)
}
pub fn safe_project(s: &str) -> bool {
    !complex(s)
        && !destructive(s)
        && (auto(s)
            || matches(
                r"^(git\s+(status|diff|log|branch|show|remote|tag|rev-parse|describe|stash\s+list)|cargo\s+(check|test|build|clippy|fmt)|go\s+(test|build|vet)|(npm|pnpm|yarn)\s+(test|run\s+[A-Za-z0-9:_-]+|list|outdated|view)|pytest|vitest|jest|tsc|cat|head|tail|wc|echo|printf|date|top|ps|uptime|which|whereis|uname|sw_vers|df|du|free|lsof)\b",
                s,
            ))
}
pub fn shell_asks(command: &str, mode: &str, cfg: &Value, sensitive: bool) -> bool {
    if mode == "review" || sensitive || complex(command) || destructive(command) {
        return true;
    }
    let remembered = cfg["agentAlwaysAllowedExactCommands"]
        .as_array()
        .is_some_and(|a| a.iter().any(|c| c.as_str() == Some(command.trim())))
        || cfg["agentAlwaysAllowedCommands"]
            .as_array()
            .is_some_and(|a| {
                a.iter()
                    .any(|c| c.as_str() == command.split_whitespace().next())
            });
    !(remembered || auto(command) || mode == "turbo" || mode == "auto" && safe_project(command))
}
pub fn sensitive(path: &Path, home: &Path, config: &Path) -> bool {
    path.starts_with(config)
        || [
            ".ssh",
            ".aws",
            ".gnupg",
            ".pi/agent",
            ".local/share/opencode",
            "Library/Keychains",
            "AppData/Roaming/Microsoft/Credentials",
            "AppData/Local/Microsoft/Credentials",
        ]
        .iter()
        .any(|p| path.starts_with(home.join(p)))
        || path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with(".env"))
}
pub fn file_asks(write: bool, outside: bool, sensitive: bool, mode: &str) -> bool {
    if mode == "review" || sensitive {
        return true;
    }
    if write {
        if outside {
            mode != "turbo"
        } else {
            !matches!(mode, "auto" | "turbo")
        }
    } else {
        outside && !matches!(mode, "auto" | "turbo")
    }
}
pub fn broad_search(command: &str, home: &Path) -> bool {
    let search = matches(
        r"(?i)\b(find|mdfind|grep|rg|ag|ack|get-childitem|select-string)\b",
        command,
    );
    search
        && (command.contains('~')
            || command.contains("$HOME")
            || command.contains(&home.to_string_lossy().into_owned()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn risky_shell_always_precedes_remembered_and_turbo() {
        for command in [
            "rm -rf /",
            "echo $(touch a)",
            "cat .env",
            "git reset --hard",
            "ls; touch a",
        ] {
            let cfg = serde_json::json!({"agentAlwaysAllowedExactCommands":[command]});
            assert!(shell_asks(command, "turbo", &cfg, command.contains(".env")));
        }
    }
    #[test]
    fn file_mode_matrix() {
        for mode in ["review", "default", "auto", "turbo"] {
            assert!(file_asks(true, true, true, mode));
            assert_eq!(file_asks(false, false, false, mode), mode == "review");
            assert_eq!(
                file_asks(true, false, false, mode),
                matches!(mode, "review" | "default")
            );
            assert_eq!(file_asks(true, true, false, mode), mode != "turbo");
        }
    }
}
