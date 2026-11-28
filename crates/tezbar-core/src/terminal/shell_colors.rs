//! Color the stock zsh prompt after normal startup, without editing user dotfiles.
use portable_pty::CommandBuilder;
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

const STARTUP: &str = include_str!("zsh-colors.zsh");

pub(super) fn configure(command: &mut CommandBuilder, shell: &str, config: &Path) {
    if command.get_env("NO_COLOR").is_some() {
        return;
    }
    if Path::new(shell)
        .file_name()
        .is_none_or(|name| name != "zsh")
    {
        return;
    }
    match install(config) {
        Ok(dir) => {
            let original = command.get_env("ZDOTDIR").map(|value| value.to_owned());
            command.env(
                "TEZBAR_USER_ZDOTDIR_SET",
                if original.is_some() { "1" } else { "0" },
            );
            command.env("TEZBAR_USER_ZDOTDIR", original.unwrap_or_default());
            command.env("ZDOTDIR", dir);
        }
        // Color is optional; a read-only profile must not prevent opening a shell.
        Err(error) => log::warn!("Could not prepare terminal prompt colors: {error}"),
    }
}

fn install(config: &Path) -> std::io::Result<std::path::PathBuf> {
    let dir = config.join("terminal-shell/zsh-v1");
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let path = dir.join(".zshenv");
    if fs::read(&path).ok().as_deref() != Some(STARTUP.as_bytes()) {
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(STARTUP.as_bytes())?;
        file.persist(&path).map_err(|error| error.error)?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn respects_no_color_and_explicit_listing_colors() {
        let root = tempfile::tempdir().unwrap();
        let mut command = CommandBuilder::new("/bin/zsh");
        command.env("NO_COLOR", "1");
        command.env("ZDOTDIR", "/original");
        configure(&mut command, "/bin/zsh", root.path());
        assert_eq!(
            command.get_env("ZDOTDIR"),
            Some(std::ffi::OsStr::new("/original"))
        );
        assert!(!root.path().join("terminal-shell").exists());

        command.env_remove("NO_COLOR");
        command.env("CLICOLOR", "0");
        configure(&mut command, "/bin/zsh", root.path());
        assert_eq!(command.get_env("CLICOLOR"), Some(std::ffi::OsStr::new("0")));
        assert_eq!(
            command.get_env("TEZBAR_USER_ZDOTDIR"),
            Some(std::ffi::OsStr::new("/original"))
        );
        assert!(root.path().join("terminal-shell/zsh-v1/.zshenv").is_file());

        let mut plain = CommandBuilder::new("/bin/zsh");
        plain.env_remove("NO_COLOR");
        plain.env_remove("CLICOLOR");
        configure(&mut plain, "/bin/zsh", root.path());
        assert_eq!(plain.get_env("CLICOLOR"), None);
    }

    #[cfg(target_os = "macos")]
    fn startup(custom: &str, alternate_dotdir: bool) -> String {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let dots = if alternate_dotdir {
            root.path().join("dot files")
        } else {
            home.clone()
        };
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&dots).unwrap();
        for (name, code) in [
            (".zshenv", "typeset -ga startup_trace=(env)"),
            (".zprofile", "startup_trace+=(profile)"),
            (".zshrc", "startup_trace+=(rc)"),
            (".zlogin", "startup_trace+=(login)"),
        ] {
            fs::write(dots.join(name), format!("{code}\n{custom}\n")).unwrap();
        }
        let integration = install(&root.path().join("config")).unwrap();
        let output = Command::new("/bin/zsh")
            .args(["-lic", "for hook in $precmd_functions; do $hook; done; print -P -- $PROMPT; print -r -- TRACE:$startup_trace; print -r -- DOTDIR:${ZDOTDIR-unset}; print -r -- HOOKS:$precmd_functions; print -r -- PRIVATE:${TEZBAR_USER_ZDOTDIR_SET-unset}"])
            .env_clear().env("PATH", "/usr/bin:/bin").env("HOME", &home)
            .env("TERM", "xterm-256color").env("TERM_PROGRAM", "Tezbar")
            .env("ZDOTDIR", integration)
            .env("TEZBAR_USER_ZDOTDIR_SET", if alternate_dotdir { "1" } else { "0" })
            .env("TEZBAR_USER_ZDOTDIR", if alternate_dotdir { dots.as_os_str() } else { std::ffi::OsStr::new("") })
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(output.contains("TRACE:env profile rc login"), "{output:?}");
        assert!(output.contains("PRIVATE:unset"), "{output:?}");
        assert!(!output.contains("HOOKS:_tezbar"), "{output:?}");
        if alternate_dotdir {
            assert!(
                output.contains(&format!("DOTDIR:{}", dots.display())),
                "{output:?}"
            );
        } else {
            assert!(output.contains("DOTDIR:unset"), "{output:?}");
        }
        output
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn stock_prompt_has_real_ansi_colors_after_normal_login_startup() {
        let output = startup("", false);
        assert!(output.contains("\x1b[33m"), "{output:?}");
        assert!(!output.contains("\x1b[32m"), "{output:?}");
        assert!(!output.contains("\x1b[36m"), "{output:?}");
        assert!(!output.contains("\x1b[34m"), "{output:?}");
        assert!(output.contains("\x1b[39m"), "{output:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn custom_prompt_and_alternate_zdotdir_are_preserved() {
        let output = startup("PROMPT='MY CUSTOM PROMPT> '", true);
        assert!(output.starts_with("MY CUSTOM PROMPT> \n"), "{output:?}");
        assert!(!output.contains('\x1b'), "{output:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn no_color_set_by_shell_startup_keeps_stock_prompt_plain() {
        let output = startup("export NO_COLOR=1", false);
        assert!(!output.contains('\x1b'), "{output:?}");
    }
}
