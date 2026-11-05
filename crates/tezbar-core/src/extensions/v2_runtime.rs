//! Metadata-only compatibility names. No extension implementation is linked here.
pub(super) fn slug(id: &str) -> Option<&str> {
    let value = id
        .strip_prefix("com.tezbar.native.")
        .or_else(|| id.strip_prefix("raycast."))
        .unwrap_or(id);
    super::v2_catalog::source()
        .iter()
        .any(|m| m["slug"] == value)
        .then_some(value)
}
pub(super) fn canonical_id(slug: &str) -> String {
    format!("com.tezbar.native.{slug}")
}
