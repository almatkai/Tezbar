//! Selection is keyed by identity. Virtual rows announce their position in the full collection.
use gpui::{prelude::*, *};
#[derive(Default, Debug)]
pub struct Selection {
    pub selected: Option<String>,
    index: usize,
    last_pointer: Option<(f32, f32)>,
}
impl Selection {
    pub fn reconcile(&mut self, ids: &[String]) {
        self.index = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id))
            .unwrap_or(self.index)
            .min(ids.len().saturating_sub(1));
        self.selected = ids.get(self.index).cloned();
    }
    pub fn step(&mut self, delta: isize, ids: &[String]) -> Option<usize> {
        if ids.is_empty() {
            return None;
        }
        self.index = self.index.saturating_add_signed(delta).min(ids.len() - 1);
        self.selected = Some(ids[self.index].clone());
        Some(self.index)
    }
    pub fn pointer(&mut self, at: (f32, f32), index: usize, ids: &[String]) -> bool {
        if self.last_pointer == Some(at) {
            return false;
        }
        self.last_pointer = Some(at);
        if let Some(id) = ids.get(index) {
            self.index = index;
            self.selected = Some(id.clone());
            true
        } else {
            false
        }
    }
    pub fn index(&self) -> usize {
        self.index
    }
}
pub fn row(
    id: SharedString,
    label: SharedString,
    index: usize,
    total: usize,
    selected: bool,
    t: crate::tokens::Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::ListBoxOption)
        .aria_label(label)
        .aria_selected(selected)
        .aria_position_in_set(index + 1)
        .aria_size_of_set(total)
        .h(px(36. * t.zoom))
        .rounded(px(8. * t.zoom))
        .px(px(10. * t.zoom))
        .flex()
        .items_center()
        .bg(if selected {
            t.accent
        } else {
            transparent_black()
        })
        .text_color(if selected {
            rgb(0xffffff).into()
        } else {
            t.text1
        })
}
/// Rows and grids use the same nearest-scroll rule, avoiding selection jumps to the top.
pub fn reveal(handle: &UniformListScrollHandle, index: usize, columns: usize) {
    handle.scroll_to_item(index / columns.max(1), ScrollStrategy::Nearest);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn identity_survives_reorder_and_removal() {
        let mut s = Selection::default();
        let mut ids = vec!["a".into(), "b".into(), "c".into()];
        s.reconcile(&ids);
        s.step(1, &ids);
        ids.swap(0, 1);
        s.reconcile(&ids);
        assert_eq!(s.index(), 0);
        ids.remove(0);
        s.reconcile(&ids);
        assert_eq!(s.selected.as_deref(), Some("a"));
    }
    #[::core::prelude::v1::test]
    fn stationary_pointer_during_scroll_does_not_select() {
        let mut s = Selection::default();
        let ids = vec!["a".into(), "b".into()];
        assert!(s.pointer((5., 5.), 0, &ids));
        assert!(!s.pointer((5., 5.), 1, &ids));
        assert_eq!(s.selected.as_deref(), Some("a"));
    }
}
/// Only visible grid rows are constructed. Cell IDs and set positions stay tied to the full item set.
pub fn grid<V: 'static>(
    id: impl Into<ElementId>,
    ids: Vec<SharedString>,
    columns: usize,
    cx: &mut Context<V>,
    cell: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) -> AnyElement + 'static,
) -> UniformList {
    let columns = columns.clamp(1, 32);
    let total = ids.len();
    let rows = total.div_ceil(columns);
    uniform_list(
        id,
        rows,
        cx.processor(move |view, range: std::ops::Range<usize>, w, cx| {
            range
                .map(|row_index| {
                    // Layout-only row containers have no element IDs. Cells keep their global
                    // identity even when a reorder moves them across virtual rows.
                    div().flex().w_full().children(
                        (row_index * columns..((row_index + 1) * columns).min(total)).map(
                            |index| {
                                div()
                                    .id(ids[index].clone())
                                    .role(Role::GridCell)
                                    .aria_row_index(row_index + 1)
                                    .aria_position_in_set(index + 1)
                                    .aria_size_of_set(total)
                                    .aria_column_index(index % columns + 1)
                                    .flex_1()
                                    .child(cell(view, index, w, cx))
                            },
                        ),
                    )
                })
                .collect::<Vec<_>>()
        }),
    )
}

#[cfg(test)]
mod grid_tests {
    use super::*;
    use ::core::prelude::v1::test;
    struct Harness {
        count: std::rc::Rc<std::cell::Cell<usize>>,
        ids: Vec<SharedString>,
    }
    impl Render for Harness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            grid("grid", self.ids.clone(), 7, cx, |view, index, _, _| {
                view.count.set(view.count.get() + 1);
                div()
                    .h(px(36.))
                    .child(view.ids[index].clone())
                    .into_any_element()
            })
            .size_full()
        }
    }
    #[gpui::test]
    fn thousand_item_grid_constructs_only_visible_rows(cx: &mut TestAppContext) {
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        let (_, cx) = cx.add_window_view(|_, _| Harness {
            count: count.clone(),
            ids: (0..1000).map(|i| format!("item-{i}").into()).collect(),
        });
        cx.run_until_parked();
        assert!(
            count.get() > 0 && count.get() < 1000,
            "constructed {} cells",
            count.get()
        );
    }
}
