//! 原生窗口对面板事件统一检查活跃文档，并经 DocCore 定位和提交。

use aline_buffer::{PatchBudget, SrcSelection, TextBudget};
use aline_edit::{
    EditBudget, EditContext, EditMode, EditRefusal, FindReplacement, PlanOutcome, RereadFailure,
    plan_find_replace,
};
use aline_markdown::{MappingPrecision, ParseBudget, Parsed};
use gpui::{App, AppContext, Context, Entity, SharedString, Window};
use std::sync::Arc;

use super::super::{
    NativeDocumentWindow, OpenFind, content_metrics, edit_error_key, layout_error_key,
};
use super::{
    FindCopy, FindDirection, FindEvent, FindPanel, FindScope, MAX_MATCHES, SearchKey, SearchResults,
};
use crate::{
    CommitOutcome, CurrentSnapshot, DocError, EditorMode, InputIdentity, ModeSwitchOutcome,
    ReadonlyDocument,
};

impl NativeDocumentWindow {
    pub(in super::super) fn open_find(
        &mut self,
        _: &OpenFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composing = self
            .state
            .session()
            .document()
            .is_some_and(|doc| doc.core().composition().is_some());
        if composing {
            self.state.feedback = Some("document.composition_pending");
            cx.notify();
            return;
        }
        if let Some((panel, _)) = &self.find_panel {
            panel.update(cx, |panel, cx| panel.focus(window, cx));
            return;
        }
        let translator = self.copy.translator.clone();
        let Ok(copy) = FindCopy::new(&translator) else {
            self.state.feedback = Some("find.failed");
            cx.notify();
            return;
        };
        let panel = cx.new(|cx| FindPanel::new(translator, copy, cx));
        let subscription = cx.subscribe_in(&panel, window, Self::handle_find_event);
        self.find_return_focus = window.focused(cx);
        self.find_panel = Some((panel.clone(), subscription));
        self.refresh_find(cx);
        panel.update(cx, |panel, cx| panel.focus(window, cx));
        cx.notify();
    }

    fn handle_find_event(
        &mut self,
        sender: &Entity<FindPanel>,
        event: &FindEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .find_panel
            .as_ref()
            .is_none_or(|(panel, _)| panel != sender)
        {
            return;
        }
        match *event {
            FindEvent::Refresh => self.refresh_find(cx),
            FindEvent::Navigate(direction) => {
                if let Err(error) = self.navigate_find(sender, direction, window, cx) {
                    self.state.feedback = Some(find_error_key(error));
                    cx.notify();
                }
            }
            FindEvent::Replace(scope) => {
                if let Err(error) = self.start_find_replace(sender, scope, window, cx) {
                    self.state.feedback = Some(find_error_key(error));
                    cx.notify();
                }
            }
            FindEvent::Close => self.close_find(window, cx),
        }
    }

    pub(in super::super) fn close_focused_find(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.find_contains_focus(window, cx) {
            return false;
        }
        self.close_find(window, cx);
        true
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .find_panel
            .as_ref()
            .is_some_and(|(panel, _)| panel.read(cx).is_composing(cx))
        {
            return;
        }
        self.find_panel = None;
        self.find_replace_task = None;
        self.find_return_focus
            .take()
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        cx.notify();
    }

