use super::*;
impl Utilities {
    pub(super) fn monitor_action(&mut self, a: Action, cx: &mut Context<Self>) {
        match a {
            Action::MonitorTab(tab) => self.monitor_tab = tab,
            Action::AutoRefresh(enabled) => self.monitor.set_auto_refresh(enabled, self.now()),
            Action::Copy => {
                if let Some(v) = &self.monitor.value {
                    self.request(
                        UtilityRequest::WriteText(v.to_string()),
                        Completion::Simple,
                        false,
                        cx,
                    );
                }
            }
            _ => {}
        }
    }
    pub(super) fn monitor_rows(&self) -> Vec<(String, String)> {
        let query = self.monitor.query.to_lowercase();
        self.monitor_display
            .iter()
            .filter(|r| self.monitor_tab == "overview" || r.section == self.monitor_tab)
            .map(|r| {
                (
                    r.id.clone(),
                    format!("{} · {}: {}", r.section, r.label, r.value),
                )
            })
            .filter(|(_, label)| label.to_lowercase().contains(&query))
            .collect()
    }
}
// TS reachability: SystemMonitorView has snapshot tabs/manual refresh/auto refresh, no kill API.
