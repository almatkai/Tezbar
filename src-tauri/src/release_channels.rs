use semver::Version;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const NORMAL_APP_ID: &str = "com.tezbar.app";
pub const REPOSITORY: &str = "almatkai/Tezbar";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Beta,
    Alpha,
}

impl ReleaseChannel {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }

    pub fn accepts(self, version: &Version) -> bool {
        match version_channel(version) {
            Some(release) => match self {
                Self::Stable => release == Self::Stable,
                Self::Beta => release == Self::Stable || release == Self::Beta,
                Self::Alpha => true,
            },
            None => false,
        }
    }

    pub fn endpoints(self) -> Vec<String> {
        let mut endpoints = vec![format!(
            "https://github.com/{REPOSITORY}/releases/download/channel-{}/latest.json",
            self.name()
        )];
        // The selected channel has its own bootstrap manifest.
        endpoints.push(format!(
            "https://raw.githubusercontent.com/{REPOSITORY}/main/updates/{}.json",
            self.name()
        ));
        // Every channel accepts completed stable releases during bootstrap.
        endpoints.push(format!(
            "https://github.com/{REPOSITORY}/releases/latest/download/latest.json"
        ));
        endpoints
    }
}

pub fn version_channel(version: &Version) -> Option<ReleaseChannel> {
    if !version.build.is_empty() {
        return None;
    }
    if version.pre.is_empty() {
        return Some(ReleaseChannel::Stable);
    }
    let parts: Vec<_> = version.pre.as_str().split('.').collect();
    if parts.len() != 2 || parts[1].parse::<u64>().ok()? == 0 {
        return None;
    }
    match parts[0] {
        "alpha" => Some(ReleaseChannel::Alpha),
        "beta" => Some(ReleaseChannel::Beta),
        _ => None,
    }
}

pub fn default_channel(identifier: &str, version: &Version) -> Result<ReleaseChannel, String> {
    match (identifier, version_channel(version)) {
        (NORMAL_APP_ID, Some(channel)) => Ok(channel),
        _ => Err("The build's release channel does not match its application identity".into()),
    }
}

#[derive(Deserialize, Serialize)]
struct Preference {
    channel: ReleaseChannel,
}

pub fn read_channel(
    directory: &Path,
    identifier: &str,
    version: &Version,
) -> Result<ReleaseChannel, String> {
    let default = default_channel(identifier, version)?;
    let path = directory.join("update-channel.json");
    let channel = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<Preference>(&bytes) {
            Ok(Preference { channel }) => channel,
            _ => ReleaseChannel::Stable, // Unknown preferences fail closed.
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => default,
        Err(error) => return Err(format!("Cannot read update preferences: {error}")),
    };
    // Keep the selected channel across Alpha/Beta promotion to Stable.
    if !path.exists() {
        write_channel(directory, channel)?;
    }
    Ok(channel)
}

