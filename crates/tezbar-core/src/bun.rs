use std::{path::PathBuf, process::{Command, Stdio}};
pub fn bun_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    }
}

fn bun_in_user_profile() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| home.join(".bun").join("bin").join(bun_executable_name()))
}

pub fn locate_bun(app_local_data: &std::path::Path) -> Result<PathBuf, String> {
    let cached = app_local_data.join("bun").join(bun_executable_name());
    if cached.is_file() {
        return Ok(cached);
    }
    if let Some(user_bun) = bun_in_user_profile().filter(|path| path.is_file()) {
        return Ok(user_bun);
    }
    if Command::new(bun_executable_name())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        return Ok(PathBuf::from(bun_executable_name()));
    }
    Err("Bun runtime is not installed on this system. Tezbar core features and Native V2 extensions run natively in Rust without Bun.".to_string())
}