    pub(in super::super) fn find_contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.find_panel
            .as_ref()
            .is_some_and(|(panel, _)| panel.read(cx).contains_focus(window, cx))
    }

    pub(in super::super) fn refresh_find(&mut self, cx: &mut Context<Self>) {
        let Some((panel, _)) = &self.find_panel else {
            return;
        };
        let document = self.state.session.document_mut();
        let editable = document
            .as_ref()
            .is_some_and(|document| document.is_editable());
        let snapshot = document.and_then(|document| document.current_snapshot().ok());
        panel.update(cx, |panel, cx| panel.refresh(snapshot, editable, cx));
    }

    fn navigate_find(
        &mut self,
        panel: &Entity<FindPanel>,
        direction: FindDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), DocError> {
        let reader = self.reader.as_mut().ok_or(DocError::StaleResult)?;
        let document = self
            .state
            .session
            .document_mut()
            .ok_or(DocError::ClosedTarget)?;
        if document.core().composition().is_some() {
            return Err(DocError::CompositionActive);
        }
        if !document.is_parse_current() {
            return Err(DocError::StaleResult);
        }
        let current = document.current_snapshot()?;
        let target = panel
            .read(cx)
            .target(&current, direction, cx)
            .map_err(DocError::Edit)?;
        let source_bytes = current.text().validate_range(target.range())?;
        let selection = SrcSelection::new(
            current.text().src_offset(source_bytes.start)?,
            current.text().src_offset(source_bytes.end)?,
        )?;
        let before = document.core().mode();
        if before == EditorMode::Visual && reveal_in_source(document.parsed(), target.range()) {
            switch_find_to_source(document)?;
        }
        let identity = document.input_identity()?;
        document.navigate(&identity, &selection)?;
        let viewport_width = f32::from(window.viewport_size().width);
        let metrics = content_metrics(viewport_width, viewport_width);
        match reader.prepare(document, window, metrics, &self.fonts) {
            Ok(true) => reader.schedule_height_hint(window, cx),
            Ok(false) => {}
            Err(error) => {
                self.state.feedback = Some(layout_error_key(error));
                cx.notify();
                return Ok(());
            }
        }
        reader
            .scroll_to_source(document, selection.anchor().byte_offset())
            .map_err(|_| DocError::StaleResult)?;
        if let Some(evidence) = &self.evidence
            && before != document.core().mode()
        {
            evidence
                .borrow_mut()
                .switching(before, document.core().mode());
        }
        panel.update(cx, |panel, cx| panel.accept_target(&target, cx));
        self.state.feedback = None;
        cx.notify();
        Ok(())
    }

    fn start_find_replace(
        &mut self,
        panel: &Entity<FindPanel>,
        scope: FindScope,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), DocError> {
        if self.find_replace_task.is_some() {
            return Ok(());
        }
        let document = self
            .state
            .session
            .document_mut()
            .ok_or(DocError::ClosedTarget)?;
        if !document.is_editable() {
            return Err(DocError::ReadOnly);
        }
        if document.core().composition().is_some() {
            return Err(DocError::CompositionActive);
        }
        if !document.is_parse_current() {
            return Err(DocError::StaleResult);
        }
        let work = FindReplaceWork::freeze(panel.read(cx), document, scope, cx)?;
        let background = cx.background_executor().spawn(async move { work.run() });
        let panel = panel.downgrade();
        self.find_replace_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = background.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let Some(panel) = panel.upgrade() else { return };
                if this
                    .find_panel
                    .as_ref()
                    .is_none_or(|(active, _)| active != &panel)
                {
                    return;
                }
                this.find_replace_task = None;
                match this.apply_find_replace(panel.read(cx), completion, cx) {
                    Ok(CommitOutcome::Committed { .. }) => {
                        this.state.feedback = None;
                        this.refresh_find(cx);
                        this.refresh_after_edit(window, cx);
                        this.note_document_activity(window, cx);
                    }
                    Ok(CommitOutcome::Unchanged) => this.state.feedback = None,
                    Err(DocError::StaleResult | DocError::ClosedTarget) => return,
                    Err(error @ DocError::Edit(EditRefusal::ProtectedRange { .. })) => {
                        this.show_edit_refusal(error)
                    }
                    Err(error) => this.state.feedback = Some(find_error_key(error)),
                }
                cx.notify();
            });
        }));
        Ok(())
    }

    fn apply_find_replace(
        &mut self,
        panel: &FindPanel,
        (work, planned): (FindReplaceWork, Result<(bool, PlanOutcome), EditRefusal>),
        cx: &App,
    ) -> Result<CommitOutcome, DocError> {
        let document = self
            .state
            .session
            .document_mut()
            .ok_or(DocError::ClosedTarget)?;
        let latest = document.current_snapshot()?;
        work.validate(panel, document, &latest, cx)?;
        let (source, outcome) = planned.map_err(DocError::Edit)?;
        let millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let outcome = commit_find_replace(
            document,
            &work.parsed,
            source,
            outcome,
            panel.query_generation(),
            millis,
        )?;
        if source
            && matches!(outcome, CommitOutcome::Committed { .. })
            && let Some(evidence) = &self.evidence
        {
            evidence
                .borrow_mut()
                .switching(EditorMode::Visual, EditorMode::Source);
        }
        Ok(outcome)
    }
}

