use crate::release_channels::{version_channel, ReleaseChannel, REPOSITORY};
use semver::Version;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PublishedRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    assets: Vec<InstallerAsset>,
}

#[derive(Debug, Deserialize)]
struct InstallerAsset {
    name: String,
    browser_download_url: String,
    state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualRelease {
    pub version: Version,
    pub notes: String,
    pub release_url: String,
    pub installer_url: String,
}

fn installer_name(version: &Version, os: &str, arch: &str) -> Option<String> {
    match (os, arch) {
        ("macos", "aarch64") => Some(format!("Tezbar_{version}_aarch64.dmg")),
        ("macos", "x86_64") => Some(format!("Tezbar_{version}_x64.dmg")),
        ("windows", "x86_64") => Some(format!("Tezbar_{version}_x64-setup.exe")),
        _ => None,
    }
}

pub fn newest_installer(
    releases: &[PublishedRelease],
    current: &Version,
    channel: ReleaseChannel,
    os: &str,
    arch: &str,
) -> Option<ManualRelease> {
    releases
        .iter()
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            let release_channel = version_channel(&version)?;
            if release.draft
                || release.published_at.is_none()
                || release.tag_name != format!("v{version}")
                || release.prerelease != (release_channel != ReleaseChannel::Stable)
                || version <= *current
                || !channel.accepts(&version)
            {
                return None;
            }
            let name = installer_name(&version, os, arch)?;
            let url =
                format!("https://github.com/{REPOSITORY}/releases/download/v{version}/{name}");
            release.assets.iter().find(|asset| {
                asset.state == "uploaded" && asset.name == name && asset.browser_download_url == url
            })?;
            Some(ManualRelease {
                release_url: format!("https://github.com/{REPOSITORY}/releases/tag/v{version}"),
                installer_url: url,
                notes: release.body.clone().unwrap_or_default(),
                version,
            })
        })
        .max_by(|left, right| left.version.cmp(&right.version))
}

pub async fn check(
    current: &Version,
    channel: ReleaseChannel,
) -> Result<Option<ManualRelease>, String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("Tezbar-App/{current}"))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Cannot check published releases: {e}"))?;
    // GitHub's latest endpoint excludes Beta/Alpha; scan published releases instead.
    let mut releases = Vec::new();
    for page in 1..=5 {
        let batch = client
            .get(format!(
                "https://api.github.com/repos/{REPOSITORY}/releases?per_page=100&page={page}"
            ))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Cannot check published releases: {e}"))?
            .json::<Vec<PublishedRelease>>()
            .await
            .map_err(|e| format!("Cannot read published releases: {e}"))?;
        let complete = batch.len() < 100;
        releases.extend(batch);
        if complete {
            return Ok(newest_installer(
                &releases,
                current,
                channel,
                std::env::consts::OS,
                std::env::consts::ARCH,
            ));
        }
    }
    Err("Release history could not be checked completely. Please open GitHub Releases.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires GitHub network access and a published Beta installer"]
    async fn live_beta_installer_discovery_from_stable_028() {
        let release = check(&Version::parse("0.2.8").unwrap(), ReleaseChannel::Beta)
            .await
            .unwrap()
            .expect("Published Beta installer should be visible from Stable 0.2.8");
        assert!(release.version >= Version::parse("0.2.9-beta.1").unwrap());
        println!(
            "Discovered Beta {}: {}",
            release.version, release.installer_url
        );
    }

    fn release(version: &str) -> PublishedRelease {
        let version = Version::parse(version).unwrap();
        let name = installer_name(&version, "macos", "aarch64").unwrap();
        PublishedRelease {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: !version.pre.is_empty(),
            body: Some("Release notes".into()),
            published_at: Some("2026-10-14T06:32:54Z".into()),
            assets: vec![InstallerAsset {
                browser_download_url: format!(
                    "https://github.com/{REPOSITORY}/releases/download/v{version}/{name}"
                ),
                name,
                state: "uploaded".into(),
            }],
        }
    }

    #[test]
    fn stable_028_on_beta_discovers_the_manual_gemma_beta() {
        let releases = [
            release("0.2.8"),
            release("0.2.9-beta.1"),
            release("0.3.0-alpha.1"),
        ];
        let current = Version::parse("0.2.8").unwrap();
        let beta = newest_installer(
            &releases,
            &current,
            ReleaseChannel::Beta,
            "macos",
            "aarch64",
        )
        .unwrap();
        assert_eq!(beta.version.to_string(), "0.2.9-beta.1");
        assert!(beta
            .installer_url
            .ends_with("Tezbar_0.2.9-beta.1_aarch64.dmg"));
        assert!(newest_installer(
            &releases,
            &current,
            ReleaseChannel::Stable,
            "macos",
            "aarch64"
        )
        .is_none());
        assert_eq!(
            newest_installer(
                &releases,
                &current,
                ReleaseChannel::Alpha,
                "macos",
                "aarch64"
            )
            .unwrap()
            .version
            .to_string(),
            "0.3.0-alpha.1"
        );
    }

    #[test]
    fn does_not_downgrade_or_offer_another_platforms_installer() {
        let releases = [release("0.2.8-beta.1"), release("0.2.9-beta.1")];
        assert!(newest_installer(
            &releases,
            &Version::parse("0.2.9-beta.1").unwrap(),
            ReleaseChannel::Beta,
            "macos",
            "aarch64"
        )
        .is_none());
        assert!(newest_installer(
            &releases,
            &Version::parse("0.2.8").unwrap(),
            ReleaseChannel::Beta,
            "windows",
            "x86_64"
        )
        .is_none());
    }

    #[test]
    fn rejects_drafts_wrong_channels_and_noncanonical_downloads() {
        let current = Version::parse("0.2.8").unwrap();
        for case in 0..5 {
            let mut r = release("0.2.9-beta.1");
            match case {
                0 => r.draft = true,
                1 => r.prerelease = false,
                2 => r.assets[0]
                    .browser_download_url
                    .push_str("?redirect=elsewhere"),
                3 => r.assets[0].state = "new".into(),
                _ => r.published_at = None,
            }
            assert!(
                newest_installer(&[r], &current, ReleaseChannel::Beta, "macos", "aarch64")
                    .is_none()
            );
        }
    }

    #[test]
    fn selects_the_highest_eligible_version_even_when_listing_order_differs() {
        let releases = [
            release("0.2.9-beta.2"),
            release("0.2.9-beta.1"),
            release("0.2.9"),
        ];
        let selected = newest_installer(
            &releases,
            &Version::parse("0.2.8").unwrap(),
            ReleaseChannel::Beta,
            "macos",
            "aarch64",
        )
        .unwrap();
        assert_eq!(selected.version.to_string(), "0.2.9");
    }
}
