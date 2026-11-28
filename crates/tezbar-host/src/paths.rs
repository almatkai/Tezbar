//! Pure profile resolution. Callers supply environment values; no profile is read here.
use std::path::PathBuf;
#[derive(Clone, Copy)]
pub enum Platform {
    Mac,
    Windows,
}
pub struct Environment {
    pub platform: Platform,
    pub home: PathBuf,
    pub local_data: PathBuf,
    pub cache: PathBuf,
    pub resources: PathBuf,
    pub temp: PathBuf,
    pub data_override: Option<PathBuf>,
    pub config_override: Option<PathBuf>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub resources: PathBuf,
    pub temp: PathBuf,
    pub logs: PathBuf,
}
pub fn resolve(env: Environment, identity: &str) -> Result<Profile, String> {
    if identity.is_empty() || identity.contains(['/', '\\']) || identity == ".." {
        return Err("Invalid application identity".into());
    }
    let data = env.data_override.unwrap_or_else(|| match env.platform {
        Platform::Mac => env.home.join("Library/Application Support").join(identity),
        Platform::Windows => env.local_data.join(identity),
    });
    let logs = match env.platform {
        Platform::Mac => env.home.join("Library/Logs").join(identity),
        Platform::Windows => env.local_data.join(identity).join("logs"),
    };
    Ok(Profile {
        data,
        config: env
            .config_override
            .unwrap_or_else(|| env.home.join(".openray")),
        cache: env.cache.join(identity),
        resources: env.resources,
        temp: env.temp,
        logs,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_paths_match_locked_path_resolver_and_overrides_are_scoped() {
        for platform in [Platform::Mac, Platform::Windows] {
            let p = resolve(
                Environment {
                    platform,
                    home: "/fixture/home".into(),
                    local_data: "/fixture/local".into(),
                    cache: "/fixture/cache".into(),
                    resources: "/fixture/resources".into(),
                    temp: "/fixture/temp".into(),
                    data_override: Some("/fixture/custom-data".into()),
                    config_override: None,
                },
                "com.tezbar.app",
            )
            .unwrap();
            assert_eq!(p.data, PathBuf::from("/fixture/custom-data"));
            assert_eq!(p.config, PathBuf::from("/fixture/home/.openray"));
            assert_eq!(p.cache, PathBuf::from("/fixture/cache/com.tezbar.app"));
            assert_eq!(
                p.logs,
                PathBuf::from(match platform {
                    Platform::Mac => "/fixture/home/Library/Logs/com.tezbar.app",
                    Platform::Windows => "/fixture/local/com.tezbar.app/logs",
                })
            );
        }
    }
}

pub fn config_path(home: Option<PathBuf>, override_dir: Option<PathBuf>) -> Option<PathBuf> {
    override_dir
        .or_else(|| home.map(|home| home.join(".openray")))
        .map(|dir| dir.join("config.json"))
}
