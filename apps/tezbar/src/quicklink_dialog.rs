use gpui::{prelude::*, *};
use serde_json::{json, Value};
use tezbar_core::host::DialogReply;
use tezbar_ui::text_input::{Committed, FieldOptions, TextInput};

pub struct Editor {
    fields: Vec<Entity<TextInput>>,
    values: Vec<String>,
    reply: Option<tokio::sync::oneshot::Sender<Result<DialogReply, String>>>,
    _lease: crate::events::DialogLease,
    _subscriptions: Vec<Subscription>,
    error: String,
    _watch: Task<()>,
}
impl Editor {
    pub fn new(
        value: Value,
        reply: tokio::sync::oneshot::Sender<Result<DialogReply, String>>,
        lease: crate::events::DialogLease,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let values = vec![
            value["name"].as_str().unwrap_or("Quick Link").to_owned(),
            value["link"].as_str().unwrap_or("").to_owned(),
        ];
        let mut fields = vec![];
        let mut subscriptions = vec![];
        for (i, title) in ["Name", "Link"].iter().enumerate() {
            let field = cx.new(|cx| {
                let mut f = TextInput::new(cx);
                f.configure(FieldOptions {
                    name: (*title).into(),
                    placeholder: (*title).into(),
                    ..Default::default()
                });
                f.set_text(&values[i], w, cx);
                f
            });
            subscriptions.push(cx.subscribe(&field, move |this, _, event: &Committed, cx| {
                this.values[i] = event.0.clone();
                cx.notify();
            }));
            fields.push(field);
        }
        let watch = cx.spawn_in(w, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let closed = this.update_in(cx, |this, w, _| {
                if this.reply.as_ref().is_none_or(|r| r.is_closed()) {
                    w.remove_window();
                    true
                } else {
                    false
                }
            });
            if closed.unwrap_or(true) {
                break;
            }
        });
        Self {
            fields,
            values,
            reply: Some(reply),
            _lease: lease,
            _subscriptions: subscriptions,
            error: String::new(),
            _watch: watch,
        }
    }
    fn finish(&mut self, save: bool, w: &mut Window, cx: &mut Context<Self>) {
        self.values = self
            .fields
            .iter()
            .map(|f| f.read(cx).content.to_string())
            .collect();
        let value = json!({"name":self.values[0],"link":self.values[1]});
        if save {
            if let Err(e) = tezbar_core::launcher::stores::validate_quicklink(&value) {
                self.error = e;
                cx.notify();
                return;
            }
        }
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(Ok(DialogReply::QuickLink(save.then_some(value))));
        }
        w.remove_window();
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(Ok(DialogReply::QuickLink(None)));
        }
    }
}
impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .bg(rgb(0x202020))
            .text_color(rgb(0xffffff))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "escape" {
                    this.finish(false, w, cx);
                }
            }))
            .child("Create Quick Link")
            .child("Name")
            .child(self.fields[0].clone())
            .child("Link")
            .child(self.fields[1].clone())
            .child(self.error.clone())
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .id("quicklink-cancel")
                            .cursor_pointer()
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, w, cx| this.finish(false, w, cx))),
                    )
                    .child(
                        div()
                            .id("quicklink-save")
                            .cursor_pointer()
                            .child("Save")
                            .on_click(cx.listener(|this, _, w, cx| this.finish(true, w, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[gpui::test]
    fn editor_prefill_and_cancel(cx: &mut TestAppContext) {
        let (reply, mut receiver) = tokio::sync::oneshot::channel();
        let (relay, _) = crate::events::Relay::new();
        let (view, cx) = cx.add_window_view(|w, cx| {
            Editor::new(
                json!({"link":"pssword"}),
                reply,
                crate::events::DialogLease(relay),
                w,
                cx,
            )
        });
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                assert_eq!(v.fields[1].read(cx).content.as_ref(), "pssword");
                v.finish(false, w, cx);
            })
        });
        assert_eq!(
            receiver.try_recv().unwrap().unwrap(),
            DialogReply::QuickLink(None)
        );
    }
    #[gpui::test]
    fn editor_save_uses_edited_fields(cx: &mut TestAppContext) {
        let (reply, mut receiver) = tokio::sync::oneshot::channel();
        let (relay, _) = crate::events::Relay::new();
        let (view, cx) = cx.add_window_view(|w, cx| {
            Editor::new(
                json!({"link":"pssword"}),
                reply,
                crate::events::DialogLease(relay),
                w,
                cx,
            )
        });
        cx.update(|w, cx| {
            view.update(cx, |v, cx| {
                v.fields[0].update(cx, |f, cx| f.set_text("Edited name", w, cx));
                v.fields[1].update(cx, |f, cx| f.set_text("edited-link", w, cx));
                v.finish(true, w, cx);
            })
        });
        assert_eq!(
            receiver.try_recv().unwrap().unwrap(),
            DialogReply::QuickLink(Some(json!({"name":"Edited name","link":"edited-link"})))
        );
    }
}
