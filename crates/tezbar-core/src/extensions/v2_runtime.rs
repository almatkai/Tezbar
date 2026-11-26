//! Metadata-only compatibility names. Execution still requires a verified installed package.
use std::{collections::HashMap, sync::OnceLock};

fn aliases() -> &'static HashMap<String, Option<String>> {
    static ALIASES: OnceLock<HashMap<String, Option<String>>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        let source = super::v2_catalog::source();
        let mut aliases = HashMap::new();
        let mut insert = |id: String, slug: &str| {
            aliases
                .entry(id)
                .and_modify(|owner: &mut Option<String>| {
                    if owner.as_deref() != Some(slug) {
                        *owner = None;
                    }
                })
                .or_insert_with(|| Some(slug.to_owned()));
        };
        for manifest in &source {
            let Some(slug) = manifest["slug"].as_str() else {
                continue;
            };
            for id in [
                slug.to_owned(),
                canonical_id(slug),
                format!("raycast.{slug}"),
                format!("com.tezbar.{slug}"),
            ] {
                insert(id, slug);
            }
            for id in manifest["replaces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
            {
                insert(id.to_owned(), slug);
            }
        }
        // Loved identities can differ from their native slug. Keep this mapping
        // outside release manifests so pinned historical ZIP metadata stays exact.
        let loved: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("loved_extensions.json"))
                .expect("bundled Loved catalog");
        for row in loved {
            let Some(owner) = row["nativePort"]["id"].as_str() else {
                continue;
            };
            let Some(manifest) = source.iter().find(|m| m["id"] == owner) else {
                continue;
            };
            if let (Some(id), Some(slug)) = (row["id"].as_str(), manifest["slug"].as_str()) {
                insert(id.to_owned(), slug);
            }
        }
        aliases
    })
}
pub(super) fn slug(id: &str) -> Option<&str> {
    aliases().get(id).and_then(Option::as_deref)
}
pub(super) fn compatible_ids(slug: &str) -> Vec<String> {
    aliases()
        .iter()
        .filter(|(_, owner)| owner.as_deref() == Some(slug))
        .map(|(id, _)| id.clone())
        .collect()
}
pub(super) fn canonical_id(slug: &str) -> String {
    format!("com.tezbar.native.{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_loved_alias_is_exact_and_preserves_release_identity() {
        assert_eq!(slug("raycast.tezbar-color-picker"), Some("color-picker"));
        assert_eq!(slug("com.tezbar.color-picker"), Some("color-picker"));
        assert_eq!(slug("raycast.color-picker"), Some("color-picker"));
        for bad in [
            "Raycast.tezbar-color-picker",
            "evil.raycast.tezbar-color-picker",
            "raycast.tezbar-color-picker-extra",
            " raycast.tezbar-color-picker",
        ] {
            assert_eq!(slug(bad), None);
        }
        let source = super::super::v2_catalog::source();
        let color = source.iter().find(|m| m["slug"] == "color-picker").unwrap();
        assert_eq!(color["version"], "1.0.1");
        assert_eq!(
            color["artifacts"]["x86_64-pc-windows-msvc"]["sha256"],
            "a0f651442d4d2ebf35398c40919f81050f6bb60366eb2b224bb5cc765ce2ff56"
        );
        assert!(!color["replaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "raycast.tezbar-color-picker"));
    }
}
