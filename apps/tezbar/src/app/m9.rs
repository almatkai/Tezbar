//! Source fenced M9 host facilities; the existing App owns dialogs and leases.
use super::*;
use crate::surfaces::extension_runtime::ExtensionEffect;
impl Owner {
    pub(super) fn m9_effect(
        &mut self,
        id: EntityId,
        mount: u64,
        e: ExtensionEffect,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.quitting
            || !self
                .launcher
                .update(cx, |v, _, cx| v.extension_source(id, mount, cx))
                .unwrap_or(false)
        {
            if matches!(e, ExtensionEffect::PickFiles { .. }) {
                self.lifecycle.suppress(true);
                drop(events::DialogLease(self.relay.clone()));
            }
            return;
        }
        match e {
            ExtensionEffect::Preferences => self.settings_intent(
                Some(crate::surfaces::command_bar::navigation::Navigation {
                    nonce: 0,
                    surface: tezbar_ui_model::router::Surface::Settings("extensions".into()),
                    payload: crate::surfaces::command_bar::navigation::Payload::Settings {
                        tab: "extensions".into(),
                    },
                }),
                cx,
            ),
            ExtensionEffect::Back => {
                let _ = self.launcher.update(cx, |v, w, cx| v.extension_back(w, cx));
            }
            ExtensionEffect::Close => self.hide(cx),
            ExtensionEffect::Notice(message) => {
                let _ = self.launcher.update(cx, |v, _, cx| {
                    v.error = Some(message);
                    cx.notify();
                });
            }
            ExtensionEffect::PickFiles { ticket, multiple } => {
                self.lifecycle.suppress(true);
                let lease = events::DialogLease(self.relay.clone());
                let prompt = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple,
                    prompt: Some("Choose files".into()),
                });
                self.tasks.push(cx.spawn(async move |this, cx| {
                    let _lease = lease;
                    let result = prompt
                        .await
                        .map_err(|_| "File chooser closed".to_string())
                        .and_then(|r| r.map_err(|_| "File chooser failed".to_string()))
                        .map(|paths| {
                            paths
                                .unwrap_or_default()
                                .into_iter()
                                .map(|p| p.to_string_lossy().into_owned())
                                .collect()
                        });
                    let _ = this.update(cx, |owner, cx| {
                        let _ = owner.launcher.update(cx, |v, w, cx| {
                            v.extension_files(id, mount, ticket, result, w, cx)
                        });
                    });
                }));
            }
        }
    }
}
