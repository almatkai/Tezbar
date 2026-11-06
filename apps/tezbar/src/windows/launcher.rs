//! The shared launcher shell owns input/focus and cancellable requests. M5 supplies all modes.
use crate::{
    core_client::{CoreClient, Generation, SearchRow},
    events::{Event, Relay},
};
use gpui::{prelude::*, *};
use tezbar_ui::{
    controls,
    image_cache::ImageCache,
    text_input::{ClipboardEffect, Committed, FieldOptions, TextInput},
    tokens,
};
pub struct Launcher {
    resize: Option<super::resize::Gesture>,
    pub input: Entity<TextInput>,
    pub rows: Vec<SearchRow>,
    pub ready: bool,
    pub error: Option<String>,
    pub visible: bool,
    pub zoom: f32,
    selected: usize,
    scroll: UniformListScrollHandle,
    generation: Generation,
    search: Option<Task<()>>,
    core: CoreClient,
    relay: Relay,
    images: ImageCache,
    _subscriptions: Vec<Subscription>,
}
impl Launcher {
    pub fn new(core: CoreClient, relay: Relay, w: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut field = TextInput::new(cx);
            field.configure(FieldOptions {
                name: "Search apps, files and commands".into(),
                placeholder: "Search apps, files and commands".into(),
                border: false,
                ..Default::default()
            });
            field
        });
        let committed = cx.subscribe_in(&input, w, |this, _, e: &Committed, _, cx| {
            this.query(e.0.clone(), cx)
        });
        let release = cx.on_release(|this, cx| this.images.release_all_windows(cx));
        let clipboard =
            cx.subscribe_in(&input, w, |_, field, e: &ClipboardEffect, w, cx| match e {
                ClipboardEffect::Read => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                        field.update(cx, |field, cx| field.paste_text(&text, w, cx));
                    }
                }
                ClipboardEffect::Write(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                }
            });
        let queue = relay.clone();
        let activation = cx.observe_window_activation(w, move |this, w, _| {
            if this.visible && !w.is_window_active() {
                let _ = queue.send(Event::Blur(0));
            }
        });
        let appearance = cx.observe_window_appearance(w, |_, _, cx| cx.notify());
        let bounds_relay = relay.clone();
        let bounds = cx.observe_window_bounds(w, move |_, _, _| {
            let _ = bounds_relay.send(Event::Bounds);
        });
        Self {
            resize: None,
            input,
            rows: vec![],
            ready: false,
            error: None,
            visible: false,
            zoom: 1.,
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            generation: Generation::default(),
            search: None,
            core,
            relay,
            images: ImageCache::default(),
            _subscriptions: vec![
                committed, clipboard, activation, appearance, bounds, release,
            ],
        }
    }
    pub fn query(&mut self, text: String, cx: &mut Context<Self>) {
        if !self.ready || !self.visible {
            return;
        }
        let g = self.generation.next();
        let core = self.core.clone();
        self.search = Some(cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { core.search(text).await }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation.accepts(g) {
                    match result {
                        Ok(Ok(rows)) => {
                            this.rows = rows;
                            this.selected = this.selected.min(this.rows.len().saturating_sub(1));
                            this.error = None;
                        }
                        Ok(Err(e)) => this.error = Some(e),
                        Err(e) => this.error = Some(e.to_string()),
                    }
                    cx.notify();
                }
            });
        }));
    }
    pub fn show(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        cx.activate(true);
        w.activate_window();
        self.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
    pub fn hide(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        self.generation.cancel();
        self.search = None;
        self.input.update(cx, |i, cx| i.cancel_composition(w, cx));
        self.images.release(w, cx);
        cx.notify();
    }
}
impl Render for Launcher {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        w.set_rem_size(px(16. * self.zoom));
        let t = tokens::tokens(w);
        let list = uniform_list(
            "launcher-results",
            self.rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                range
                    .map(|index| {
                        let row = &this.rows[index];
                        tezbar_ui::virtual_list::row(
                            row.id.clone().into(),
                            row.title.clone().into(),
                            index,
                            this.rows.len(),
                            index == this.selected,
                            t,
                        )
                        .gap(px(10.))
                        .child(div().flex_1().text_size(px(13.)).child(row.title.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.text3)
                                .child(row.subtitle.clone()),
                        )
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        controls::sheet(t)
            .relative()
            .on_mouse_move(cx.listener(|this, _, w, cx| {
                if let Some(gesture) = this.resize {
                    let _ = gesture.update(w, this.zoom, cx);
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resize = None),
            )
            .on_mouse_up(MouseButton::Left, {
                let relay = self.relay.clone();
                move |_, _, _| {
                    let _ = relay.send(Event::SnapEnd);
                }
            })
            .font_family(tokens::font_family(cx, false))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if this.input.read(cx).marked_range.is_some() {
                    return;
                }
                match e.keystroke.key.as_str() {
                    "escape" => {
                        let _ = this.relay.send(Event::Hide);
                    }
                    "up" | "down" => {
                        this.selected = if e.keystroke.key == "up" {
                            this.selected.saturating_sub(1)
                        } else {
                            (this.selected + 1).min(this.rows.len().saturating_sub(1))
                        };
                        this.scroll
                            .scroll_to_item(this.selected, ScrollStrategy::Nearest);
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.resize = None),
            )
            .child(
                controls::header(t)
                    .child(div().id("drag-handle").w(px(8.)).h_full().on_mouse_down(
                        MouseButton::Left,
                        {
                            let relay = self.relay.clone();
                            move |_, _, _| {
                                let _ = relay.send(Event::SnapBegin);
                            }
                        },
                    ))
                    .child(self.input.clone()),
            )
            .child(div().flex_1().min_h_0().p(px(8.)).child(list))
            .child(
                controls::footer(t)
                    .child(if let Some(error) = &self.error {
                        error.clone()
                    } else if self.ready {
                        "".into()
                    } else {
                        "Starting search…".into()
                    })
                    .when(self.error.is_some(), |d| {
                        let relay = self.relay.clone();
                        d.child(controls::action_button(
                            "retry-core",
                            "Retry",
                            controls::ButtonTone::Quiet,
                            false,
                            t,
                            move |_, _| {
                                let _ = relay.send(Event::RetryCore);
                            },
                        ))
                    })
                    .child(div().flex_1())
                    .child("Open")
                    .child(controls::keycap("↵", t)),
            )
            .children(super::resize::handles(cx, |view, edge, w, _| {
                view.resize = super::resize::Gesture::begin(edge, w)
            }))
    }
}
