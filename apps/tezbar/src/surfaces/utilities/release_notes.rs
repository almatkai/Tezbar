use super::*;
use tezbar_update::notes::{Effect, Notes, State};
#[derive(Default)]
pub struct Release {
    pub model: Notes,
    pub version: Option<String>,
    pub bundled: Option<String>,
    pub current: String,
    pub markdown: Option<Entity<tezbar_ui::markdown::MarkdownView>>,
    task: Option<Task<()>>,
    revision: u64,
}
impl Release {
    pub fn hide(&mut self) {
        self.task = None;
        self.model.close();
    }
}
impl Utilities {
    /// Parent supplies the build's bundled version/date/Markdown before first navigation.
    pub fn configure_release(&mut self, current: String, bundled: Option<String>) {
        self.release.current = current;
        self.release.bundled = bundled;
    }
    pub(super) fn open_release(&mut self, version: Option<String>, cx: &mut Context<Self>) {
        self.release.task = None;
        let version = version
            .or_else(|| self.release.version.clone())
            .unwrap_or_else(|| self.release.current.clone());
        self.release.version = Some(version.clone());
        let effects = self.release.model.open(
            &version,
            &self.release.current,
            self.release.bundled.as_deref(),
        );
        for effect in effects {
            if let Effect::Fetch {
                generation,
                url,
                accept,
            } = effect
            {
                let transport = self.transport.clone();
                let epoch = self.epoch;
                self.release.task = Some(cx.spawn(async move |this, cx| {
                    let result = gpui_tokio::Tokio::spawn(cx, async move {
                        transport.release(url, accept).await
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    let _ = this.update(cx, |this, cx| {
                        if epoch != this.epoch {
                            return;
                        }
                        this.release
                            .model
                            .complete(generation, result.as_deref().map_err(Clone::clone));
                        this.sync_release(cx);
                    });
                }));
            }
        }
        self.sync_release(cx);
    }
    fn sync_release(&mut self, cx: &mut Context<Self>) {
        if let State::Ready(note) = &self.release.model.state {
            self.release.revision += 1;
            let body = note.body.clone();
            let version = note.version.clone();
            let read = !body.trim().is_empty() && version == self.release.current;
            if self.release.markdown.is_none() {
                let view = cx.new(|_| tezbar_ui::markdown::MarkdownView::default());
                self.subscriptions
                    .push(Self::markdown_subscription(&view, cx));
                self.release.markdown = Some(view);
            }
            let markdown = self.release.markdown.as_ref().unwrap();
            markdown.update(cx, |m, cx| {
                m.set_content(self.release.revision, &body, true, cx)
            });
            if read {
                cx.emit(UtilityEffect::ReleaseRead(version));
            }
        }
        cx.notify();
    }
}
