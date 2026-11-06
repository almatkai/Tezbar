//! One footer owner shared by launcher and chat. Rendering never starts work.
use super::*;
use crate::surfaces::utilities::{
    UtilityRequest,
    background_tasks::{Background, TaskStatus},
};

#[derive(Clone)]
pub(super) struct OpenTask(pub TaskStatus);
pub(super) struct TaskIndicator {
    adapter: Arc<dyn RequestAdapter>,
    state: Background,
    visible: bool,
    open: bool,
    generation: u64,
    pending: bool,
    dirty: bool,
    request: Option<Task<()>>,
    timer: Option<Task<()>>,
    _events: Option<EventSubscription>,
}
impl EventEmitter<OpenTask> for TaskIndicator {}
impl TaskIndicator {
    pub(super) fn new(adapter: Arc<dyn RequestAdapter>, cx: &mut Context<Self>) -> Self {
        let events = cx.try_global::<CoreEvents>().cloned().map(|hub| {
            hub.subscribe(
                &cx.entity(),
                vec!["knowledge:status".into(), "background-tasks:changed".into()],
                |view, _, cx| view.update(cx, |view, cx| view.refresh(cx)),
                cx,
            )
        });
        Self {
            adapter,
            state: Background::default(),
            visible: false,
            open: false,
            generation: 0,
            pending: false,
            dirty: false,
            request: None,
            timer: None,
            _events: events,
        }
    }
    pub(super) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.generation += 1;
        self.timer = None;
        self.request = None;
        self.pending = false;
        self.dirty = false;
        self.open = false;
        if visible {
            self.refresh(cx);
        }
        cx.notify();
    }
    pub(super) fn set_update(
        &mut self,
        status: &tezbar_update::AppUpdateStatus,
        cx: &mut Context<Self>,
    ) {
        self.state.update = serde_json::to_value(status).ok();
        if self.state.all().is_empty() {
            self.open = false;
        }
        cx.notify();
    }
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.open;
        self.open = false;
        if was_open {
            cx.notify();
        }
        was_open
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        if self.pending {
            self.dirty = true;
            return;
        }
        self.timer = None;
        self.pending = true;
        self.dirty = false;
        let generation = self.generation;
        let adapter = self.adapter.clone();
        self.request = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move {
                adapter
                    .send(Request::Utility(UtilityRequest::BackgroundTasks))
                    .await
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |this, cx| {
                if !this.visible || this.generation != generation {
                    return;
                }
                this.pending = false;
                if let Ok(mut tasks) = result.and_then(|v| {
                    serde_json::from_value::<Vec<TaskStatus>>(v).map_err(|e| e.to_string())
                }) {
                    tasks.truncate(128);
                    this.state.tasks = tasks;
                }
                if this.state.all().is_empty() {
                    this.open = false;
                }
                if this.dirty {
                    this.refresh(cx);
                } else {
                    this.schedule(cx);
                }
                cx.notify();
            });
        }));
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        let delay = if self.state.tasks.is_empty() { 10 } else { 1 };
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(delay))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh(cx));
        }));
    }
    fn open_task(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        if let Some(task) = self.state.all().into_iter().find(|t| t.id == id) {
            self.open = false;
            cx.emit(OpenTask(task));
            cx.notify();
        }
    }
}
fn value(task: &TaskStatus) -> String {
    if matches!(task.kind.as_str(), "indexing" | "update") {
        task.progress
            .filter(|p| p.is_finite())
            .map(|p| format!("{:.0}%", p.clamp(0., 1.) * 100.))
            .unwrap_or_default()
    } else {
        let seconds = task
            .remaining_seconds
            .filter(|s| s.is_finite())
            .unwrap_or(0.)
            .max(0.) as u64;
        if seconds >= 3600 {
            format!(
                "{}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        } else {
            format!("{}:{:02}", seconds / 60, seconds % 60)
        }
    }
}
impl Render for TaskIndicator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(window);
        let tasks = self.state.all();
        let primary = tasks
            .iter()
            .find(|t| t.kind == "update")
            .or_else(|| tasks.iter().find(|t| t.kind == "indexing"))
            .or_else(|| tasks.first());
        let Some(primary) = primary.filter(|_| self.visible) else {
            return div().into_any_element();
        };
        let label = format!("{} {}", primary.title, value(primary));
        let mut root = div().id("task-indicator").relative().flex_shrink_0().child(
            controls::button(
                "background-task-toggle",
                label.clone(),
                controls::ButtonTone::Quiet,
                false,
                t,
            )
            .aria_label(format!("{label}. Show all background tasks"))
            .aria_expanded(self.open)
            .debug_selector(|| "background-task-toggle".into())
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = !this.open;
                cx.notify();
            })),
        );
        if self.open {
            let mut menu = controls::popover("background-task-menu", "Background tasks", t)
                .absolute()
                .bottom(px(32. * t.zoom))
                .right_0()
                .w(px(300. * t.zoom))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.dismiss(cx);
                }));
            for task in tasks {
                let id = task.id.clone();
                menu = menu.child(
                    controls::button(
                        SharedString::from(format!("background-task-{}", task.id)),
                        format!("{} {} · {}", task.title, value(&task), task.detail),
                        controls::ButtonTone::Ghost,
                        false,
                        t,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_task(&id, cx))),
                );
            }
            root = root.child(menu);
        }
        root.into_any_element()
    }
}
impl Launcher {
    pub(super) fn sync_task_indicator(&mut self, cx: &mut Context<Self>) {
        let visible = self.ready
            && self.visible
            && !self.onboarding_active
            && self
                .navigation
                .as_ref()
                .is_none_or(|n| n.surface == Surface::AiChat);
        self.task_indicator
            .update(cx, |indicator, cx| indicator.set_visible(visible, cx));
    }
    pub(super) fn open_background_task(&mut self, task: &TaskStatus, cx: &mut Context<Self>) {
        use crate::surfaces::utilities::UtilityEffect;
        let effect = match task.kind.as_str() {
            "update"
                if matches!(
                    self.update_status,
                    tezbar_update::AppUpdateStatus::Ready { .. }
                ) =>
            {
                UtilityEffect::UpdateAction(UpdateAction::Restart)
            }
            "update" => UtilityEffect::OpenSettings("general".into()),
            "indexing" if task.id.starts_with("semantic-") => {
                UtilityEffect::OpenSettings("knowledge".into())
            }
            "indexing" => UtilityEffect::Navigate(Surface::Indexing, Payload::None),
            _ => {
                let (Some(extension_id), Some(command_name)) =
                    (&task.extension_id, &task.command_name)
                else {
                    return;
                };
                UtilityEffect::OpenExtension {
                    extension_id: extension_id.clone(),
                    command_name: command_name.clone(),
                }
            }
        };
        self.utility_effect(&effect, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    #[derive(Default)]
    struct Fixture {
        calls: AtomicUsize,
        active: AtomicBool,
    }
    impl RequestAdapter for Fixture {
        fn send(
            &self,
            request: Request,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>>
        {
            if request.wire().0 != "background-tasks:list" {
                return Box::pin(async { Ok(json!([])) });
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.load(Ordering::SeqCst);
            Box::pin(async move {
                Ok(if active {
                    json!([{"id":"index","kind":"indexing","title":"Indexing","detail":"Files","progress":0.5}])
                } else {
                    json!([])
                })
            })
        }
    }
    fn runtime(cx: &mut TestAppContext) -> tokio::runtime::Runtime {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        cx.update(|cx| gpui_tokio::init_from_handle(cx, rt.handle().clone()));
        rt
    }
    fn settle(rt: &tokio::runtime::Runtime, cx: &mut VisualTestContext) {
        rt.block_on(async {
            for _ in 0..32 {
                cx.run_until_parked();
                tokio::task::yield_now().await;
            }
        });
    }
    #[gpui::test]
    fn task_footer_polls_only_while_mounted_with_active_and_idle_cadence(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let fixture = Arc::new(Fixture::default());
        let (view, cx) = cx.add_window_view(|_, cx| TaskIndicator::new(fixture.clone(), cx));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        view.update(cx, |v, cx| v.set_visible(true, cx));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
        cx.executor().advance_clock(Duration::from_secs(9));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
        fixture.active.store(true, Ordering::SeqCst);
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
        assert!(cx.debug_bounds("background-task-toggle").is_some());
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 3);
        view.update(cx, |v, cx| v.set_visible(false, cx));
        cx.executor().advance_clock(Duration::from_secs(60));
        settle(&rt, cx);
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 3);
        assert!(cx.debug_bounds("background-task-toggle").is_none());
    }
    #[gpui::test]
    fn ready_update_and_identity_actions_are_available_in_command_footer(cx: &mut TestAppContext) {
        let rt = runtime(cx);
        let adapter = Arc::new(Fixture::default());
        let (relay, mut events) = Relay::new();
        let (view, cx) =
            cx.add_window_view(|w, cx| Launcher::with_adapter(adapter, relay, None, w, cx));
        view.update(cx, |v, cx| {
            v.ready = true;
            v.visible = true;
            v.sync_task_indicator(cx);
            v.core_event(
                &CoreEvent::decode(
                    "update-status",
                    json!({"kind":"ready","version":"0.3.0-beta.8"}),
                ),
                cx,
            );
        });
        settle(&rt, cx);
        assert!(cx.debug_bounds("background-task-toggle").is_some());
        view.update(cx, |v, cx| {
            v.task_indicator
                .update(cx, |i, cx| i.open_task("app-update:ready", cx))
        });
        settle(&rt, cx);
        assert!(matches!(
            rt.block_on(events.recv()),
            Some(Event::Update(UpdateAction::Restart))
        ));
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.return_home(w, cx);
                v.navigate(Surface::Notes, Payload::None, cx);
            })
        });
        settle(&rt, cx);
        assert!(!view.read_with(cx, |v, cx| v.task_indicator.read(cx).visible));
        assert!(cx.debug_bounds("background-task-toggle").is_none());
    }
}