fn commit_find_replace(
    document: &mut ReadonlyDocument,
    parsed: &Parsed,
    source: bool,
    outcome: PlanOutcome,
    generation: u64,
    millis: u64,
) -> Result<CommitOutcome, DocError> {
    let PlanOutcome::Changed(plan) = outcome else {
        return Ok(CommitOutcome::Unchanged);
    };
    let identity = document.input_identity()?;
    if source {
        document
            .edit_core()?
            .apply_find_change_in_source_mode(&identity, parsed, &plan, generation, millis)
    } else {
        document
            .edit_core()?
            .apply_find_change(&identity, parsed, &plan, generation, millis)
    }
}

fn switch_find_to_source(document: &mut ReadonlyDocument) -> Result<(), DocError> {
    // 定位是只读意图。切换模式不可借机封口仍待定的普通输入。
    if document.core().open_block_draft().is_some() {
        return Err(DocError::PendingInput);
    }
    match document.switch_mode(EditorMode::Source)? {
        ModeSwitchOutcome::Ready => Ok(()),
        ModeSwitchOutcome::Deferred => Err(DocError::CompositionActive),
    }
}

pub(in super::super) fn reveal_in_source(parsed: &Parsed, range: &aline_buffer::SrcRange) -> bool {
    let bytes = range.byte_range();
    let overlaps = |range: &aline_buffer::SrcRange| {
        let other = range.byte_range();
        bytes.start < other.end && bytes.end > other.start
    };
    if parsed
        .protected_ranges()
        .iter()
        .any(|item| overlaps(item.range()))
    {
        return true;
    }
    let mut visible = false;
    for segment in parsed
        .projection()
        .segments()
        .iter()
        .filter(|item| overlaps(item.source_range()))
    {
        match segment.precision() {
            MappingPrecision::Exact => visible = true,
            MappingPrecision::Aligned => {
                let aligned = segment.source_range().byte_range();
                if bytes.start <= aligned.start && aligned.end <= bytes.end {
                    visible = true;
                } else {
                    // 对齐片段可能是实体或解码文本；只命中其源码子串时，绘制层
                    // 没有可对应的完整选区，应转源码视图展示真实命中。
                    return true;
                }
            }
            MappingPrecision::Hidden => return true,
            MappingPrecision::Synthetic => {}
        }
    }
    !visible
}

fn find_error_key(error: DocError) -> &'static str {
    match error {
        DocError::CompositionActive | DocError::PendingInput => edit_error_key(error),
        _ => "find.refused",
    }
}

struct ReplaceIntent {
    key: SearchKey,
    current: Option<usize>,
    replacement: SharedString,
}

struct FindReplaceWork {
    results: Arc<SearchResults>,
    snapshot: CurrentSnapshot,
    identity: InputIdentity,
    parsed: Parsed,
    mode: EditorMode,
    text_budget: TextBudget,
    intent: ReplaceIntent,
}

impl FindReplaceWork {
    fn freeze(
        panel: &FindPanel,
        document: &mut ReadonlyDocument,
        scope: FindScope,
        cx: &App,
    ) -> Result<Self, DocError> {
        if panel.is_composing(cx) {
            return Err(DocError::CompositionActive);
        }
        let snapshot = document.current_snapshot()?;
        let identity = document.input_identity()?;
        let (results, key) = panel
            .search
            .validated(&snapshot, panel.query.read(cx).committed(), panel.case)
            .map_err(DocError::Edit)?;
        let current = match scope {
            FindScope::Current => Some(
                panel
                    .search
                    .current
                    .ok_or(DocError::Edit(EditRefusal::FindMatchUnavailable))?,
            ),
            FindScope::All => None,
        };
        Ok(Self {
            results: results.clone(),
            snapshot,
            identity,
            parsed: document.parsed().clone(),
            mode: document.core().mode(),
            text_budget: document.text_budget(),
            intent: ReplaceIntent {
                key,
                current,
                replacement: panel.replacement.read(cx).text.clone(),
            },
        })
    }

