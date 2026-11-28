use super::*;
impl Utilities {
    pub(super) fn permission_action(&mut self, a: Action, cx: &mut Context<Self>) {
        match a {
            Action::Permission(id) => {
                if let Some(model::PermissionEffect::Request { generation, id }) =
                    self.permissions.request(id)
                {
                    self.request(
                        UtilityRequest::Permission(id),
                        Completion::Permission(generation),
                        true,
                        cx,
                    );
                }
            }
            Action::PermissionSettings(url) => {
                if url.starts_with("x-apple.systempreferences:") || url.starts_with("ms-settings:")
                {
                    self.request(UtilityRequest::Open(url), Completion::Simple, false, cx);
                }
            }
            _ => {}
        }
    }
}
