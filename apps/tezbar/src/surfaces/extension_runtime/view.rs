use super::*;
use tezbar_ui::{
    controls::{self, ButtonTone},
    tokens::{self, Tokens},
};
#[derive(Clone)]
pub(super) struct Row {
    pub id: String,
    pub node: Node,
    pub section: Option<String>,
}
pub(super) fn rows(s: &RuntimeSession) -> Vec<Row> {
    let ids = s.filtered_ids();
    let mut all = vec![];
    s.root.visit(&mut all);
    let indices: BTreeMap<usize, usize> = all
        .iter()
        .enumerate()
        .map(|(i, n)| (*n as *const Node as usize, i))
        .collect();
    fn walk(
        n: &Node,
        section: Option<String>,
        indices: &BTreeMap<usize, usize>,
        ids: &[String],
        out: &mut Vec<Row>,
    ) {
        let section = if matches!(
            n.kind,
            NodeKind::ListSection
                | NodeKind::GridSection
                | NodeKind::MenuSection
                | NodeKind::MenuSubmenu
        ) {
            Some(
                [
                    n.string("title").unwrap_or(""),
                    n.string("subtitle").unwrap_or(""),
                ]
                .join(" ")
                .trim()
                .to_owned(),
            )
        } else {
            section
        };
        if n.kind.row() {
            let id = n
                .string("id")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("row:{}", indices[&(n as *const Node as usize)]));
            if ids.contains(&id) {
                out.push(Row {
                    id,
                    node: n.clone(),
                    section,
                });
            }
            return;
        }
        for child in &n.children {
            walk(child, section.clone(), indices, ids, out);
        }
    }
    let mut out = vec![];
    walk(&s.root, None, &indices, &ids, &mut out);
    out
}
pub(super) fn item_detail(n: &Node) -> Option<Node> {
    n.children
        .iter()
        .find(|n| n.kind == NodeKind::ListItemDetail)
        .cloned()
        .or_else(|| {
            n.props
                .get("detail")
                .and_then(|v| serde_json::from_value::<Node>(v.clone()).ok())
                .filter(|n| n.kind == NodeKind::ListItemDetail && n.validate().is_ok())
        })
}
pub(super) fn document_id(n: &Node) -> String {
    let text = n.string("markdown").unwrap_or("");
    format!(
        "markdown:{}",
        text.bytes()
            .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
    )
}
pub(super) fn documents(root: &Node) -> BTreeMap<String, String> {
    let mut nodes = vec![];
    root.visit(&mut nodes);
    let mut out = BTreeMap::new();
    for node in nodes {
        if let Some(text) = node.string("markdown") {
            out.insert(document_id(node), text.to_owned());
        }
        if let Some(detail) = item_detail(node) {
            if let Some(text) = detail.string("markdown") {
                out.insert(document_id(&detail), text.to_owned());
            }
        }
    }
    out
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(v) => v.clone(),
        Value::Number(_) | Value::Bool(_) => v.to_string(),
        Value::Array(v) => v.iter().map(scalar).collect::<Vec<_>>().join(", "),
        _ => v
            .get("text")
            .or_else(|| v.get("value"))
            .map(scalar)
            .unwrap_or_default(),
    }
}
fn source(v: &Value, light: bool) -> Option<String> {
    v.as_str()
        .map(|raw| {
            if raw.chars().next().is_some_and(char::is_uppercase)
                && raw.chars().all(char::is_alphanumeric)
            {
                format!("raycast-{raw}")
            } else {
                raw.to_owned()
            }
        })
        .or_else(|| v.get("source").and_then(|v| source(v, light)))
        .or_else(|| {
            v.get(if light { "light" } else { "dark" })
                .or_else(|| v.get(if light { "dark" } else { "light" }))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
}
pub(super) fn node_images(
    n: &Node,
    id: &str,
    light: bool,
) -> Vec<(String, String, tezbar_ui_model::extensions::images::Variant)> {
    use tezbar_ui_model::extensions::images::Variant;
    let mut out = vec![];
    for (field, variant) in [("icon", Variant::Icon), ("content", Variant::Artwork)] {
        if let Some(v) = n.props.get(field) {
            let v = if field == "content" {
                v.get("value").unwrap_or(v)
            } else {
                v
            };
            if let Some(raw) = source(v, light) {
                out.push((format!("{id}:{field}"), raw, variant));
            }
        }
    }
    out
}
pub(super) fn shows_search(n: &Node) -> bool {
    fn has_rows(n: &Node) -> bool {
        n.kind.row() || n.children.iter().any(has_rows)
    }
    n.kind == NodeKind::Grid
        || n.kind == NodeKind::List
            && (n.props.contains_key("searchBarPlaceholder")
                || n.props.contains_key("searchText")
                || n.props.contains_key("searchBarAccessory")
                || n.props.get("__hasServerSearch") == Some(&Value::Bool(true))
                || has_rows(n)
                || n.string("navigationTitle").is_none())
}
pub(super) fn field_id(n: &Node, index: usize) -> String {
    n.string("id")
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{}:{index}",
                serde_json::to_value(n.kind).unwrap().as_str().unwrap()
            )
        })
}

pub(super) fn form_label<'a>(n: &'a Node, id: &'a str) -> &'a str {
    n.string("title").unwrap_or_else(|| {
        if matches!(n.kind, NodeKind::Description | NodeKind::FormSeparator) { "" } else { id }
    })
}

pub(super) fn selected_option_title<'a>(options: &[&'a Node], value: &Value) -> Option<&'a str> {
    options.iter().find(|n| matches!(n.kind, NodeKind::DropdownItem | NodeKind::TagPickerItem)
        && n.props.get("value") == Some(value)).and_then(|n| n.string("title"))
}

