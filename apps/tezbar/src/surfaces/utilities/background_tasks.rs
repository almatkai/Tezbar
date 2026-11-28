use super::*;
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub progress: Option<f64>,
    pub remaining_seconds: Option<f64>,
    pub extension_id: Option<String>,
    pub command_name: Option<String>,
}
#[derive(Default)]
pub struct Background {
    pub tasks: Vec<TaskStatus>,
    pub knowledge: Option<Value>,
    pub index_summary: String,
    pub index_progress: f32,
    pub update: Option<Value>,
}
impl Utilities {
    pub(super) fn task_action(&mut self, index: usize, cx: &mut Context<Self>) {
        let tasks = self.background.all();
        let Some(task) = tasks.get(index) else {
            return;
        };
        match task.kind.as_str() {
            "indexing" if task.id.starts_with("semantic-") => {
                cx.emit(UtilityEffect::OpenSettings("knowledge".into()))
            }
            "indexing" => cx.emit(UtilityEffect::Navigate(Surface::Indexing, Payload::None)),
            "update" => {
                if self
                    .background
                    .update
                    .as_ref()
                    .is_some_and(|u| u["kind"] == "ready")
                {
                    cx.emit(UtilityEffect::UpdateAction(
                        super::super::command_bar::navigation::UpdateAction::Restart,
                    ));
                } else {
                    cx.emit(UtilityEffect::OpenSettings("general".into()));
                }
            }
            _ => {
                if let (Some(extension_id), Some(command_name)) =
                    (&task.extension_id, &task.command_name)
                {
                    cx.emit(UtilityEffect::OpenExtension {
                        extension_id: extension_id.clone(),
                        command_name: command_name.clone(),
                    });
                }
            }
        }
    }
}
impl Background {
    pub fn all(&self) -> Vec<TaskStatus> {
        let mut tasks = self.tasks.clone();
        if let Some(update) = &self.update {
            let kind = update["kind"].as_str().unwrap_or("");
            if matches!(kind, "downloading" | "ready") {
                let total = update["total"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n > 0.);
                let downloaded = update["downloaded"]
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .unwrap_or(0.)
                    .max(0.);
                tasks.insert(
                    0,
                    TaskStatus {
                        id: format!("app-update:{kind}"),
                        kind: "update".into(),
                        title: if kind == "ready" {
                            "Update ready"
                        } else {
                            "Updating…"
                        }
                        .into(),
                        detail: format!(
                            "v{} · {}",
                            update["version"].as_str().unwrap_or(""),
                            if kind == "ready" {
                                "Restart to apply".into()
                            } else {
                                format!("{:.1} MB downloaded", downloaded / 1024. / 1024.)
                            }
                        ),
                        progress: if kind == "ready" {
                            Some(1.)
                        } else {
                            total.map(|n| (downloaded / n).clamp(0., 1.))
                        },
                        remaining_seconds: None,
                        extension_id: None,
                        command_name: None,
                    },
                );
            }
        }
        tasks
    }
}
impl Utilities {
    pub fn set_update_status(
        &mut self,
        status: tezbar_update::AppUpdateStatus,
        cx: &mut Context<Self>,
    ) {
        self.background.update = serde_json::to_value(status).ok();
        cx.notify();
    }
}
