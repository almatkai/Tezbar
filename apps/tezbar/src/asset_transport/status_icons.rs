//! Native SDK status bitmaps. No OS objects, font database, runtime or client.
//! Symbol paths reuse legacy raycast-api/list-runtime.tsx::SymbolIcon; Gear
//! preserves the previous native status glyph. Unknown symbols are refused,
//! never silently substituted with that gear. Source objects follow the existing
//! runtime's source/dark/light representation (dark first, as in legacy).
use super::*;
use std::sync::Weak;
use tezbar_core::{
    NativeCore,
    host::{HostFuture, StatusIcon},
};
use tezbar_ui_model::extensions::images::resolve;

pub(crate) fn resolve_icon(
    core: Weak<NativeCore>,
    extension: String,
    icon: Value,
) -> HostFuture<StatusIcon> {
    Box::pin(async move {
        let core = core.upgrade().ok_or("Status asset owner closed")?;
        let scope = installed_scope(core.clone(), extension.clone()).await?;
        let raw = source(&icon, 0).ok_or("Unsupported status icon representation")?;
        if icon.get("mask").is_some() || icon.get("fileIcon").is_some() {
            return Err("Status icon masks and fileIcon are unsupported".into());
        }
        let source = resolve(&raw, &scope.extension_root, &scope.allowed_legacy)?;
        let pixels = match source {
            Source::Builtin(name) => {
                let color = tint(icon.get("tintColor"));
                let name = name
                    .strip_prefix("raycast-")
                    .or_else(|| name.strip_prefix("tezbar-"))
                    .ok_or("Unsupported declared status builtin")?
                    .to_lowercase();
                let permit = core.assets.decode_permit().await?;
                tokio::task::spawn_blocking(move || -> Result<StatusIcon, String> {
                    let _permit = permit;
                    let rgb = [color[0], color[1], color[2]];
                    let mut icon = if name == "gear" {
                        gear(rgb)
                    } else {
                        let svg = symbol_svg(&name, rgb)?;
                        let mut bitmap =
                            decode_preview(svg.as_bytes(), 24, ASSET_BYTES, ASSET_BYTES)?;
                        tezbar_ui::image_cache::rgba_to_bgra(bitmap.as_mut()); // BGRA -> straight RGBA
                        let (w, h) = bitmap.dimensions();
                        (bitmap.into_raw(), w, h)
                    };
                    for pixel in icon.0.chunks_exact_mut(4) {
                        pixel[3] = ((u16::from(pixel[3]) * u16::from(color[3]) + 127) / 255) as u8;
                    }
                    Ok(icon)
                })
                .await
                .map_err(|_| "Status icon decoding failed")??
            }
            Source::File(path) => {
                let effect = Effect::ReadFile {
                    token: 0,
                    path: path.clone(),
                    max_bytes: ASSET_BYTES,
                };
                bitmap(
                    load_asset(
                        core.clone(),
                        None,
                        Some(scope.clone()),
                        Source::File(path),
                        effect,
                        24,
                    )
                    .await?,
                )?
            }
            Source::Remote(url) => {
                let effect = Effect::Fetch {
                    token: 0,
                    url: url.clone(),
                    max_bytes: ASSET_BYTES,
                    max_width: 24,
                };
                bitmap(
                    load_asset(
                        core.clone(),
                        None,
                        Some(scope.clone()),
                        Source::Remote(url),
                        effect,
                        24,
                    )
                    .await?,
                )?
            }
            Source::Data(data) => {
                let effect = Effect::DecodeData {
                    token: 0,
                    data: data.clone(),
                    max_bytes: ASSET_BYTES,
                };
                bitmap(
                    load_asset(
                        core.clone(),
                        None,
                        Some(scope.clone()),
                        Source::Data(data),
                        effect,
                        24,
                    )
                    .await?,
                )?
            }
            _ => return Err("Unsupported status icon source (glyph/blob/application)".into()),
        };
        // Every async phase may outlive uninstall/reinstall or registry replacement.
        // Re-authorize by ID, not just by a once-issued pathname capability.
        let current = installed_scope(core, extension).await?;
        if current.extension_root != scope.extension_root
            || current.allowed_legacy != scope.allowed_legacy
        {
            return Err("Installed status icon scope changed".into());
        }
        Ok(pixels)
    })
}
fn bitmap(result: AssetResult) -> Result<StatusIcon, String> {
    let size = result.image.size(0);
    let (w, h) = (u32::from(size.width), u32::from(size.height));
    let mut bytes = result
        .image
        .as_bytes(0)
        .ok_or("Missing status icon pixels")?
        .to_vec();
    tezbar_ui::image_cache::rgba_to_bgra(&mut bytes);
    Ok((bytes, w, h))
}
fn source(icon: &Value, depth: usize) -> Option<String> {
    if depth > 4 || icon.get("mask").is_some() || icon.get("fileIcon").is_some() {
        return None;
    }
    if let Some(raw) = icon.as_str() {
        let raw = raw.strip_prefix("Icon.").unwrap_or(raw);
        return Some(
            if raw.chars().next().is_some_and(char::is_uppercase)
                && raw.chars().all(char::is_alphanumeric)
            {
                format!("raycast-{raw}")
            } else {
                raw.to_owned()
            },
        );
    }
    icon.get("source")
        .or_else(|| icon.get("dark"))
        .or_else(|| icon.get("light"))
        .and_then(|v| source(v, depth + 1))
}
fn tint(value: Option<&Value>) -> [u8; 4] {
    const DEFAULT: [u8; 4] = [128, 128, 128, 255];
    let Some(value) = value else {
        return DEFAULT;
    };
    let raw = value.as_str().or_else(|| {
        value
            .get("dark")
            .or_else(|| value.get("light"))
            .and_then(Value::as_str)
    });
    let Some(raw) = raw.filter(|s| s.len() <= 256) else {
        return DEFAULT;
    };
    if raw.eq_ignore_ascii_case("primarytext") || raw.eq_ignore_ascii_case("secondarytext") {
        return DEFAULT;
    }
    // Reuse the already-locked SVG CSS colour parser. Invalid tints degrade to
    // current/default colour while preserving the supported declared icon.
    raw.parse::<svgtypes::Color>()
        .map(|c| [c.red, c.green, c.blue, c.alpha])
        .unwrap_or(DEFAULT)
}
fn gear(color: [u8; 3]) -> StatusIcon {
    let mut rgba = vec![0; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - 7.5;
            let dy = y as f32 - 7.5;
            let r = (dx * dx + dy * dy).sqrt();
            if r >= 3. && (r <= 5.8 || (r <= 7.3 && (8. * dy.atan2(dx)).cos() > 0.25)) {
                let i = (y * 16 + x) * 4;
                rgba[i..i + 3].copy_from_slice(&color);
                rgba[i + 3] = 255;
            }
        }
    }
    (rgba, 16, 16)
}
fn symbol_svg(name: &str, color: [u8; 3]) -> Result<String, String> {
    // Exact supported identities, not arbitrary names mapped to the legacy catch-all.
    let body = match name {
        "checkmark" | "checkmarkcircle" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="m8 12 2.6 2.6L16.5 9"/>"#
        }
        "xmark" | "xmarkcircle" => r#"<circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/>"#,
        "minus" | "minuscircle" => r#"<circle cx="12" cy="12" r="9"/><path d="M8 12h8"/>"#,
        "exclamationmark" | "exclamationmarkcircle" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v6m0 4h.01"/>"#
        }
        "globe" | "network" | "wifi" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.7 3.7 5.7 3.7 9S14.5 18.3 12 21M12 3c-2.5 2.7-3.7 5.7-3.7 9S9.5 18.3 12 21"/>"#
        }
        "download" => r#"<path d="M12 3v12m0 0 4-4m-4 4-4-4M5 19h14"/>"#,
        "upload" => r#"<path d="M12 21V9m0 0 4 4m-4-4-4 4M5 5h14"/>"#,
        "link" => {
            r#"<path d="M10 13a5 5 0 0 0 7.1 0l1.4-1.4a5 5 0 0 0-7.1-7.1L10.5 5M14 11a5 5 0 0 0-7.1 0l-1.4 1.4a5 5 0 0 0 7.1 7.1l.9-.9"/>"#
        }
        "phone" | "voice" => {
            r#"<path d="M22 16.9v3a2 2 0 0 1-2.2 2A19.8 19.8 0 0 1 2.1 4.2 2 2 0 0 1 4.1 2h3a2 2 0 0 1 2 1.7l.5 2.8a2 2 0 0 1-.6 1.8L7.9 9.4a16 16 0 0 0 6.7 6.7l1.1-1.1a2 2 0 0 1 1.8-.6l2.8.5a2 2 0 0 1 1.7 2Z"/>"#
        }
        "video" | "play" => {
            r#"<rect x="3" y="6" width="18" height="12" rx="2"/><path d="m10 9 5 3-5 3V9Z"/>"#
        }
        "server" | "harddrive" => {
            r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 15h.01M11 15h6"/>"#
        }
        "questionmark" | "questionmarkcircle" | "ping" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.8 2.8 0 0 1 5.2 1.4c0 2-2.7 2.2-2.7 4M12 18h.01"/>"#
        }
        "circlefilled" => r#"<circle cx="12" cy="12" r="9" fill="currentColor"/>"#,
        _ => return Err("Unsupported declared status builtin".into()),
    };
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" color="#{:02x}{:02x}{:02x}" stroke="currentColor" stroke-width="2">{body}</svg>"##,
        color[0], color[1], color[2]
    ))
}

#[cfg(test)]
#[path = "status_icons_tests.rs"]
mod tests;
