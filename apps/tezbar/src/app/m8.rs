//! M8 host effects, routed by the real App owner. No independent App/window/state model.
//! Routed by the retained source identity and emit-time mount.
use super::*;
use crate::{
    asset_transport::chat_image_payload,
    surfaces::{
        ai_chat::ChatEffect,
        command_bar::{
            ChatSource,
            navigation::{Navigation, Payload},
        },
    },
};
use tezbar_ui_model::router::Surface;

impl Owner {
    pub(super) fn m8_effect(
        &mut self,
        source: ChatSource,
        effect: ChatEffect,
        cx: &mut Context<Self>,
    ) {
        let owned = self
            .launcher
            .update(cx, |view, _, cx| view.accepts_chat_source(source, cx))
            .unwrap_or(false);
        if !owned || self.lifecycle.quitting {
            if matches!(effect, ChatEffect::ChooseImages { .. }) {
                self.lifecycle.suppress(true);
                drop(events::DialogLease(self.relay.clone()));
            }
            return;
        }
        match effect {
            ChatEffect::Back => {
                let _ = self
                    .launcher
                    .update(cx, |view, w, cx| view.chat_back(source, w, cx));
            }
            ChatEffect::Settings => self.settings_intent(
                Some(Navigation {
                    nonce: 0,
                    surface: Surface::Settings("ai".into()),
                    payload: Payload::Settings { tab: "ai".into() },
                }),
                cx,
            ),
            ChatEffect::ReviewProject { cwd, nonce } => {
                self.settings_project_review(cwd, nonce, cx)
            }
            ChatEffect::DocumentImage {
                ticket,
                source: raw,
            } => {
                let _ = self.launcher.update(cx, |view, _, cx| {
                    view.chat_document_image(source, ticket, raw, cx)
                });
            }
            ChatEffect::ChooseImages { ticket } => self.m8_choose_images(source, ticket, cx),
            ChatEffect::ClipboardImage { ticket } => {
                // Invoked ONLY by the explicit product effect. Tests inject ClipboardItem;
                // no clipboard read is performed by constructor/render/background polling.
                let read = cx.read_from_clipboard_async();
                let future = Box::pin(async move {
                    let item = read
                        .await
                        .map_err(|_| "Clipboard image is unavailable".to_string())?;
                    clipboard_images(item)
                });
                let _ = self.launcher.update(cx, |view, _, cx| {
                    view.chat_clipboard_job(source, ticket, future, cx)
                });
            }
            ChatEffect::Background(_) => {} // Consumed once by retained Launcher, never relayed.
        }
    }
    fn m8_choose_images(&mut self, source: ChatSource, ticket: u64, cx: &mut Context<Self>) {
        // Relay admission reserves one dialog lease for this exact ChooseImages effect.
        self.lifecycle.suppress(true);
        let lease = events::DialogLease(self.relay.clone());
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose up to four PNG, JPEG or WebP images".into()),
        });
        let client = self.core.clone();
        // Keep the native dialog's lease through completion even if chat is hidden.
        // The result is then admitted into the Launcher's cancellable image lane.
        self.tasks.push(cx.spawn(async move |this, cx| {
            let _lease = lease;
            let paths = prompt
                .await
                .map_err(|_| "Image chooser closed".to_string())
                .and_then(|result| result.map_err(|_| "Image chooser failed".to_string()))
                .map(|paths| paths.unwrap_or_default());
            let future = match paths {
                Ok(paths) => client.chosen_chat_images(paths),
                Err(error) => Box::pin(async { Err(error) }),
            };
            let _ = this.update(cx, |owner, cx| {
                let _ = owner.launcher.update(cx, |view, _, cx| {
                    view.chat_image_job(source, ticket, future, cx)
                });
            });
        }));
    }
    /// Called at the START of a real quit, before draining shared Settings, launcher
    /// state and layouts, and before CoreClient::shutdown. It never stops the core.
    pub(super) fn m8_prepare_quit(&mut self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        self.launcher
            .update(cx, |view, w, cx| view.prepare_chat_quit(w, cx))
            .unwrap_or_else(|_| Task::ready(Err("Chat window closed during save".into())))
    }
}

/// Pure conversion seam for the real GPUI clipboard and headless host fixture.
fn clipboard_images(item: Option<ClipboardItem>) -> Result<Vec<serde_json::Value>, String> {
    let item = item.ok_or("Clipboard contains no image")?;
    let mut result = Vec::new();
    for entry in item.entries {
        if let ClipboardEntry::Image(image) = entry {
            if result.len() == 4 {
                return Err("Paste at most four images".into());
            }
            result.push(chat_image_payload(
                image.bytes,
                Some("Clipboard image".into()),
            )?);
        }
    }
    if result.is_empty() {
        return Err("Clipboard contains no PNG, JPEG or WebP image".into());
    }
    Ok(result)
}

#[cfg(test)]
#[path = "m8_tests.rs"]
mod tests;
