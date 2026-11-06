//! M7 hooks, separate from launcher input/cache and M6 background-task ownership.
use super::*;
use crate::surfaces::{
    onboarding::{Onboarding, OnboardingEffect},
    settings::SettingsEffect,
};
#[cfg(test)]
pub(super) struct FixtureTransport(pub Arc<dyn RequestAdapter>);
#[cfg(test)]
impl crate::surfaces::settings::SettingsTransport for FixtureTransport {
    fn durable_config(&self) -> crate::surfaces::settings::SettingsFuture {
        self.0.send(Request::LlmConfig)
    }
    fn send(
        &self,
        request: crate::surfaces::settings::SettingsRequest,
    ) -> crate::surfaces::settings::SettingsFuture {
        match request {
            crate::surfaces::settings::SettingsRequest::Save(patch) => {
                self.0.send(Request::LlmSet(patch))
            }
            crate::surfaces::settings::SettingsRequest::Config => self.0.send(Request::LlmConfig),
            request => self.0.send(Request::Settings(request)),
        }
    }
}
impl Launcher {
    pub(crate) fn mount_onboarding(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.onboarding_active {
            self.show_onboarding(w, cx);
            return Ok(());
        }
        let store = self
            .settings_store
            .clone()
            .ok_or("Settings owner is unavailable")?;
        let view =
            cx.new(|cx| Onboarding::new(store, &self.state, cfg!(target_os = "macos"), w, cx));
        self.onboarding_subscription = Some(cx.subscribe(
            &view,
            |this, view, event: &OnboardingEffect, cx| match event {
                OnboardingEffect::Finished => {
                    let _ = this.relay.send(Event::OnboardingFinished(view.entity_id()));
                }
                OnboardingEffect::Settings(effect) => {
                    let origin = view.read(cx).content.entity_id();
                    let _ = this
                        .relay
                        .send(Event::SettingsEffect(origin, effect.clone()));
                }
            },
        ));
        self.onboarding = Some(view.clone());
        self.onboarding_active = true;
        self.dispatch(Input::Hide, cx);
        self.hide_utilities(w, cx);
        self.timer = None;
        self.timer_at = None;
        view.update(cx, |view, cx| view.activate(w, cx));
        self.show_onboarding(w, cx);
        cx.notify();
        Ok(())
    }
    pub(super) fn show_onboarding(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        cx.activate(true);
        w.activate_window();
        if let Some(view) = &self.onboarding {
            view.update(cx, |view, cx| view.set_visible(true, cx));
        }
        cx.notify();
    }
    pub(super) fn hide_onboarding(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.onboarding {
            view.update(cx, |view, cx| view.set_visible(false, cx));
        }
    }
    pub(crate) fn finish_onboarding(
        &mut self,
        id: EntityId,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.onboarding_active
            || self
                .onboarding
                .as_ref()
                .is_none_or(|view| view.entity_id() != id)
        {
            return false;
        }
        self.onboarding_active = false;
        self.hide_onboarding(cx);
        self.input.focus_handle(cx).focus(w, cx);
        if self.navigation.is_none() {
            self.core_ready(cx);
        } else {
            self.show_utilities(w, cx);
        }
        cx.notify();
        true
    }
    pub(crate) fn m7_content(
        &self,
        id: EntityId,
        cx: &App,
    ) -> Option<Entity<crate::surfaces::settings::SettingsContent>> {
        self.onboarding
            .as_ref()
            .map(|view| view.read(cx).content.clone())
            .filter(|content| content.entity_id() == id)
    }
    pub(crate) fn save_settings_ui(
        &mut self,
        effect: &SettingsEffect,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        match effect {
            SettingsEffect::Theme(theme) => self.state.theme = *theme,
            SettingsEffect::RememberTab(tab) => self.state.settings_tab = tab.clone(),
            SettingsEffect::Currency(code) => self.state.currency.pinned = code.clone(),
            SettingsEffect::AutoUpdate(enabled) => self.state.auto_update_enabled = *enabled,
            SettingsEffect::RetryPersistence => {}
            _ => return Task::ready(Err("Unexpected UI preference".into())),
        }
        if self.m7_state_unavailable {
            return Task::ready(Err(
                "Saved launcher preferences could not be loaded; the original file was preserved"
                    .into(),
            ));
        }
        self.persist(cx);
        self.await_settings_ui(cx)
    }
    pub(crate) fn reset_update_check(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        self.state.last_update_check = None;
        if self.m7_state_unavailable {
            return Task::ready(Err(
                "Saved launcher preferences could not be loaded; the original file was preserved"
                    .into(),
            ));
        }
        self.persist(cx);
        self.await_settings_ui(cx)
    }
    fn await_settings_ui(&self, cx: &mut Context<Self>) -> Task<Result<(), String>> {
        let notify = self.m7_state_notify.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let notified = notify.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let state = this
                    .update(cx, |this, _| {
                        let current = this
                            .persist_revision
                            .load(std::sync::atomic::Ordering::SeqCst);
                        (this.m7_persist_done >= current)
                            .then(|| this.state_write_error().map_or(Ok(()), Err))
                    })
                    .map_err(|_| "Launcher preference owner closed".to_string())?;
                if let Some(result) = state {
                    return result;
                }
                notified.await;
            }
        })
    }
    pub(crate) fn route_from_settings(
        &mut self,
        surface: Surface,
        payload: Payload,
        cx: &mut Context<Self>,
    ) {
        self.nonce += 1;
        let intent = Navigation {
            nonce: self.nonce,
            surface,
            payload,
        };
        self.receive_navigation(intent, cx);
    }
}
