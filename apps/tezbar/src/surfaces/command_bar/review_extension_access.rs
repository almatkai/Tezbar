//! Opus review probe only (test cfg). Read-only/test seams into private Launcher state.
use super::*;
use crate::surfaces::extension_runtime::RuntimeView;
impl Launcher {
    pub(crate) fn review_m9_discard(&self, reply: &Value, cx: &mut Context<Self>) {
        self.dispose_dropped_extension(reply, cx);
    }
    pub(crate) fn review_m9_set_adapter(&mut self, adapter: Arc<dyn RequestAdapter>) {
        self.adapter = adapter;
    }
    pub(crate) fn review_m9_configure_extensions(&mut self, core: CoreClient) {
        self.configure_extensions(core);
    }
    pub(crate) fn review_m9_runtime(&self) -> Option<Entity<RuntimeView>> {
        self.extensions.runtime.clone()
    }
    pub(crate) fn review_m9_surface(&self) -> Option<Surface> {
        self.navigation.as_ref().map(|n| n.surface.clone())
    }
    pub(crate) fn review_m9_runtime_state(
        &self,
        cx: &App,
    ) -> Option<(EntityId, Option<String>, Option<Value>, Option<String>)> {
        self.extensions.runtime.as_ref().map(|r| {
            let v = r.read(cx);
            (
                r.entity_id(),
                v.session.as_ref().map(|s| s.key.session_id.clone()),
                v.session
                    .as_ref()
                    .and_then(|s| s.form.values().get("note").cloned()),
                v.session.as_ref().map(|s| format!("{:?}", s.root.kind)),
            )
        })
    }
}
