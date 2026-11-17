//! Presentation primitives for the native chat, modelled on shadcn/ui AI Elements
//! (Message, Actions, Reasoning, Tool, PromptInput, Suggestion, Confirmation).
//! Icons are Lucide outlines tinted through `text_color`, like the rest of the app.
use gpui::{prelude::*, *};
use std::time::Duration;
use tezbar_ui::tokens::Tokens;

macro_rules! lucide {
    ($($body:literal),+ $(,)?) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
            $($body),+,
            "</svg>"
        )
    };
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Copy,
    Retry,
    Trash,
    Brain,
    Wrench,
    ChevronDown,
    ChevronRight,
    CircleCheck,
    CircleX,
    Loader,
    ArrowUp,
    Stop,
    Paperclip,
    Clipboard,
    Monitor,
    Folder,
    History,
    NewChat,
    Sparkles,
    Shield,
    ShieldAlert,
    Alert,
    Close,
    Search,
    Terminal,
    Image,
}

impl Icon {
    fn data(self) -> &'static str {
        match self {
            Icon::Copy => lucide!(
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/>"#,
                r#"<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
            ),
            Icon::Retry => lucide!(
                r#"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>"#,
                r#"<path d="M21 3v5h-5"/>"#,
                r#"<path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>"#,
                r#"<path d="M8 16H3v5"/>"#
            ),
            Icon::Trash => lucide!(
                r#"<path d="M3 6h18"/>"#,
                r#"<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>"#,
                r#"<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>"#
            ),
            Icon::Brain => lucide!(
                r#"<path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/>"#,
                r#"<path d="M12 5a3 3 0 1 1 5.997.125 4 4 0 0 1 2.526 5.77 4 4 0 0 1-.556 6.588A4 4 0 1 1 12 18Z"/>"#,
                r#"<path d="M15 13a4.5 4.5 0 0 1-3-4 4.5 4.5 0 0 1-3 4"/>"#
            ),
            Icon::Wrench => lucide!(
                r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#
            ),
            Icon::ChevronDown => lucide!(r#"<path d="m6 9 6 6 6-6"/>"#),
            Icon::ChevronRight => lucide!(r#"<path d="m9 18 6-6-6-6"/>"#),
            Icon::CircleCheck => lucide!(
                r#"<circle cx="12" cy="12" r="10"/>"#,
                r#"<path d="m9 12 2 2 4-4"/>"#
            ),
            Icon::CircleX => lucide!(
                r#"<circle cx="12" cy="12" r="10"/>"#,
                r#"<path d="m15 9-6 6"/>"#,
                r#"<path d="m9 9 6 6"/>"#
            ),
            Icon::Loader => lucide!(r#"<path d="M21 12a9 9 0 1 1-6.219-8.56"/>"#),
            Icon::ArrowUp => lucide!(r#"<path d="m5 12 7-7 7 7"/>"#, r#"<path d="M12 19V5"/>"#),
            Icon::Stop => {
                lucide!(r#"<rect x="6" y="6" width="12" height="12" rx="2" fill="black"/>"#)
            }
            Icon::Paperclip => lucide!(
                r#"<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/>"#
            ),
            Icon::Clipboard => lucide!(
                r#"<rect width="8" height="4" x="8" y="2" rx="1" ry="1"/>"#,
                r#"<path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>"#
            ),
            Icon::Monitor => lucide!(
                r#"<rect width="20" height="14" x="2" y="3" rx="2"/>"#,
                r#"<path d="M8 21h8"/>"#,
                r#"<path d="M12 17v4"/>"#
            ),
            Icon::Folder => lucide!(
                r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#
            ),
            Icon::History => lucide!(
                r#"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/>"#,
                r#"<path d="M3 3v5h5"/>"#,
                r#"<path d="M12 7v5l4 2"/>"#
            ),
            Icon::NewChat => lucide!(
                r#"<path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>"#,
                r#"<path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z"/>"#
            ),
            Icon::Sparkles => lucide!(
                r#"<path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/>"#,
                r#"<path d="M20 3v4"/>"#,
                r#"<path d="M22 5h-4"/>"#
            ),
            Icon::Shield => lucide!(
                r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#
            ),
            Icon::ShieldAlert => lucide!(
                r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#,
                r#"<path d="M12 8v4"/>"#,
                r#"<path d="M12 16h.01"/>"#
            ),
            Icon::Alert => lucide!(
                r#"<circle cx="12" cy="12" r="10"/>"#,
                r#"<path d="M12 8v4"/>"#,
                r#"<path d="M12 16h.01"/>"#
            ),
            Icon::Close => lucide!(r#"<path d="M18 6 6 18"/>"#, r#"<path d="m6 6 12 12"/>"#),
            Icon::Search => lucide!(
                r#"<circle cx="11" cy="11" r="8"/>"#,
                r#"<path d="m21 21-4.3-4.3"/>"#
            ),
            Icon::Terminal => lucide!(r#"<path d="m4 17 6-6-6-6"/>"#, r#"<path d="M12 19h8"/>"#),
            Icon::Image => lucide!(
                r#"<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>"#,
                r#"<circle cx="9" cy="9" r="2"/>"#,
                r#"<path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>"#
            ),
        }
    }
}

pub(super) fn icon(icon: Icon, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .data(icon.data().as_bytes())
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
}

/// Lucide `loader-circle`, spinning. Only mounted while work is visibly in flight.
pub(super) fn spinner(id: impl Into<ElementId>, size: f32, color: Hsla) -> impl IntoElement {
    icon(Icon::Loader, size, color).with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

/// shadcn `primary-foreground`: legible on a `text1` (primary) fill in both themes.
pub(super) fn on_primary(t: Tokens) -> Hsla {
    if t.light {
        rgb(0xffffff).into()
    } else {
        rgb(0x18181b).into()
    }
}

/// The raised input/card surface (shadcn `bg-background` + `dark:bg-input/30`).
pub(super) fn surface(t: Tokens) -> Hsla {
    if t.light {
        rgb(0xffffff).into()
    } else {
        hsla(0., 0., 1., 0.035)
    }
}

/// shadcn `Badge variant="secondary"` with a leading status glyph.
pub(super) fn badge(t: Tokens) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4. * t.zoom))
        .h(px(20. * t.zoom))
        .px(px(7. * t.zoom))
        .rounded_full()
        .bg(t.pill_strong)
        .text_size(px(10.5 * t.zoom))
        .line_height(px(14. * t.zoom))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.text2)
}

/// Upper-case section caption used by Tool parameters/results.
pub(super) fn caption(text: impl Into<SharedString>, t: Tokens) -> Div {
    div()
        .text_size(px(10. * t.zoom))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.text3)
        .child(text.into().to_uppercase())
}

/// Monospace, scrollable output block (shadcn `CodeBlock` inside a Tool).
pub(super) fn code_block(id: impl Into<ElementId>, text: String, t: Tokens) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .font_family(tezbar_ui::tokens::mono_family())
        .text_size(px(11.5 * t.zoom))
        .line_height(relative(1.5))
        .text_color(t.text2)
        .rounded(px(7. * t.zoom))
        .bg(t.field)
        .border_1()
        .border_color(t.hairline)
        .px(px(10. * t.zoom))
        .py(px(8. * t.zoom))
        .max_h(px(180. * t.zoom))
        .overflow_y_scroll()
        .child(text)
}

/// Destructive inline alert (shadcn `Alert variant="destructive"`).
pub(super) fn alert(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    t: Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Alert)
        .flex()
        .items_start()
        .gap(px(8. * t.zoom))
        .px(px(10. * t.zoom))
        .py(px(8. * t.zoom))
        .rounded(px(8. * t.zoom))
        .border_1()
        .border_color(t.danger.opacity(0.35))
        .bg(t.danger.opacity(0.08))
        .text_size(px(12. * t.zoom))
        .line_height(relative(1.45))
        .text_color(t.danger)
        .child(
            div()
                .pt(px(1.5 * t.zoom))
                .child(icon(Icon::Alert, 14. * t.zoom, t.danger)),
        )
        .child(div().flex_1().min_w_0().child(text.into()))
}

pub(super) fn duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f32 / 1000.)
    } else {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}