    fn validate(
        &self,
        panel: &FindPanel,
        document: &mut ReadonlyDocument,
        latest: &CurrentSnapshot,
        cx: &App,
    ) -> Result<(), DocError> {
        let input = document.input_identity()?;
        self.identity
            .validate(&input)
            .map_err(|_| DocError::StaleResult)?;
        if panel.is_composing(cx)
            || document.core().composition().is_some()
            || !document.is_parse_current()
            || self.mode != document.core().mode()
            || latest.selection() != self.snapshot.selection()
            || document.parsed().projection().id() != self.parsed.projection().id()
            || self.intent.replacement != panel.replacement.read(cx).text
            || self
                .intent
                .current
                .is_some_and(|ordinal| panel.search.current != Some(ordinal))
        {
            return Err(DocError::StaleResult);
        }
        let (_, key) = panel
            .search
            .validated(latest, panel.query.read(cx).committed(), panel.case)
            .map_err(|_| DocError::StaleResult)?;
        if key != self.intent.key {
            return Err(DocError::StaleResult);
        }
        Ok(())
    }

    fn run(self) -> (Self, Result<(bool, PlanOutcome), EditRefusal>) {
        let mode = match self.mode {
            EditorMode::Visual => EditMode::Visual,
            EditorMode::Source => EditMode::Source,
        };
        let planned = match self.plan(mode) {
            Err(error) if mode == EditMode::Visual && source_fallback(&error) => {
                self.plan(EditMode::Source).map(|outcome| (true, outcome))
            }
            outcome => outcome.map(|outcome| (false, outcome)),
        };
        (self, planned)
    }

    fn plan(&self, mode: EditMode) -> Result<PlanOutcome, EditRefusal> {
        let bytes = self.text_budget.max_bytes();
        let budget = EditBudget::new(
            PatchBudget::new(MAX_MATCHES, bytes, bytes),
            self.text_budget,
            ParseBudget::m0().with_max_source_bytes(bytes),
        );
        let replacement = match self.intent.current {
            Some(match_index) => FindReplacement::Current {
                match_index,
                text: self.intent.replacement.as_ref(),
            },
            None => FindReplacement::All(self.intent.replacement.as_ref()),
        };
        plan_find_replace(
            &self.results,
            &self.intent.key.version,
            &self.parsed,
            self.snapshot.selection(),
            &EditContext::new(mode),
            replacement,
            budget,
        )
    }
}

fn source_fallback(error: &EditRefusal) -> bool {
    matches!(
        error,
        EditRefusal::ProtectedRange {
            kind: aline_markdown::ProtectionKind::RawFallback,
            ..
        } | EditRefusal::HtmlBoundary { .. }
            | EditRefusal::RereadFailed {
                reason: RereadFailure::Projection,
                ..
            }
    )
}

#[cfg(test)]
pub(in super::super) fn replace_all_for_test(
    document: &mut ReadonlyDocument,
    query: SharedString,
    replacement: SharedString,
    consume_generation: u64,
) -> Result<CommitOutcome, DocError> {
    use super::SearchState;
    use aline_edit::FindCase;

    let snapshot = document.current_snapshot()?;
    let identity = document.input_identity()?;
    let mut search = SearchState::default();
    let case = FindCase::Sensitive;
    let (key, results) = search
        .submit(Some(snapshot.clone()), query.clone(), case)
        .ok_or(DocError::StaleResult)?
        .run();
    search.complete(&key, results);
    let (results, key) = search
        .validated(&snapshot, query, case)
        .map_err(DocError::Edit)?;
    let work = FindReplaceWork {
        results: results.clone(),
        snapshot,
        identity,
        parsed: document.parsed().clone(),
        mode: document.core().mode(),
        text_budget: document.text_budget(),
        intent: ReplaceIntent {
            key,
            current: None,
            replacement,
        },
    };
    let (work, planned) = work.run();
    let (source, outcome) = planned.map_err(DocError::Edit)?;
    commit_find_replace(
        document,
        &work.parsed,
        source,
        outcome,
        consume_generation,
        0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aline_buffer::TextSnapshot;
    use aline_markdown::{ParseConfig, parse};

    #[test]
    fn partially_matched_aligned_entity_reveals_source() -> Result<(), Box<dyn std::error::Error>> {
        let source = "x &amp; y";
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(128))?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let partial = snapshot.src_range(3..6)?;
        let entity = snapshot.src_range(2..7)?;

        assert!(reveal_in_source(&parsed, &partial));
        assert!(!reveal_in_source(&parsed, &entity));
        Ok(())
    }
}
