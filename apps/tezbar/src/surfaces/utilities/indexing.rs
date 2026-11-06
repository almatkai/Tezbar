use super::*;
impl Utilities {
    pub(super) fn index_effect(&mut self, e: model::IndexingEffect, cx: &mut Context<Self>) {
        use model::IndexingEffect as E;
        match e {
            E::List {
                generation,
                query,
                offset,
                limit,
            } => self.request(
                UtilityRequest::Sources {
                    query,
                    offset,
                    limit,
                },
                Completion::Sources(generation),
                false,
                cx,
            ),
            E::OpenFile(p) => self.request(UtilityRequest::Open(p), Completion::Simple, false, cx),
            E::Reveal(p) => cx.emit(UtilityEffect::Reveal(p.into())),
            E::Start => self.request(UtilityRequest::IndexStart, Completion::Simple, false, cx),
            E::Pause => self.request(UtilityRequest::IndexPause, Completion::Simple, false, cx),
            E::Resume => self.request(UtilityRequest::IndexResume, Completion::Simple, false, cx),
            E::Snapshot => self.refresh(cx),
        }
    }
    pub(super) fn index_action(&mut self, a: Action, cx: &mut Context<Self>) {
        use model::IndexingEffect as E;
        let e = match a {
            Action::Select(i) => {
                self.indexing.selected = i;
                None
            }
            Action::MoreSources => self.indexing.load(true),
            Action::Activate => self.indexing.source_action(false),
            Action::Reveal => self.indexing.source_action(true),
            Action::IndexStart => Some(E::Start),
            Action::IndexPause => Some(E::Pause),
            Action::IndexResume => Some(E::Resume),
            Action::Settings => {
                cx.emit(UtilityEffect::OpenSettings("knowledge".into()));
                None
            }
            _ => None,
        };
        if let Some(e) = e {
            self.index_effect(e, cx);
        }
    }
}
impl Utilities {
    pub(super) fn knowledge_changed(&mut self, value: Value) {
        let status = if value["status"].is_object() {
            &value["status"]
        } else {
            &value
        };
        let state = status["state"].as_str().unwrap_or("idle");
        let label = match state {
            "scanning" => "Scanning folders",
            "indexing" => "Indexing now",
            "paused" => "Paused",
            "failed" => "Needs attention",
            "completed" => "Up to date",
            _ => "Standing by",
        };
        let total = status["totalPageCount"].as_u64().unwrap_or(0);
        let indexed = status["indexedPageCount"].as_u64().unwrap_or(0);
        let sources = status["sourceCount"].as_u64().unwrap_or(0);
        let progress = status["progress"]
            .as_f64()
            .filter(|n| n.is_finite())
            .unwrap_or(0.)
            .clamp(0., 1.);
        self.background.index_progress = progress as f32;
        self.background.index_summary = format!(
            "{label} · {:.0}% · {sources} files · {indexed}/{total} pages{}",
            progress * 100.,
            status["error"]
                .as_str()
                .map(|error| format!(" · {error}"))
                .unwrap_or_default()
        );
        self.background.knowledge = Some(value);
    }
}