impl RuntimeView {
    fn button(
        &self,
        id: String,
        label: String,
        event: UiEvent,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let owner = cx.entity().downgrade();
        controls::action_button(id, label, ButtonTone::Quiet, false, t, move |w, cx| {
            let _ = owner.update(cx, |v, cx| v.event(event.clone(), w, cx));
        })
    }
    fn text_field(&self, id: &str) -> AnyElement {
        self.fields
            .get(id)
            .map(|e| {
                div()
                    .min_h(px(32.))
                    .child(e.input.clone())
                    .into_any_element()
            })
            .unwrap_or_else(|| div().child("Field unavailable").into_any_element())
    }
    fn image(&self, key: &str, raw: Option<&Value>, t: Tokens, size: f32) -> AnyElement {
        if let Some(image) = self.image_state.get(key) {
            return img(image)
                .size(px(size * t.zoom))
                .object_fit(ObjectFit::Contain)
                .into_any_element();
        }
        let tint = raw
            .and_then(|v| v.get("tintColor"))
            .and_then(Value::as_str)
            .and_then(|v| {
                parse_color(v).or_else(|| match v.to_lowercase().as_str() {
                    "red" => Some(t.danger),
                    "green" => Some(t.success),
                    "yellow" | "orange" => Some(t.warning),
                    "primarytext" => Some(t.text1),
                    "secondarytext" => Some(t.text3),
                    _ => None,
                })
            })
            .unwrap_or(t.text3);
        let raw = raw.and_then(|v| source(v, t.light)).unwrap_or_default();
        // Built-ins/glyphs remain native text; paths and URLs are never echoed.
        let glyph = if raw.chars().count() <= 4 && !raw.is_ascii() {
            raw.clone()
        } else if raw.starts_with("raycast-")
            || raw.starts_with("tezbar-")
            || raw.chars().next().is_some_and(char::is_uppercase)
                && raw.chars().all(char::is_alphanumeric)
        {
            match raw.rsplit('-').next().unwrap_or("").to_lowercase().as_str() {
                "checkmark" => "✓",
                "star" => "☆",
                "folder" => "▱",
                "plus" => "+",
                "arrowright" => "→",
                "arrowleft" => "←",
                "arrowup" => "↑",
                "arrowdown" => "↓",
                "magnifyingglass" => "⌕",
                "document" | "textdocument" => "▤",
                "trash" => "⌫",
                "gear" => "⚙",
                "heart" => "♡",
                "link" => "↗",
                "globe" => "◎",
                "calendar" => "▦",
                "clock" => "◷",
                "person" => "♙",
                "warning" | "exclamationmark" => "!",
                "code" => "‹›",
                "terminal" => ">_",
                "clipboard" => "▣",
                "download" => "⇩",
                "upload" => "⇧",
                "xmark" => "×",
                "minus" => "−",
                "circle" => "○",
                "square" => "□",
                _ => "",
            }
            .to_owned()
        } else {
            "◇".into()
        };
        let label = if raw.starts_with("raycast-") || raw.starts_with("tezbar-") {
            format!(
                "{} icon",
                raw.split_once('-').map_or(raw.as_str(), |(_, name)| name)
            )
        } else {
            "Extension image".into()
        };
        let glyph = if glyph.is_empty() {
            let name = raw.rsplit('-').next().unwrap_or("");
            let capitals = name
                .chars()
                .filter(|c| c.is_uppercase())
                .take(2)
                .collect::<String>();
            if capitals.is_empty() {
                name.chars().take(2).collect()
            } else {
                capitals
            }
        } else {
            glyph
        };
        div()
            .id(format!("extension-image-{key}"))
            .role(Role::Image)
            .aria_label(label)
            .size(px(size * t.zoom))
            .text_color(tint)
            .child(glyph)
            .into_any_element()
    }
    fn row_view(&self, row: &Row, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.session.as_ref().and_then(|s| s.selection.as_ref()) == Some(&row.id);
        let id = row.id.clone();
        let owner = cx.entity().downgrade();
        let mut d = div()
            .id(format!("extension-row-{id}"))
            .debug_selector({
                let id = id.clone();
                move || format!("extension-row-{id}")
            })
            .role(Role::ListBoxOption)
            .aria_selected(selected)
            .aria_position_in_set(self.rows.iter().position(|r| r.id == row.id).unwrap_or(0) + 1)
            .aria_size_of_set(self.rows.len())
            .tab_index(0)
            .w_full()
            .min_h(px(tokens::ROW * t.zoom))
            .px(px(10.))
            .py(px(5.))
            .rounded(px(tokens::ROW_RADIUS))
            .bg(if selected {
                t.selected
            } else {
                transparent_black()
            })
            .flex()
            .items_center()
            .gap(px(8.))
            .on_click(move |_, w, cx| {
                let _ = owner.update(cx, |v, cx| v.event(UiEvent::Select(id.clone()), w, cx));
            });
        d = d
            .child(self.image(
                &format!("{}:icon", row.id),
                row.node.props.get("icon"),
                t,
                20.,
            ))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(row.node.string("title").unwrap_or("Untitled").to_owned()),
            )
            .child(
                div()
                    .text_color(t.text3)
                    .child(row.node.string("subtitle").unwrap_or("").to_owned()),
            );
        for accessory in row
            .node
            .props
            .get("accessories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let label = accessory
                .get("text")
                .or_else(|| accessory.get("tag"))
                .map(scalar)
                .or_else(|| {
                    accessory.get("date").and_then(Value::as_str).map(|raw| {
                        use tezbar_core::instant::transform::PresentationFormatter;
                        tezbar_core::instant::transform::parse_timestamp_data(raw, 0)
                            .map(|stamp| {
                                super::super::command_bar::locale::NativeLocale
                                    .local_timestamp(stamp.ms)
                            })
                            .unwrap_or_else(|_| raw.to_owned())
                    })
                })
                .unwrap_or_default();
            d = d.child(div().text_color(t.text3).child(label));
        }
        div()
            .when(
                row.section.is_some()
                    && self
                        .rows
                        .iter()
                        .position(|r| r.id == row.id)
                        .is_some_and(|i| i == 0 || self.rows[i - 1].section != row.section),
                |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.text3)
                            .child(row.section.clone().unwrap_or_default()),
                    )
                },
            )
            .child(d)
            .into_any_element()
    }
    fn grid_row(&self, rows: &[Row], t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mut d = div().flex().gap(px(12. * t.zoom)).h(px(156. * t.zoom));
        for row in rows {
            let id = row.id.clone();
            let owner = cx.entity().downgrade();
            let selected = self.session.as_ref().and_then(|s| s.selection.as_ref()) == Some(&id);
            let mut tile = div()
                .id(format!("extension-grid-{id}"))
                .role(Role::ListBoxOption)
                .aria_selected(selected)
                .tab_index(0)
                .flex_1()
                .h_full()
                .p(px(12. * t.zoom))
                .rounded(px(10. * t.zoom))
                .border_1()
                .border_color(t.hairline)
                .bg(if selected {
                    t.selected
                } else {
                    transparent_black()
                })
                .on_click(move |_, w, cx| {
                    let _ = owner.update(cx, |v, cx| v.event(UiEvent::Select(id.clone()), w, cx));
                });
            if let Some(color) = row
                .node
                .props
                .get("content")
                .and_then(tezbar_ui_model::extensions::runtime::color_from_grid_content)
                .and_then(parse_color)
            {
                tile = tile.child(div().h(px(92. * t.zoom)).w_full().bg(color));
            } else {
                tile = tile.child(self.image(
                    &format!("{}:content", row.id),
                    row.node.props.get("content"),
                    t,
                    92.,
                ));
            }
            d = d.child(
                tile.child(
                    div()
                        .text_size(px(12.))
                        .child(row.node.string("title").unwrap_or("").to_owned()),
                )
                .child(
                    div()
                        .text_color(t.text3)
                        .text_size(px(11.))
                        .child(row.node.string("subtitle").unwrap_or("").to_owned()),
                ),
            );
        }
        let heading = rows
            .first()
            .filter(|row| {
                self.rows
                    .iter()
                    .position(|r| r.id == row.id)
                    .is_some_and(|i| i == 0 || self.rows[i - 1].section != row.section)
            })
            .and_then(|r| r.section.clone());
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(18. * t.zoom))
                    .text_color(t.text3)
                    .text_size(px(11.))
                    .child(heading.unwrap_or_default()),
            )
            .child(d)
            .into_any_element()
    }
    fn metadata(&self, n: &Node, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mut d = div().flex().flex_col().gap(px(8.));
        for (i, n) in n.children.iter().enumerate() {
            let label = n
                .string("title")
                .or_else(|| n.string("label"))
                .unwrap_or("")
                .to_owned();
            let row = match n.kind {
                NodeKind::MetadataSeparator | NodeKind::ItemMetadataSeparator => {
                    div().h(px(1.)).bg(t.hairline).into_any_element()
                }
                NodeKind::MetadataLink | NodeKind::ItemMetadataLink => div()
                    .child(label.clone())
                    .child(
                        self.button(
                            format!("metadata-{i}-{label}"),
                            n.string("text")
                                .or_else(|| n.string("title"))
                                .unwrap_or("Open link")
                                .into(),
                            UiEvent::Link(n.string("target").unwrap_or("").into()),
                            t,
                            cx,
                        ),
                    )
                    .into_any_element(),
                NodeKind::MetadataTags | NodeKind::ItemMetadataTags => {
                    let mut tags = div().flex().flex_wrap().gap(px(5.)).child(label);
                    for tag in &n.children {
                        tags = tags.child(
                            div()
                                .text_color(t.text2)
                                .child(tag.string("text").unwrap_or("").to_owned()),
                        );
                    }
                    tags.into_any_element()
                }
                _ => div()
                    .child(div().text_color(t.text3).text_size(px(11.)).child(label))
                    .child(
                        n.props
                            .get("text")
                            .or_else(|| n.props.get("value"))
                            .map(scalar)
                            .unwrap_or_default(),
                    )
                    .into_any_element(),
            };
            d = d.child(row);
        }
        d.into_any_element()
    }
    fn detail(&self, n: &Node, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        if color_wheel::legacy(n) {
            let mut formats = div()
                .flex()
                .flex_col()
                .w(relative(0.45))
                .max_w(px(320. * t.zoom))
                .min_w_0()
                .rounded(px(tokens::ROW_RADIUS))
                .border_1()
                .border_color(t.hairline)
                .bg(t.bar)
                .overflow_hidden()
                .child(
                    div()
                        .border_b_1()
                        .border_color(t.hairline)
                        .px(px(16.))
                        .py(px(12.))
                        .text_size(px(13.))
                        .child("Color Formats"),
                );
            for row in self.color_formats() {
                let owner = cx.entity().downgrade();
                let key = row.key;
                formats = formats.child(
                    controls::action_button(
                        format!("color-format-{key}"),
                        "",
                        ButtonTone::Ghost,
                        false,
                        t,
                        move |_, cx| {
                            let _ = owner.update(cx, |v, cx| v.copy_color_format(key, cx));
                        },
                    )
                    .aria_label(format!("Copy {}", row.title))
                    .px(px(12.))
                    .py(px(10.))
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.text4)
                                    .child(row.title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_family(tezbar_ui::tokens::mono_family())
                                    .child(row.value),
                            ),
                    ),
                );
            }
            return div()
                .flex()
                .gap(px(16.))
                .p(px(16.))
                .h(px(420. * t.zoom))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .rounded(px(tokens::ROW_RADIUS))
                        .border_1()
                        .border_color(t.hairline)
                        .bg(t.bar)
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(24.))
                        .child(self.wheel(cx)),
                )
                .child(formats)
                .into_any_element();
        }
        let mut body = div().flex().flex_col().gap(px(12.)).flex_1().min_w_0();
        if let Some(doc) = self.documents.get(&document_id(n)) {
            body = body.child(doc.view.clone());
        }
        let mut d = div().flex().gap(px(16.)).p(px(16.)).child(body);
        if let Some(metadata) = n.metadata.as_deref().or_else(|| {
            n.children
                .iter()
                .find(|n| matches!(n.kind, NodeKind::Metadata | NodeKind::ItemMetadata))
        }) {
            d = d.child(
                div()
                    .w(px(280. * t.zoom))
                    .border_l_1()
                    .border_color(t.hairline)
                    .pl(px(16. * t.zoom))
                    .child(self.metadata(metadata, t, cx)),
            );
        }
        d.into_any_element()
    }
    fn field(&self, n: &Node, id: String, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        if n.kind == NodeKind::FormSeparator {
            return div().w_full().my(px(8. * t.zoom)).h(px(1.)).bg(t.hairline).into_any_element();
        }
        let title = form_label(n, &id).to_owned();
        let value = self
            .session
            .as_ref()
            .and_then(|s| s.form.values().get(&id))
            .cloned()
            .unwrap_or(Value::Null);
        let control = match n.kind {
            NodeKind::Checkbox => {
                let owner = cx.entity().downgrade();
                let field = id.clone();
                controls::checkbox(
                    format!("field-{id}"),
                    n.string("label").unwrap_or(&title).to_owned(),
                    value == true,
                    t,
                )
                .on_click(move |_, w, cx| {
                    let _ = owner.update(cx, |v, cx| {
                        v.event(
                            UiEvent::ChangeField {
                                id: field.clone(),
                                value: json!(value != true),
                            },
                            w,
                            cx,
                        )
                    });
                })
                .into_any_element()
            }
            NodeKind::FilePicker => div()
                .child(
                    value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(scalar)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .child(self.button(
                    format!("field-{id}"),
                    "Choose files…".into(),
                    UiEvent::PickFiles(id.clone()),
                    t,
                    cx,
                ))
                .into_any_element(),
            NodeKind::Dropdown | NodeKind::TagPicker => {
                let data = n
                    .props
                    .get("data")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_object())
                    .map(|props| Node {
                        kind: if n.kind == NodeKind::TagPicker {
                            NodeKind::TagPickerItem
                        } else {
                            NodeKind::DropdownItem
                        },
                        props: props.clone(),
                        children: vec![],
                        metadata: None,
                    })
                    .collect::<Vec<_>>();
                let mut options = vec![];
                n.visit(&mut options);
                options.extend(data.iter());
                let open = self.field_picker.as_deref() == Some(&id);
                let title = if n.kind == NodeKind::TagPicker {
                    value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    selected_option_title(&options, &value)
                        .or(value.as_str())
                        .unwrap_or("Choose…")
                        .to_owned()
                };
                let owner = cx.entity().downgrade();
                let field = id.clone();
                let trigger = controls::action_button(
                    format!("field-picker-{id}"),
                    format!("{title}  ⌄"),
                    ButtonTone::Quiet,
                    false,
                    t,
                    move |_, cx| {
                        let _ = owner.update(cx, |v, cx| {
                            v.field_picker = if v.field_picker.as_deref() == Some(&field) {
                                None
                            } else {
                                Some(field.clone())
                            };
                            cx.notify();
                        });
                    },
                )
                .aria_expanded(open)
                .aria_label(n.string("title").unwrap_or(&id).to_owned());
                let mut d = div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(trigger);
                let mut choices = div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.hairline)
                    .bg(t.popover_solid)
                    .p(px(6.));
                for (i, option) in options.iter().enumerate() {
                    if matches!(option.kind, NodeKind::DropdownSection) {
                        choices = choices.child(
                            div()
                                .text_color(t.text3)
                                .child(option.string("title").unwrap_or("").to_owned()),
                        );
                    }
                    if !matches!(
                        option.kind,
                        NodeKind::DropdownItem | NodeKind::TagPickerItem
                    ) {
                        continue;
                    }
                    let v = option
                        .string("value")
                        .or_else(|| option.string("title"))
                        .unwrap_or("")
                        .to_owned();
                    let selected = if n.kind == NodeKind::TagPicker {
                        value.as_array().is_some_and(|a| a.contains(&json!(v)))
                    } else {
                        value == json!(v)
                    };
                    let next = if n.kind == NodeKind::TagPicker {
                        let mut a = value.as_array().cloned().unwrap_or_default();
                        if selected {
                            a.retain(|x| x != &json!(v));
                        } else {
                            a.push(json!(v));
                        }
                        json!(a)
                    } else {
                        json!(v)
                    };
                    choices = choices.child(self.button(
                        format!("option-{id}-{i}"),
                        format!(
                            "{}{}",
                            if selected { "✓ " } else { "" },
                            option.string("title").unwrap_or(&v)
                        ),
                        UiEvent::ChangeField {
                            id: id.clone(),
                            value: next,
                        },
                        t,
                        cx,
                    ));
                }
                if open {
                    d = d.child(choices);
                }
                d.into_any_element()
            }
            NodeKind::Description => div()
                .text_color(t.text2)
                .child(n.string("text").unwrap_or("").to_owned())
                .into_any_element(),
            NodeKind::FormSeparator => div().h(px(1.)).bg(t.hairline).into_any_element(),
            _ => self.text_field(&id),
        };
        div()
            .flex()
            .items_start()
            .gap(px(20. * t.zoom))
            .py(px(8. * t.zoom))
            .child(
                div()
                    .w(px(132. * t.zoom))
                    .flex_shrink_0()
                    .py(px(8.))
                    .text_size(px(12.))
                    .text_color(t.text3)
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(control)
                    .when(n.string("error").is_some(), |d| {
                        d.child(
                            div()
                                .id(format!("field-error-{id}"))
                                .role(Role::Alert)
                                .text_size(px(11.))
                                .text_color(t.danger)
                                .child(n.string("error").unwrap_or("").to_owned()),
                        )
                    }),
            )
            .into_any_element()
    }
    fn menu(&self, n: &Node, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mut d = div().flex().flex_col().gap(px(6.)).p(px(8.));
        for (i, child) in n.children.iter().enumerate() {
            d = d.child(match child.kind {
                NodeKind::MenuSeparator => div().h(px(1.)).bg(t.hairline).into_any_element(),
                NodeKind::MenuSection | NodeKind::MenuSubmenu => div()
                    .child(
                        div()
                            .text_color(t.text3)
                            .child(child.string("title").unwrap_or("").to_owned()),
                    )
                    .child(self.menu(child, t, cx))
                    .into_any_element(),
                NodeKind::MenuItem => {
                    let id = child
                        .props
                        .get("actionIds")
                        .and_then(Value::as_array)
                        .and_then(|v| v.first())
                        .and_then(Value::as_str)
                        .or_else(|| child.string("actionId"));
                    if let Some(id) = id {
                        self.button(
                            format!("menu-item-{i}-{id}"),
                            child.string("title").unwrap_or("Item").into(),
                            UiEvent::Menu(id.into()),
                            t,
                            cx,
                        )
                        .into_any_element()
                    } else {
                        div()
                            .text_color(t.text3)
                            .child(child.string("title").unwrap_or("").to_owned())
                            .into_any_element()
                    }
                }
                _ => div().into_any_element(),
            });
        }
        d.into_any_element()
    }
    fn form(&self, n: &Node, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mut nodes = vec![];
        n.visit(&mut nodes);
        let mut d = div()
            .w_full()
            .max_w(px(704. * t.zoom))
            .mx_auto()
            .px(px(32. * t.zoom))
            .py(px(20. * t.zoom))
            .flex()
            .flex_col();
        let mut field_index = 0;
        for n in nodes {
            let id = field_id(n, field_index);
            if n.kind.field() {
                field_index += 1;
            }
            if n.kind.field() || matches!(n.kind, NodeKind::Description | NodeKind::FormSeparator) {
                d = d.child(self.field(n, id, t, cx));
            }
        }
        if let Some(wheel) = n.props.get("nativeColorWheel") {
            d = d.child(self.wheel(cx));
            if let Some(formats) = wheel["formats"].as_array() {
                for (i, row) in formats.iter().take(16).enumerate() {
                    if let (Some(title), Some(value), Some(action)) = (
                        row["title"].as_str(),
                        row["value"].as_str(),
                        row["actionId"].as_str(),
                    ) {
                        d = d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(self.button(
                                    format!("wheel-format-{i}"),
                                    title.into(),
                                    UiEvent::Action(action.into()),
                                    t,
                                    cx,
                                ))
                                .child(div().font_family(tezbar_ui::tokens::mono_family()).child(value.to_owned())),
                        );
                    }
                }
            }
            if let Some(action) = wheel["saveActionId"].as_str() {
                return d
                    .child(self.button(
                        "save-wheel-color".into(),
                        "Save Color".into(),
                        UiEvent::Action(action.into()),
                        t,
                        cx,
                    ))
                    .into_any_element();
            }
        }
        d.child(
            self.button(
                "submit-form".into(),
                self.active_actions(cx)
                    .first()
                    .map(|a| a.title.clone())
                    .unwrap_or_else(|| "Submit".into()),
                UiEvent::Primary,
                t,
                cx,
            ),
        )
        .into_any_element()
    }
    fn setup(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .session
            .as_ref()
            .and_then(|s| {
                s.root
                    .string("title")
                    .or_else(|| s.root.string("navigationTitle"))
            })
            .unwrap_or("Extension");
        let mut d = div().w_full().max_w(px(760. * t.zoom)).mx_auto()
            .px(px(32. * t.zoom)).py(px(24. * t.zoom)).flex().flex_col().gap(px(12.))
            .child(div().flex().items_start().gap(px(16.)).mb(px(12.))
                .child(div().size(px(56. * t.zoom)).rounded(px(14.)).border_1().border_color(t.hairline).bg(t.pill)
                    .flex().items_center().justify_center().text_size(px(18.)).text_color(t.text3)
                    .child(title.chars().next().unwrap_or(' ').to_uppercase().to_string()))
                .child(div().flex_1().flex().flex_col().gap(px(6.))
                    .child(div().text_size(px(11.)).text_color(t.accent_strong).child("One-time setup"))
                    .child(div().text_size(px(22. * t.zoom)).child(format!("Welcome to {title}")))
                    .child(div().text_size(px(12.)).text_color(t.text3).child("Add the settings below to start using this extension. You can change them later from Extensions in Settings."))))
            .child(div().border_b_1().border_color(t.hairline).px(px(16.)).py(px(12.))
                .child(div().text_size(px(12.)).child("Configuration"))
                .child(div().text_size(px(10.5)).text_color(t.text4).child("Required fields are marked with an asterisk.")));
        for f in &self.preference.model.fields {
            let key = f.key();
            let schema = self.preference.schema.iter().find(|v| v["name"] == f.name);
            let label = schema
                .and_then(|v| v["title"].as_str())
                .unwrap_or(&f.name)
                .to_owned();
            let mut row = div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .px(px(16.))
                .py(px(10.))
                .text_size(px(12.))
                .child(label.clone());
            if let Some(description) = schema
                .and_then(|v| v["description"].as_str())
                .filter(|v| !v.is_empty())
            {
                row = row.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.text4)
                        .child(description.to_owned()),
                );
            }
            let value = self
                .preference
                .values
                .get(&key)
                .cloned()
                .unwrap_or(Value::Null);
            if f.kind == "checkbox" {
                let owner = cx.entity().downgrade();
                row = row.child(
                    controls::checkbox(format!("preference-{key}"), label, value == true, t)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |v, cx| {
                                v.edit_preference(key.clone(), json!(value != true), cx)
                            });
                        }),
                );
            } else if f.kind == "dropdown" {
                for option in &f.options {
                    let owner = cx.entity().downgrade();
                    let key = key.clone();
                    let next = option.clone();
                    row = row.child(
                        controls::action_button(
                            format!("preference-{key}-{option}"),
                            schema
                                .and_then(|f| f["data"].as_array())
                                .and_then(|a| a.iter().find(|x| x["value"] == *option))
                                .and_then(|x| x["title"].as_str())
                                .unwrap_or(option)
                                .to_owned(),
                            ButtonTone::Quiet,
                            self.preference.saving,
                            t,
                            move |_, cx| {
                                let _ = owner.update(cx, |v, cx| {
                                    v.edit_preference(key.clone(), json!(next), cx)
                                });
                            },
                        )
                        .aria_selected(value.as_str() == Some(option.as_str()))
                        .when(value.as_str() == Some(option.as_str()), |d| {
                            d.bg(t.selected)
                        }),
                    );
                }
            } else if let Some(field) = self.preference.fields.get(&key) {
                row = row.child(field.input.clone());
            }
            d = d.child(row);
        }
        let owner = cx.entity().downgrade();
        d.child(controls::action_button(
            "save-extension-preferences",
            "Save and continue",
            ButtonTone::Primary,
            self.preference.saving,
            t,
            move |_, cx| {
                let _ = owner.update(cx, |v, cx| v.save_setup(cx));
            },
        ))
        .into_any_element()
    }
    fn active_actions(&self, cx: &App) -> Vec<tezbar_ui_model::extensions::runtime::Action> {
        let Some(s) = &self.session else {
            return vec![];
        };
        let ids = self
            .rows
            .iter()
            .find(|r| s.root.kind != NodeKind::MenuBarExtra && Some(&r.id) == s.selection.as_ref())
            .and_then(|r| r.node.props.get("actionIds"))
            .and_then(Value::as_array);
        s.action_filter(self.action_search.read(cx).content.as_ref())
            .into_iter()
            .filter(|a| {
                ids.is_none_or(|ids| ids.iter().any(|id| id.as_str() == Some(a.id.as_str())))
            })
            .cloned()
            .collect()
    }
    fn action_rows(
        &self,
        cx: &App,
    ) -> Vec<Result<tezbar_ui_model::extensions::runtime::Action, String>> {
        let mut out = Vec::new();
        let mut previous = None;
        for action in self.active_actions(cx) {
            if action.section != previous {
                if let Some(section) = &action.section {
                    if !section.is_empty() {
                        out.push(Err(section.clone()));
                    }
                }
                previous = action.section.clone();
            }
            out.push(Ok(action));
        }
        out
    }
    #[cfg(test)]
    pub(super) fn action_rows_for_test_index(&self, cx: &App) -> usize {
        let Some(action) = self
            .active_actions(cx)
            .get(self.action_index)
            .map(|a| a.id.clone())
        else {
            return 0;
        };
        self.action_rows(cx)
            .iter()
            .position(|r| r.as_ref().is_ok_and(|a| a.id == action))
            .unwrap()
    }
    fn actions(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let count = self.action_rows(cx).len();
        controls::popover("extension-actions", "Extension actions", t)
            .flex()
            .flex_col()
            .w(px(330. * t.zoom))
            .child(self.action_search.clone())
            .when(count == 0, |d| d.child("No matching actions"))
            .child(
                uniform_list(
                    "extension-action-rows",
                    count,
                    cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                        let actions = v.action_rows(cx);
                        let selected = v
                            .active_actions(cx)
                            .get(v.action_index)
                            .map(|a| a.id.clone());
                        range
                            .filter_map(|i| {
                                actions.get(i).map(|row| {
                                    let a = match row {
                                        Ok(action) => action,
                                        Err(section) => {
                                            return div()
                                                .h(px(tokens::ROW * t.zoom))
                                                .px(px(10.))
                                                .py(px(6.))
                                                .text_size(px(11.))
                                                .text_color(t.text3)
                                                .child(section.clone())
                                                .into_any_element();
                                        }
                                    };
                                    let label = a
                                        .shortcut
                                        .as_ref()
                                        .map(|k| {
                                            format!(
                                                "{}  {} {}",
                                                a.title,
                                                k.modifiers.join("+"),
                                                k.key.as_deref().unwrap_or("")
                                            )
                                        })
                                        .unwrap_or_else(|| a.title.clone());
                                    v.button(
                                        format!("extension-action-{}", a.id),
                                        label,
                                        UiEvent::Action(a.id.clone()),
                                        t,
                                        cx,
                                    )
                                    .h(px(tokens::ROW * t.zoom))
                                    .when(a.style.as_deref() == Some("destructive"), |d| {
                                        d.text_color(t.danger)
                                    })
                                    .when(selected.as_ref() == Some(&a.id), |d| d.bg(t.selected))
                                    .into_any_element()
                                })
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.action_scroll)
                .h(px((count.min(8) as f32 * tokens::ROW * t.zoom).max(1.))),
            )
            .into_any_element()
    }
    fn bound_key(&mut self, key: &str, w: &mut Window, cx: &mut Context<Self>) {
        cx.propagate();
        self.key(
            &KeyDownEvent {
                keystroke: Keystroke {
                    key: key.into(),
                    key_char: None,
                    modifiers: w.modifiers(),
                },
                is_held: false,
                prefer_character_input: false,
            },
            w,
            cx,
        );
    }
    pub(super) fn key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        if key == "escape" && self.field_picker.take().is_some() {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let m = e.keystroke.modifiers;
        let primary = if cfg!(target_os = "macos") {
            m.platform
        } else {
            m.control
        };
        // Focused native buttons own their keyboard click through GPUI's
        // paired key-down/key-up path. Do not replace it with Primary/Submit.
        if matches!(key, "enter" | "space")
            && !self.dropdown_open
            && !self.focus.is_focused(w)
            && !self.search.focus_handle(cx).is_focused(w)
            && !self.action_search.focus_handle(cx).is_focused(w)
            && !self
                .fields
                .values()
                .any(|f| f.input.focus_handle(cx).is_focused(w))
        {
            return;
        }
        let field = self
            .fields
            .values()
            .find(|f| f.input.focus_handle(cx).is_focused(w))
            .map(|f| f.input.read(cx));
        let composing = field.is_some_and(|f| f.marked_range.is_some())
            || self.search.read(cx).marked_range.is_some()
            || self.action_search.read(cx).marked_range.is_some();
        if composing {
            return;
        }
        if self.dropdown_open && matches!(key, "escape" | "up" | "down" | "enter") {
            if !e.is_held || key != "enter" {
                if key == "escape" {
                    self.dropdown_open = false;
                    cx.notify();
                } else if key == "enter" {
                    self.choose_dropdown(w, cx);
                } else if let Some((_, _, items)) = self.dropdown_choices() {
                    if !items.is_empty() {
                        self.dropdown_index = if key == "down" {
                            (self.dropdown_index + 1) % items.len()
                        } else {
                            (self.dropdown_index + items.len() - 1) % items.len()
                        };
                        self.scroll_dropdown();
                        cx.notify();
                    }
                }
            }
            cx.stop_propagation();
            return;
        }
        let textarea = field.is_some_and(|f| f.options.multiline);
        let panel = self.session.as_ref().is_some_and(|s| s.action_panel);
        if key == "escape" {
            if !e.is_held {
                self.event(
                    if panel {
                        UiEvent::CloseActions
                    } else {
                        UiEvent::Back
                    },
                    w,
                    cx,
                );
                self.focus_content(w, cx);
            }
            cx.stop_propagation();
            return;
        }
        if primary && key == "k" && !m.shift && !m.alt {
            if !e.is_held {
                self.event(
                    if panel {
                        UiEvent::CloseActions
                    } else {
                        UiEvent::OpenActions
                    },
                    w,
                    cx,
                );
                if panel {
                    self.focus_content(w, cx);
                } else {
                    self.action_search.focus_handle(cx).focus(w, cx);
                }
            }
            cx.stop_propagation();
            return;
        }
        if panel && matches!(key, "up" | "down" | "enter") {
            let actions = self
                .active_actions(cx)
                .into_iter()
                .map(|a| a.id)
                .collect::<Vec<_>>();
            if key == "enter" {
                if !e.is_held {
                    if let Some(id) = actions.get(self.action_index) {
                        self.event(UiEvent::Action(id.clone()), w, cx);
                    }
                }
            } else if !actions.is_empty() {
                self.action_index = if key == "down" {
                    (self.action_index + 1) % actions.len()
                } else {
                    (self.action_index + actions.len() - 1) % actions.len()
                };
                let id = &actions[self.action_index];
                if let Some(row) = self
                    .action_rows(cx)
                    .iter()
                    .position(|r| r.as_ref().is_ok_and(|a| &a.id == id))
                {
                    self.action_scroll
                        .scroll_to_item(row, ScrollStrategy::Nearest);
                }
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        let form = self
            .session
            .as_ref()
            .is_some_and(|s| s.root.kind == NodeKind::Form);
        if key == "enter" {
            if form && m.shift && self.dispatch_shortcut(e, w, cx) {
                return;
            }
            if form {
                if !tezbar_ui_model::extensions::forms::should_submit(
                    tezbar_ui_model::extensions::forms::SubmitKey {
                        enter: true,
                        command: m.platform,
                        control: m.control,
                        alt: m.alt,
                        shift: m.shift,
                        repeat: e.is_held,
                        composing,
                        textarea,
                        mac: cfg!(target_os = "macos"),
                    },
                ) {
                    return;
                }
            } else if e.is_held {
                return;
            }
            self.event(
                if primary && !form {
                    UiEvent::Secondary
                } else {
                    UiEvent::Primary
                },
                w,
                cx,
            );
            cx.stop_propagation();
            return;
        }
        let horizontal = matches!(key, "left" | "right");
        let grid = self
            .session
            .as_ref()
            .is_some_and(|s| s.root.kind == NodeKind::Grid);
        if horizontal && (!grid || self.search.focus_handle(cx).is_focused(w)) {
            return;
        }
        if !form && !primary && !m.alt && matches!(key, "up" | "down" | "left" | "right") {
            let step = if self
                .session
                .as_ref()
                .is_some_and(|s| s.root.kind == NodeKind::Grid)
                && matches!(key, "up" | "down")
            {
                self.columns()
            } else {
                1
            };
            let selected = self.session.as_ref().and_then(|s| s.selection.as_ref());
            let current = self
                .rows
                .iter()
                .position(|r| Some(&r.id) == selected)
                .unwrap_or(0);
            let next = if grid && matches!(key, "up" | "down") {
                let ranges = self.grid_ranges();
                let row = ranges
                    .iter()
                    .position(|r| r.contains(&current))
                    .unwrap_or(0);
                let col = current.saturating_sub(ranges.get(row).map(|r| r.start).unwrap_or(0));
                let next = if key == "up" {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(ranges.len().saturating_sub(1))
                };
                ranges
                    .get(next)
                    .map(|r| r.start + col.min(r.len().saturating_sub(1)))
                    .unwrap_or(current)
            } else if matches!(key, "up" | "left") {
                current.saturating_sub(step)
            } else {
                (current + step).min(self.rows.len().saturating_sub(1))
            };
            if let Some(row) = self.rows.get(next) {
                self.event(UiEvent::Select(row.id.clone()), w, cx);
                self.list_scroll.scroll_to_item(
                    if self.columns() > 1 {
                        self.grid_ranges()
                            .iter()
                            .position(|r| r.contains(&next))
                            .unwrap_or(0)
                    } else {
                        next
                    },
                    ScrollStrategy::Nearest,
                );
            }
            cx.stop_propagation();
            return;
        }
        self.dispatch_shortcut(e, w, cx);
    }
    fn dispatch_shortcut(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if e.is_held {
            return false;
        }
        let key = e.keystroke.key.as_str();
        let m = e.keystroke.modifiers;
        let mut mods = vec![];
        if m.platform {
            mods.push("cmd".into());
        }
        if m.control {
            mods.push("ctrl".into());
        }
        if m.alt {
            mods.push("opt".into());
        }
        if m.shift {
            mods.push("shift".into());
        }
        let id = self
            .session
            .as_ref()
            .and_then(|session| session.shortcut_action_id(key, &mods))
            .map(str::to_owned);
        if let Some(id) = id {
            self.event(UiEvent::Action(id), w, cx);
            cx.stop_propagation();
            return true;
        }
        false
    }
    fn columns(&self) -> usize {
        self.session
            .as_ref()
            .map(|s| grid_columns(&s.root))
            .unwrap_or(1)
    }
    pub(super) fn dropdown_choices(
        &self,
    ) -> Option<(String, String, Vec<(Option<String>, String, String)>)> {
        let root = &self.session.as_ref()?.root;
        let node: Node =
            serde_json::from_value(root.props.get("searchBarAccessory")?.clone()).ok()?;
        let callback = node.string("actionId")?.to_owned();
        let mut choices = Vec::new();
        fn walk(
            node: &Node,
            section: Option<String>,
            out: &mut Vec<(Option<String>, String, String)>,
        ) {
            let section = if node.kind == NodeKind::ListDropdownSection {
                node.string("title").map(str::to_owned)
            } else {
                section
            };
            if node.kind == NodeKind::ListDropdownItem {
                let value = node
                    .string("value")
                    .or_else(|| node.string("title"))
                    .unwrap_or("")
                    .to_owned();
                out.push((
                    section.clone(),
                    node.string("title").unwrap_or(&value).to_owned(),
                    value,
                ));
            }
            for child in &node.children {
                walk(child, section.clone(), out);
            }
        }
        walk(&node, None, &mut choices);
        let selected = node
            .string("value")
            .map(str::to_owned)
            .or_else(|| {
                self.dropdown_value
                    .as_ref()
                    .filter(|value| choices.iter().any(|c| &c.2 == *value))
                    .cloned()
            })
            .or_else(|| node.string("defaultValue").map(str::to_owned))
            .or_else(|| choices.first().map(|c| c.2.clone()))
            .unwrap_or_default();
        Some((callback, selected, choices))
    }
    fn choose_dropdown(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some((callback, _, items)) = self.dropdown_choices() {
            if let Some((_, _, value)) = items.get(self.dropdown_index) {
                let value = value.clone();
                self.dropdown_open = false;
                self.dropdown_value = Some(value.clone());
                self.event(UiEvent::Dropdown { callback, value }, w, cx);
                self.search.focus_handle(cx).focus(w, cx);
            }
        }
    }
    fn dropdown_rows(&self) -> Vec<Result<(usize, String, String), String>> {
        let mut rows = Vec::new();
        let mut previous = None;
        if let Some((_, _, choices)) = self.dropdown_choices() {
            for (i, (section, label, value)) in choices.into_iter().enumerate() {
                if section != previous {
                    if let Some(title) = &section {
                        rows.push(Err(title.clone()));
                    }
                    previous = section;
                }
                rows.push(Ok((i, label, value)));
            }
        }
        rows
    }
    fn scroll_dropdown(&self) {
        if let Some(row) = self
            .dropdown_rows()
            .iter()
            .position(|r| r.as_ref().is_ok_and(|(i, _, _)| *i == self.dropdown_index))
        {
            self.dropdown_scroll
                .scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }
    fn dropdown(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some((_, selected, choices)) = self.dropdown_choices() else {
            return div().into_any_element();
        };
        let label = choices
            .iter()
            .find(|c| c.2 == selected)
            .map(|c| c.1.clone())
            .unwrap_or_else(|| "Filter".into());
        let owner = cx.entity().downgrade();
        let mut container = div().relative().child(
            controls::action_button(
                "extension-dropdown",
                format!("{label} ▾"),
                ButtonTone::Quiet,
                false,
                t,
                move |_, cx| {
                    let _ = owner.update(cx, |v, cx| {
                        v.dropdown_open = !v.dropdown_open;
                        v.dropdown_index = v
                            .dropdown_choices()
                            .and_then(|(_, selected, items)| {
                                items.iter().position(|c| c.2 == selected)
                            })
                            .unwrap_or(0);
                        v.scroll_dropdown();
                        cx.notify();
                    });
                },
            )
            .track_focus(&self.dropdown_focus)
            .role(Role::ComboBox)
            .aria_expanded(self.dropdown_open),
        );
        if self.dropdown_open {
            let count = self.dropdown_rows().len();
            let menu = controls::popover("extension-dropdown-options", "Filter extension", t)
                .absolute()
                .top(px(tokens::HEADER * t.zoom))
                .right_0()
                .w(px(260. * t.zoom))
                .child(
                    uniform_list(
                        "extension-dropdown-rows",
                        count,
                        cx.processor(move |v, range: std::ops::Range<usize>, _, cx| {
                            let rows = v.dropdown_rows();
                            let selected =
                                v.dropdown_choices().map(|(_, s, _)| s).unwrap_or_default();
                            range
                                .filter_map(|i| {
                                    rows.get(i).map(|row| match row {
                                        Err(section) => div()
                                            .h(px(tokens::ROW * t.zoom))
                                            .text_color(t.text3)
                                            .px(px(10.))
                                            .child(section.clone())
                                            .into_any_element(),
                                        Ok((index, label, value)) => {
                                            let index = *index;
                                            let owner = cx.entity().downgrade();
                                            controls::action_button(
                                                format!("extension-dropdown-option-{index}"),
                                                label.clone(),
                                                ButtonTone::Quiet,
                                                false,
                                                t,
                                                move |w, cx| {
                                                    let _ = owner.update(cx, |v, cx| {
                                                        v.dropdown_index = index;
                                                        v.choose_dropdown(w, cx)
                                                    });
                                                },
                                            )
                                            .h(px(tokens::ROW * t.zoom))
                                            .role(Role::ListBoxOption)
                                            .aria_selected(selected == *value)
                                            .when(index == v.dropdown_index, |d| d.bg(t.selected))
                                            .into_any_element()
                                        }
                                    })
                                })
                                .collect()
                        }),
                    )
                    .track_scroll(&self.dropdown_scroll)
                    .h(px(count.min(8).max(1) as f32 * tokens::ROW * t.zoom)),
                );
            container = container.child(menu);
        }
        container.into_any_element()
    }
    pub(super) fn grid_ranges(&self) -> Vec<std::ops::Range<usize>> {
        let columns = self.columns();
        let mut ranges = Vec::new();
        let mut start = 0;
        while start < self.rows.len() {
            let section = &self.rows[start].section;
            let end = (start + 1..self.rows.len())
                .find(|&i| self.rows[i].section != *section)
                .unwrap_or(self.rows.len());
            for i in (start..end).step_by(columns) {
                ranges.push(i..(i + columns).min(end));
            }
            start = end;
        }
        ranges
    }
}
fn parse_color(raw: &str) -> Option<Hsla> {
    let hex = raw.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(|v| rgb(v).into())
}
impl Render for RuntimeView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::tokens(w);
        let root = self.session.as_ref().map(|s| s.root.clone());
        let title = root
            .as_ref()
            .and_then(|n| n.string("navigationTitle").or_else(|| n.string("title")))
            .unwrap_or("Extension")
            .to_owned();
        let mut header = controls::header(t).gap(px(8.)).child(controls::back_button(
            self.button("extension-back".into(), "Back".into(), UiEvent::Back, t, cx),
            t,
        ));
        if root.as_ref().is_some_and(|root| shows_search(root)) {
            header = header
                .child(div().flex_1().child(self.search.clone()))
                .child(self.dropdown(t, cx));
        } else {
            header = header.child(div().flex_1().child(title.clone()));
        }
        let body = if let Some(root) = &root {
            match root.kind {
                NodeKind::Detail | NodeKind::ListItemDetail => div()
                    .id("extension-detail")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.detail(root, t, cx))
                    .into_any_element(),
                NodeKind::Form => div()
                    .id("extension-form")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.form(root, t, cx))
                    .into_any_element(),
                NodeKind::MenuBarExtra => div()
                    .id("extension-menu")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.menu(root, t, cx))
                    .into_any_element(),
                NodeKind::PreferenceSetup => div()
                    .id("extension-preferences")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.setup(t, cx))
                    .into_any_element(),
                _ => {
                    let count = if root.kind == NodeKind::Grid {
                        self.grid_ranges().len()
                    } else {
                        self.rows.len()
                    };
                    let mut body = div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .p(px(8. * t.zoom))
                        .gap(px(8. * t.zoom));
                    if count == 0 {
                        let mut all = vec![];
                        root.visit(&mut all);
                        let empty = all
                            .iter()
                            .find(|n| matches!(n.kind, NodeKind::ListEmpty | NodeKind::GridEmpty));
                        body = body.child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(8.))
                                .p(px(24.))
                                .text_size(px(12. * t.zoom))
                                .text_color(t.text3)
                                .child(
                                    empty
                                        .and_then(|n| n.string("title"))
                                        .unwrap_or("No items")
                                        .to_owned(),
                                )
                                .child(
                                    empty
                                        .and_then(|n| n.string("description"))
                                        .unwrap_or("")
                                        .to_owned(),
                                ),
                        );
                    } else {
                        body = body.child(
                            uniform_list(
                                "extension-items",
                                count,
                                cx.processor(move |v, range: std::ops::Range<usize>, _w, cx| {
                                    range
                                        .map(|i| {
                                            if v.session
                                                .as_ref()
                                                .is_some_and(|s| s.root.kind == NodeKind::Grid)
                                            {
                                                v.grid_row(
                                                    &v.rows[v.grid_ranges()[i].clone()],
                                                    t,
                                                    cx,
                                                )
                                            } else {
                                                v.row_view(&v.rows[i], t, cx)
                                            }
                                        })
                                        .collect()
                                }),
                            )
                            .track_scroll(&self.list_scroll.clone())
                            .flex_1(),
                        );
                    }
                    if root.props.get("isShowingDetail") == Some(&json!(true)) {
                        if let Some(row) = self.rows.iter().find(|r| {
                            self.session.as_ref().and_then(|s| s.selection.as_ref()) == Some(&r.id)
                        }) {
                            if let Some(detail) = item_detail(&row.node) {
                                body = body.child(
                                    div()
                                        .id("extension-item-detail")
                                        .w(relative(0.5))
                                        .min_w_0()
                                        .border_l_1()
                                        .border_color(t.hairline)
                                        .overflow_y_scroll()
                                        .child(self.detail(&detail, t, cx)),
                                );
                            }
                        }
                    }
                    body.into_any_element()
                }
            }
        } else {
            div()
                .flex_1()
                .p(px(24.))
                .child("Run an extension command to open its view.")
                .into_any_element()
        };
        let status = self
            .error
            .map(str::to_owned)
            .or_else(|| self.session.as_ref().and_then(|s| s.notice_message.clone()))
            .unwrap_or_else(|| {
                if self.session.as_ref().is_some_and(|s| {
                    s.notice == Some(tezbar_ui_model::extensions::runtime::Notice::Working)
                }) {
                    return "Working…".into();
                }
                if root
                    .as_ref()
                    .is_some_and(|n| n.props.get("isLoading") == Some(&json!(true)))
                {
                    "Loading…".into()
                } else {
                    if root
                        .as_ref()
                        .is_some_and(|r| matches!(r.kind, NodeKind::List | NodeKind::Grid))
                    {
                        format!("{title} · {} items", self.rows.len())
                    } else {
                        title
                    }
                }
            });
        controls::sheet(t)
            .id("extension-runtime")
            .font_family(tezbar_ui::tokens::font_family(cx, false))
            .relative()
            .track_focus(&self.focus)
            .capture_action(cx.listener(|v, _: &tezbar_ui::text_input::Newline, w, cx| {
                v.bound_key("enter", w, cx)
            }))
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Up, w, cx| v.bound_key("up", w, cx)),
            )
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Down, w, cx| v.bound_key("down", w, cx)),
            )
            .on_scroll(cx.listener(|v, _, w, cx| v.schedule_viewport(w, cx)))
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Right, w, cx| {
                    v.bound_key("right", w, cx)
                }),
            )
            .capture_action(
                cx.listener(|v, _: &tezbar_ui::text_input::Left, w, cx| v.bound_key("left", w, cx)),
            )
            .capture_key_down(cx.listener(|v, e, w, cx| v.key(e, w, cx)))
            .child(header)
            .when(
                root.as_ref()
                    .is_some_and(|n| n.props.get("isLoading") == Some(&json!(true))),
                |d| {
                    d.child(
                        div()
                            .id("extension-loading")
                            .role(Role::ProgressIndicator)
                            .aria_label("Loading extension")
                            .h(px(2.))
                            .w_full()
                            .bg(t.accent_strong),
                    )
                },
            )
            .when(self.error.is_some(), |d| {
                d.child(
                    div()
                        .id("extension-error")
                        .role(Role::Alert)
                        .mx(px(12.))
                        .my(px(8.))
                        .p(px(12.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(t.hairline)
                        .text_size(px(12.))
                        .text_color(t.danger)
                        .child(self.error.unwrap_or("")),
                )
            })
            .child(body)
            .child(
                controls::footer(t)
                    .child(
                        div()
                            .id("extension-status")
                            .flex_1()
                            .role(Role::Status)
                            .child(status),
                    )
                    .when_some(self.footer.clone(), |d, footer| d.child(footer))
                    .when(!self.active_actions(cx).is_empty(), |d| {
                        d.child(self.button(
                            "extension-primary".into(),
                            self.active_actions(cx)[0].title.clone(),
                            UiEvent::Primary,
                            t,
                            cx,
                        ))
                        .child(
                            self.button(
                                "extension-actions-toggle".into(),
                                if cfg!(target_os = "macos") {
                                    "Actions  ⌘K"
                                } else {
                                    "Actions  Ctrl+K"
                                }
                                .into(),
                                UiEvent::OpenActions,
                                t,
                                cx,
                            ),
                        )
                    }),
            )
            .when(self.session.as_ref().is_some_and(|s| s.action_panel), |d| {
                d.child(
                    div()
                        .absolute()
                        .right(px(12.))
                        .bottom(px(tokens::FOOTER + 8.))
                        .child(self.actions(t, cx)),
                )
            })
    }
}

/// One layout policy for rendering, keyboard navigation and viewport leases.
pub(super) fn grid_columns(root: &Node) -> usize {
    if root.kind != NodeKind::Grid {
        return 1;
    }
    root.props
        .get("columns")
        .and_then(Value::as_u64)
        .unwrap_or(4)
        .clamp(2, 8) as usize
}