pub fn write_channel(directory: &Path, channel: ReleaseChannel) -> Result<(), String> {
    use std::io::Write;
    fs::create_dir_all(directory).map_err(|e| format!("Cannot create update preferences: {e}"))?;
    let path = directory.join("update-channel.json");
    let temporary = directory.join(format!("update-channel.{}.tmp", std::process::id()));
    let result = (|| {
        let bytes = serde_json::to_vec(&Preference { channel }).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|e| format!("Cannot save update channel: {e}"))
}

pub fn validate_manifest_identity(
    channel: ReleaseChannel,
    identifier: &str,
    version: &Version,
    json: &serde_json::Value,
) -> Result<(), String> {
    if !channel.accepts(version) {
        return Err("This release is not eligible for the selected update channel".into());
    }
    let release_channel = version_channel(version).ok_or("Unknown release channel")?;
    match json.get("appIdentifier").and_then(|v| v.as_str()) {
        Some(value) if value == identifier => {}
        None if identifier == NORMAL_APP_ID && release_channel == ReleaseChannel::Stable => {} // Legacy stable manifest.
        _ => return Err("Update belongs to a different application installation".into()),
    }
    match json.get("channel").and_then(|v| v.as_str()) {
        Some(value) if value == release_channel.name() => Ok(()),
        None if release_channel == ReleaseChannel::Stable && identifier == NORMAL_APP_ID => Ok(()),
        _ => Err("Update metadata does not match its release version".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn version(value: &str) -> Version {
        Version::parse(value).unwrap()
    }

    #[test]
    fn release_eligibility_is_not_inferred_from_version_precedence_alone() {
        assert!(ReleaseChannel::Beta.accepts(&version("0.3.0-beta.1")));
        assert!(ReleaseChannel::Beta.accepts(&version("0.3.0")));
        assert!(!ReleaseChannel::Beta.accepts(&version("0.4.0-alpha.1")));
        assert!(!ReleaseChannel::Stable.accepts(&version("0.4.0-beta.1")));
        assert!(ReleaseChannel::Alpha.accepts(&version("0.3.0")));
        assert!(ReleaseChannel::Alpha.accepts(&version("0.3.0-beta.1")));
        assert!(ReleaseChannel::Alpha.accepts(&version("0.3.0-alpha.1")));
        for invalid in ["0.3.0-rc.1", "0.3.0-alpha", "0.3.0-beta.0", "0.3.0+build.2"] {
            assert!(!ReleaseChannel::Beta.accepts(&version(invalid)));
        }
    }

    #[test]
    fn all_channels_share_the_app_identity_and_normal_builds_default_to_stable() {
        assert_eq!(
            default_channel(NORMAL_APP_ID, &version("0.3.0-alpha.1")).unwrap(),
            ReleaseChannel::Alpha
        );
        assert_eq!(
            default_channel(NORMAL_APP_ID, &version("0.2.7")).unwrap(),
            ReleaseChannel::Stable
        );
        assert!(default_channel("com.tezbar.app.alpha", &version("0.3.0-alpha.1")).is_err());
        assert!(ReleaseChannel::Alpha.endpoints()[0].contains("channel-alpha"));
        assert!(ReleaseChannel::Beta
            .endpoints()
            .iter()
            .all(|url| !url.contains("alpha")));
    }

    #[test]
    fn three_way_selection_survives_upgrades_and_corruption_fails_closed() {
        let directory = std::env::temp_dir().join(format!(
            "tezbar-channels-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0-beta.1")).unwrap(),
            ReleaseChannel::Beta
        );
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0")).unwrap(),
            ReleaseChannel::Beta
        );
        write_channel(&directory, ReleaseChannel::Stable).unwrap();
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0-beta.1")).unwrap(),
            ReleaseChannel::Stable
        );
        write_channel(&directory, ReleaseChannel::Alpha).unwrap();
        for release in ["0.3.0-alpha.1", "0.3.0-beta.1", "0.3.0"] {
            assert_eq!(
                read_channel(&directory, NORMAL_APP_ID, &version(release)).unwrap(),
                ReleaseChannel::Alpha
            );
        }
        write_channel(&directory, ReleaseChannel::Beta).unwrap();
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0-alpha.1")).unwrap(),
            ReleaseChannel::Beta
        );
        write_channel(&directory, ReleaseChannel::Stable).unwrap();
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0-alpha.1")).unwrap(),
            ReleaseChannel::Stable
        );
        fs::write(
            directory.join("update-channel.json"),
            br#"{"channel":"nightly"}"#,
        )
        .unwrap();
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0")).unwrap(),
            ReleaseChannel::Stable
        );
        fs::write(directory.join("update-channel.json"), b"broken").unwrap();
        assert_eq!(
            read_channel(&directory, NORMAL_APP_ID, &version("0.3.0")).unwrap(),
            ReleaseChannel::Stable
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_validly_signed_release_still_needs_the_correct_app_identity() {
        let alpha = serde_json::json!({"appIdentifier":NORMAL_APP_ID,"channel":"alpha"});
        assert!(validate_manifest_identity(
            ReleaseChannel::Alpha,
            NORMAL_APP_ID,
            &version("0.3.0-alpha.1"),
            &alpha
        )
        .is_ok());
        assert!(validate_manifest_identity(
            ReleaseChannel::Beta,
            NORMAL_APP_ID,
            &version("0.3.0-beta.1"),
            &alpha
        )
        .is_err());
        let other_app =
            serde_json::json!({"appIdentifier":"com.tezbar.app.alpha","channel":"alpha"});
        assert!(validate_manifest_identity(
            ReleaseChannel::Alpha,
            NORMAL_APP_ID,
            &version("0.3.0-alpha.1"),
            &other_app
        )
        .is_err());
        assert!(validate_manifest_identity(
            ReleaseChannel::Stable,
            NORMAL_APP_ID,
            &version("0.3.0"),
            &serde_json::json!({})
        )
        .is_ok());
    }
}
