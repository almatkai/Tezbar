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
    s.contains([
        ';', '|', '<', '>', '`', '(', ')', '\r', '\n', '$', '&', '{', '}', '*', '?', '[', ']',
    ]) || matches(r"~[^/\\\s]", s) || (!cfg!(windows) && s.contains('\\')) || matches(
        r"(?i)^(?:[^\s]+[/\\])?(?:sudo|doas|env|xargs|eval|exec|source|bash|sh|zsh|cmd(?:\.exe)?|powershell(?:\.exe)?|pwsh|python[0-9.]*|perl|ruby|node|bun|osascript|rundll32|wscript|cscript|invoke-expression|iex|invoke-command)\b",
        s.trim(),
    ) || matches(
        r"(?i)(?:^|\s)(?:-c|-e|--eval|--command|-command|-encodedcommand|--config)(?:[=\s]|$)|\bfind\b.*\s-(?:exec|execdir|ok|okdir)(?:\s|$)",
        s,
    )
}
pub fn destructive(s: &str) -> bool {
    matches(
        r"(?i)\b(rm|srm|unlink|remove-item|rd|rmdir|del|erase|shred|sdelete|truncate|clear-content)\b",
        s,
    ) || matches(
        r"(?i)\b(DROP\s+(DATABASE|SCHEMA|TABLE)|TRUNCATE(\s+TABLE)?|mkfs(\.[a-z0-9]+)?|fdisk|parted|gdisk|shutdown|reboot|poweroff|halt|init\s+[06])\b",
        s,
    ) || matches(
        r"(?i)\bdiskutil\s+(eraseDisk|eraseVolume|partitionDisk|zeroDisk|randomDisk|apfs\s+deleteContainer)\b",
        s,
    ) || matches(
        r"(?i)\bdd\b.*\bof=/dev/|\b(chmod|chown)\b.*(/|~)|:\(\)\s*\{",
        s,
    ) || matches(
        r"(?i)\b(git\s+(reset|clean)|format(-volume)?|clear-disk|stop-computer|restart-computer)\b",
        s,
    ) || matches(
        r"(?i)\bfind\b.*\s-delete(?:\s|$)|\bgit\s+(?:branch\b.*\s-[dD]\b|push\b.*(?:--force|-f)(?:[=\s]|$))",
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
        && !mutation_options(s)
        && (auto(s)
            || matches(
                r"^(git\s+(status|diff|log|branch|show|remote|tag|rev-parse|describe|stash\s+list)|cargo\s+(check|test|build|clippy|fmt)|go\s+(test|build|vet)|(npm|pnpm|yarn)\s+(test|run\s+[A-Za-z0-9:_-]+|list|outdated|view)|pytest|vitest|jest|tsc|cat|head|tail|wc|echo|printf|date|top|ps|uptime|which|whereis|uname|sw_vers|df|du|free|lsof)\b",
                s,
            ))
}
pub fn mutation_options(s: &str) -> bool {
    matches(r"(?i)(?:^|\s)(?:--(?:output|exec-path|git-dir|work-tree|config-env|upload-pack|receive-pack)(?:=|\s|$)|-o(?:\s|=|[^\s])|@|--(?:delete|remove|replace|in-place|write)(?:[=\s]|$))", s)
}
pub fn shell_asks(command: &str, mode: &str, cfg: &Value, sensitive: bool) -> bool {
    if mode == "review" || sensitive || complex(command) || destructive(command) || mutation_options(command) {
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
pub fn within(path: &Path, base: &Path) -> bool {
    #[cfg(windows)]
    {
        fn key(path: &Path) -> String {
            let text = path.to_string_lossy().replace('/', "\\");
            let text = if let Some(t) = text.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{t}")
            } else {
                text.strip_prefix("\\\\?\\").unwrap_or(&text).to_string()
            };
            text.trim_end_matches('\\').to_lowercase()
        }
        let path = key(path);
        let base = key(base);
        path == base
            || path
                .strip_prefix(&base)
                .is_some_and(|rest| rest.starts_with('\\'))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(base)
    }
}
pub fn shell_sensitive(command: &str, cwd: &Path, home: &Path, config: &Path) -> bool {
    // Complex expansion/interpreter syntax is independently reviewed. For simple
    // commands inspect every literal path after resolving aliases and symlinks.
    if sensitive(cwd, home, config) {
        return true;
    }
    let mut token = String::new();
    let mut quote = None;
    let mut tokens = vec![];
    for c in command.chars() {
        if Some(c) == quote {
            quote = None
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c)
        } else if c.is_whitespace() && quote.is_none() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(c)
        }
    }
    if !token.is_empty() {
        tokens.push(token)
    }
    tokens.iter().any(|token| {
        let token = token.split_once('=').map(|(_,v)|v).unwrap_or(token).trim_start_matches('@');
        let path = if token == "~" {
            home.to_path_buf()
        } else if let Some(rest) = token.strip_prefix("~/") {
            home.join(rest)
        } else {
            let path = std::path::PathBuf::from(token);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        let canonical = path.canonicalize().unwrap_or(path);
        sensitive(&canonical, home, config)
    })
}
pub fn sensitive(path: &Path, home: &Path, config: &Path) -> bool {
    within(path, config)
        || [
            ".ssh",
            ".aws",
            ".gnupg",
            ".pi/agent",
            ".local/share/opencode",
            ".local/share/keyrings",
            ".kube",
            ".azure",
            ".config/gcloud",
            ".config/gh",
            ".docker/config.json",
            "Library/Keychains",
            "Library/Accounts",
            "AppData/Roaming/Microsoft/Credentials",
            "AppData/Local/Microsoft/Credentials",
            "AppData/Local/Microsoft/Vault",
            "AppData/Roaming/Microsoft/Protect",
        ]
        .iter()
        .any(|p| within(path, &home.join(p)))
        || path.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .to_lowercase()
                .starts_with(".env")
        })
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
        && (matches(r"(?i)\b(find|rg|grep|get-childitem)\s+/?(?:\s|$)", command)
            || command.contains('~')
            || command.contains("$HOME")
            || command.contains(&home.to_string_lossy().into_owned()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn risky_shell_always_precedes_remembered_and_turbo() {
        for command in [
            "cat ~/.ss?/id_rsa", "head ~/.ss[h]/id_rsa", "cat .e*", "ls ~probeuser/.ssh",
            "git diff --output=/tmp/mutation", "git show --output=/tmp/.env HEAD", "cat -o /tmp/mutation",
            "rm -rf /",
            "echo $(touch a)",
            "cat .env",
            "git reset --hard",
            "ls; touch a",
            "find . -delete",
            "del victim.txt",
            "shred victim.txt",
            "git branch -D feature",
            "git push --force origin main",
            "/bin/bash -c 'touch a'",
            "git -c alias.danger=touch danger",
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
