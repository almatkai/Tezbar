//! Native provider preview and model search; no provider discovery or network in paint.
use super::*;
impl Launcher {
    pub(super) fn rebuild_models(&mut self, cx: &mut Context<Self>) {
        self.model_providers = presentation::picker_providers(&self.model_config);
        let (models, hidden) = presentation::picker_models(
            &self.model_config,
            &self.model_provider,
            &self.model_query,
        );
        self.model_choices = models
            .iter()
            .filter_map(|m| {
                m["id"]
                    .as_str()
                    .map(|id| (self.model_provider.clone(), id.to_owned()))
            })
            .collect();
        self.model_details = models
            .iter()
            .map(|m| {
                let mut parts = m["capabilities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if let Some(context) = m["contextWindow"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n > 0.)
                {
                    parts.push(format!("{}k context", (context / 1000.).round()));
                }
                parts.join(" · ")
            })
            .collect();
        self.model_hidden = hidden;
        self.model_choice = 0;
        cx.notify();
    }
    pub(super) fn preview_model_provider(
        &mut self,
        provider: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model_preview_epoch += 1;
        self.model_provider = provider;
        self.model_query.clear();
        self.model_search
            .update(cx, |field, cx| field.clear_silently(w, cx));
        self.rebuild_models(cx);
        self.model_search.focus_handle(cx).focus(w, cx);
    }
    pub(super) fn close_models(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.model_menu = false;
        self.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
    pub(super) fn model_popover(&self, t: tokens::Tokens, cx: &mut Context<Self>) -> AnyElement {
        let configured = tezbar_ui_model::settings::providers::configured(
            &self.model_config,
            &self.model_provider,
        );
        let title = self
            .model_providers
            .iter()
            .find(|(id, _)| id == &self.model_provider)
            .map(|(_, title)| title.clone())
            .unwrap_or(self.model_provider.clone());
        let providers = div()
            .id("model-providers")
            .role(Role::ListBox)
            .aria_label("AI providers")
            .w(px(132. * t.zoom))
            .flex_shrink_0()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(t.hairline)
            .children(
                self.model_providers
                    .iter()
                    .enumerate()
                    .map(|(index, (id, title))| {
                        let id = id.clone();
                        let selected = id == self.model_provider;
                        controls::button(
                            SharedString::from(format!("provider-{index}")),
                            title.clone(),
                            controls::ButtonTone::Ghost,
                            false,
                            t,
                        )
                        .w_full()
                        .h(px(32. * t.zoom))
                        .bg(if selected {
                            t.selected
                        } else {
                            transparent_black()
                        })
                        .aria_selected(selected)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.preview_model_provider(id.clone(), w, cx)
                        }))
                    }),
            );
        let list = uniform_list(
            "model-choice-list",
            self.model_choices.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|index| {
                        let (provider, model) = &this.model_choices[index];
                        let active = this.model_config["provider"].as_str().unwrap_or("ollama");
                        let selected = provider == active
                            && this.model_config["providerSelectedModels"][provider]
                                .as_str()
                                .or_else(|| this.model_config["model"].as_str())
                                == Some(model.as_str());
                        controls::button(
                            SharedString::from(format!("model-{index}")),
                            model.clone(),
                            controls::ButtonTone::Ghost,
                            !configured,
                            t,
                        )
                        .h(px(46. * t.zoom))
                        .w_full()
                        .relative()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(8.))
                        .bg(if index == this.model_choice {
                            t.selected
                        } else {
                            transparent_black()
                        })
                        .aria_selected(selected)
                        .on_click(cx.listener(move |this, _, _, cx| this.select_model(index, cx)))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(9.5 * t.zoom))
                                .text_color(t.text3)
                                .child(this.model_details.get(index).cloned().unwrap_or_default()),
                        )
                        .when(selected, |d| {
                            d.child(div().absolute().right(px(8.)).top(px(8.)).child("✓"))
                        })
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.model_scroll)
        .size_full();
        controls::popover("model-menu", "Choose model", t)
            .on_mouse_down_out(cx.listener(|this, _, w, cx| this.close_models(w, cx)))
            .absolute()
            .right(px(12.))
            .top(px(44. * t.zoom))
            .w(px(520. * t.zoom))
            .max_w(relative(0.94))
            .h(px(400. * t.zoom))
            .max_h(relative(0.8))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(36. * t.zoom))
                    .border_b_1()
                    .border_color(t.hairline)
                    .child(div().flex_1().min_w_0().child(self.model_search.clone()))
                    .child(controls::keycap("esc", t)),
            )
            .child(
                div().flex().flex_1().min_h_0().child(providers).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(8.))
                                .py(px(6.))
                                .child(div().text_size(px(11.5 * t.zoom)).child(title))
                                .child(
                                    div().text_size(px(9.5 * t.zoom)).text_color(t.text3).child(
                                        if !configured {
                                            "Setup required".into()
                                        } else if self.model_hidden > 0 {
                                            format!(
                                                "{} shown · {} hidden in Settings",
                                                self.model_choices.len(),
                                                self.model_hidden
                                            )
                                        } else {
                                            format!("{} models available", self.model_choices.len())
                                        },
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .when(self.model_choices.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .p(px(16.))
                                            .text_size(px(11.))
                                            .text_color(t.text3)
                                            .child("No matching models"),
                                    )
                                })
                                .child(list),
                        )
                        .child(
                            controls::button(
                                "configure-models",
                                "Configure",
                                controls::ButtonTone::Quiet,
                                false,
                                t,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model_menu = false;
                                this.navigate(
                                    Surface::Settings("ai".into()),
                                    Payload::Settings { tab: "ai".into() },
                                    cx,
                                );
                            })),
                        ),
                ),
            )
            .into_any_element()
    }
}
