//! Compiled-in Native V2 adapter dispatch; no dynamic code or JavaScript execution.
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::Value;

pub(super) fn slug(id: &str) -> Option<&str> {
    let value = id
        .strip_prefix("com.tezbar.native.")
        .or_else(|| id.strip_prefix("raycast."))
        .unwrap_or(id);
    match value {
        "amphetamine" | "audio-device" | "cleanshotx" | "color-picker" | "gif-search"
        | "google-calendar" | "google-search" | "port-manager" | "speedtest" | "timers"
        | "perplexity" => Some(value),
        _ => None,
    }
}
pub(super) fn canonical_id(slug: &str) -> String {
    format!("com.tezbar.native.{slug}")
}
pub(super) fn commands(slug: &str) -> &'static [&'static str] {
    match slug {
        "amphetamine" => super::v2_amphetamine::commands(),
        "audio-device" => super::v2_audio_device::commands(),
        "cleanshotx" => super::v2_cleanshotx::commands(),
        "color-picker" => super::v2_color_picker::commands(),
        "gif-search" => super::v2_gif_search::commands(),
        "google-calendar" => super::v2_google_calendar::commands(),
        "google-search" => super::v2_google_search::commands(),
        "port-manager" => super::v2_port_manager::commands(),
        "speedtest" => super::v2_speedtest::commands(),
        "timers" => super::v2_timers::commands(),
        "perplexity" => super::v2_perplexity::commands(),
        _ => &[],
    }
}
pub(super) fn supports(id: &str, command: &str) -> bool {
    slug(id).is_some_and(|s| commands(s).contains(&command))
}
pub(super) async fn handle(
    ctx: &CoreContext,
    slug: &str,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    match slug {
        "amphetamine" => {
            super::v2_amphetamine::handle(ctx, command, operation, payload, state).await
        }
        "audio-device" => {
            super::v2_audio_device::handle(ctx, command, operation, payload, state).await
        }
        "cleanshotx" => super::v2_cleanshotx::handle(ctx, command, operation, payload, state).await,
        "color-picker" => {
            super::v2_color_picker::handle(ctx, command, operation, payload, state).await
        }
        "gif-search" => super::v2_gif_search::handle(ctx, command, operation, payload, state).await,
        "google-calendar" => {
            super::v2_google_calendar::handle(ctx, command, operation, payload, state).await
        }
        "google-search" => {
            super::v2_google_search::handle(ctx, command, operation, payload, state).await
        }
        "port-manager" => {
            super::v2_port_manager::handle(ctx, command, operation, payload, state).await
        }
        "speedtest" => super::v2_speedtest::handle(ctx, command, operation, payload, state).await,
        "timers" => super::v2_timers::handle(ctx, command, operation, payload, state).await,
        "perplexity" => super::v2_perplexity::handle(ctx, command, operation, payload, state).await,
        _ => Err(format!("No native adapter for {slug}")),
    }
}
