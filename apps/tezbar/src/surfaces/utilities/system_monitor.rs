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
        let Some(v) = &self.monitor.value else {
            return vec![];
        };
        let q = self.monitor.query.to_lowercase();
        let keys = if self.monitor_tab == "overview" {
            vec![
                "device",
                "cpu",
                "gpu",
                "memory",
                "storage",
                "battery",
                "network",
                "fans",
                "processes",
            ]
        } else {
            vec![self.monitor_tab.as_str()]
        };
        keys.into_iter()
            .flat_map(|section| {
                let values = v[section]
                    .as_object()
                    .map(|o| {
                        o.iter()
                            .map(|(k, v)| {
                                (
                                    format!("{section}.{k}"),
                                    format!(
                                        "{section} · {k}: {}",
                                        v.as_str()
                                            .map(String::from)
                                            .unwrap_or_else(|| v.to_string())
                                    ),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![(section.into(), format!("{section}: Unavailable"))]);
                values
            })
            .filter(|(_, label)| label.to_lowercase().contains(&q))
            .collect()
    }
}
// TS reachability: SystemMonitorView has snapshot tabs/manual refresh/auto refresh, no kill API.
