//! 原生编辑动作和剪贴板入口。文档核心负责语义与历史，窗口负责平台副作用及派生视图。

use std::path::{Path, PathBuf};

use aline_buffer::{BufferError, SelectionSpec, SrcOffset, SrcRange, SrcSelection, TextBudget};
use aline_markdown::Parsed;
use gpui::{App, ClipboardEntry, ClipboardItem, Context, ImageFormat, Window};

use super::readonly_layout::{LayoutError, ReaderIndex};
use super::{
    NativeDocumentWindow, NativeOpenCoordinator, ReaderSurface, content_metrics, layout_error_key,
};
use crate::document::{ImageInsertError, MAX_RESOURCE_BYTES};
use crate::media::{
    ClipboardImageFormat, ClipboardRoute, ClipboardWriteItem, ClipboardWritePlan,
    plan_clipboard_write,
};
use crate::{
    DocError, DocumentId, EditorMode, InputIdentity, ModeSwitchOutcome, ParseConfigGeneration,
    ParseRefreshCompletion, ParseRefreshDisposition, ParseRefreshJob, ReadonlyDocument,
    WorkingVersion,
};

const MAX_READER_ROWS: usize = 1_000_000;

/// 切回 Visual 后的一次输入恢复请求；只保留身份与源选区，不持有第二份文本。
/// 当前解析尚未就绪时沿用既有刷新链，任何输入、选区或目标变化都会作废请求。
/// 窗口是唯一所有者；成功、拒绝、取消定位/Escape、反切 Source 和关闭/替换均释放。
pub(super) struct VisualInputRestore {
    document: DocumentId,
    version: WorkingVersion,
    parse_config: ParseConfigGeneration,
    selection: SelectionSpec,
}

impl VisualInputRestore {
    pub(super) fn new(document: &ReadonlyDocument) -> Self {
        let core = document.core();
        Self {
            document: core.document_id().clone(),
            version: core.version(),
            parse_config: core.parse_config_generation(),
            selection: core.current_selection(),
        }
    }

    fn matches(&self, document: &ReadonlyDocument) -> bool {
        let core = document.core();
        document.is_editable()
            && core.is_document_alive()
            && core.document_id() == &self.document
            && core.mode() == EditorMode::Visual
            && !core.has_open_block()
            && core.version() == self.version
            && core.parse_config_generation() == self.parse_config
            && core.current_selection() == self.selection
    }

    fn apply(self, document: &mut ReadonlyDocument) -> Result<(), DocError> {
        if !self.matches(document) {
            return Err(DocError::StaleResult);
        }
        let identity = document.input_identity()?;
        let current = document.current_snapshot()?;
        // 受保护区仍只定位；合法可编辑块沿用统一开块映射与零字节/零历史协议。
        document.open_visual_at(&identity, current.selection())?;
        Ok(())
    }

    fn prepare_reader(
        &self,
        document: &ReadonlyDocument,
        parsed: &Parsed,
        index: &mut ReaderIndex,
    ) -> Result<(), LayoutError> {
        if self.matches(document) {
            let range = parsed
                .snapshot()
                .src_range(self.selection.range())
                .map_err(|_| LayoutError::InvalidRange)?;
            index.ensure_blank_input_row(
                parsed,
                &range,
                document.text_budget(),
                MAX_READER_ROWS,
            )?;
        }
        Ok(())
    }
}

/// 受保护写入被拒时冻结的源码定位意图，只有用户另行触发时才移动选区。
pub(super) struct ProtectedSourceLocation {
    identity: InputIdentity,
    range: SrcRange,
}

impl ProtectedSourceLocation {
    fn capture(document: &mut ReadonlyDocument, error: &DocError) -> Option<Self> {
        let DocError::Edit(
            aline_edit::EditRefusal::ProtectedRange { range, .. }
            | aline_edit::EditRefusal::HtmlBoundary { range },
        ) = error
        else {
            return None;
        };
        if document.core().mode() != EditorMode::Visual || !document.is_editable() {
            return None;
        }
        let identity = document.input_identity().ok()?;
        let current = document.current_snapshot().ok()?;
        let range = current.text().src_range(range.clone()).ok()?;
        Some(Self { identity, range })
    }

    pub(super) fn matches(&self, document: &ReadonlyDocument) -> bool {
        let core = document.core();
        document.is_editable()
            && core.is_document_alive()
            && core.mode() == EditorMode::Visual
            && core.document_id() == self.identity.document_id()
            && core.version() == self.identity.working_version()
            && core.parse_config_generation() == self.identity.parse_config_generation()
    }

    fn apply(self, document: &mut ReadonlyDocument) -> Result<SrcOffset, DocError> {
        if !self.matches(document) {
            return Err(DocError::StaleResult);
        }
        let identity = document.input_identity()?;
        self.identity
            .validate(&identity)
            .map_err(DocError::Identity)?;
        let current = document.current_snapshot()?;
        let target = current.text().validate_range(&self.range)?;
        if document.switch_mode(EditorMode::Source)? == ModeSwitchOutcome::Deferred {
            return Err(DocError::CompositionActive);
        }
        // 模式切换可能封口普通草稿。关闭草稿前后的逻辑源码相同，重新绑定目标。
        let identity = document.input_identity()?;
        let current = document.current_snapshot()?;
        let anchor = current.text().src_offset(target.start)?;
        let head = current.text().src_offset(target.end)?;
        document
            .edit_core()?
            .select(&identity, &SrcSelection::new(anchor.clone(), head)?)?;
        Ok(anchor)
    }
}

impl NativeDocumentWindow {
    pub(super) fn show_edit_refusal(&mut self, error: DocError) {
        self.protected_source_location = self
            .state
            .session
            .document_mut()
            .and_then(|document| ProtectedSourceLocation::capture(document, &error));
        self.state.feedback = Some(edit_error_key(error));
    }

    pub(super) fn locate_protected_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_unsettled_input() {
            return;
        }
        let Some(location) = self.protected_source_location.take() else {
            return;
        };
        let result = self
            .state
            .session
            .document_mut()
            .ok_or(DocError::ClosedTarget)
            .and_then(|document| location.apply(document));
        match result {
            Ok(anchor) => {
                self.state.feedback = None;
                self.input_frame = None;
                self.source_input_site = None;
                self.visual_input_restore = None;
                self.update_menu(cx);
                if let Some(document) = self.state.session.document() {
                    if let Some(evidence) = &self.evidence {
                        evidence
                            .borrow_mut()
                            .switching(EditorMode::Visual, EditorMode::Source);
                    }
                    if !document.is_parse_current() {
                        self.refresh_after_edit(window, cx);
                    } else if let Some(reader) = &mut self.reader {
                        let width = f32::from(window.viewport_size().width);
                        match reader.prepare(
                            document,
                            window,
                            content_metrics(width, width),
                            &self.fonts,
                        ) {
                            Ok(reset) => {
                                if reset {
                                    reader.schedule_height_hint(window, cx);
                                }
                                if let Err(error) =
                                    reader.scroll_to_source(document, anchor.byte_offset())
                                {
                                    self.state.feedback = Some(layout_error_key(error));
                                }
                            }
                            Err(error) => self.state.feedback = Some(layout_error_key(error)),
                        }
                    }
                }
                self.refresh_find(cx);
            }
            Err(error) => self.state.feedback = Some(edit_error_key(error)),
        }
        cx.notify();
    }

    pub(super) fn request_view(
        &mut self,
        mode: EditorMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.state.session.document().map(|doc| doc.core().mode());
        self.switch_view(mode, cx);
        // 模式按钮会暂借焦点；只有真正切换成功后归还正文，不能在组合拒绝时抢焦点。
        if self.state.session.document().is_some_and(|document| {
            document.is_editable() && Some(document.core().mode()) != before
        }) {
            self.visual_input_restore = (mode == EditorMode::Visual)
                .then(|| self.state.session.document().map(VisualInputRestore::new))
                .flatten();
            self.restore_visual_input();
            self.focus.focus(window, cx);
        }
        if self.state.session.document().is_some_and(|document| {
            document.is_editable()
                && Some(document.core().mode()) != before
                && !document.is_parse_current()
        }) {
            self.refresh_after_edit(window, cx);
        }
    }

    fn restore_visual_input(&mut self) {
        let Some(restore) = self.visual_input_restore.take() else {
            return;
        };
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        if !restore.matches(document) {
            return;
        }
        if !document.is_parse_current() {
            self.visual_input_restore = Some(restore);
            return;
        }
        if let Err(error) = restore.apply(document) {
            self.state.feedback = Some(edit_error_key(error));
        }
    }

    pub(super) fn edit_document(
        &mut self,
        action: EditAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let now_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let allow_image = !matches!(action, EditAction::PastePlain);
        match self.state.edit(
            action,
            |budget| clipboard_paste(cx, budget, allow_image),
            now_millis,
        ) {
            Ok(effect) => {
                self.state.feedback = None;
                if let Some(key) = effect.notice {
                    self.state.feedback = Some(key);
                }
                if let Some(text) = effect.clipboard
                    && let Some(item) =
                        clipboard_item(plan_clipboard_write(&[ClipboardWriteItem::Text(&text)]))
                {
                    cx.write_to_clipboard(item);
                }
                if effect.refresh {
                    self.refresh_after_edit(window, cx);
                    self.note_document_activity(window, cx);
                }
            }
            Err(DocError::ClosedTarget | DocError::ReadOnly) => return,
            Err(error) => self.show_edit_refusal(error),
        }
        cx.notify();
    }

    /// 拖入文件的窗口适配：逐张走生产插入路径，成功后刷新派生视图。
    pub(super) fn drop_images(
        &mut self,
        paths: &[PathBuf],
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        let now_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        let (inserted, notice) =
            insert_dropped_images(document, paths, image_max_bytes(), now_millis);
        self.state.feedback = notice;
        if inserted {
            self.refresh_after_edit(window, cx);
            self.note_document_activity(window, cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_after_edit(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.invalidate_statistics();
        match self.state.queue_refresh() {
            Ok(Some(job)) => self.start_refresh(job, window, cx),
            Ok(None) => {}
            Err(_) => self.state.feedback = Some("document.refresh_failed"),
        }
    }

    fn start_refresh(&mut self, job: ParseRefreshJob, window: &Window, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.parse_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let generation = this.next_reader_generation.checked_add(1);
                let (next, prepared) =
                    this.state.refresh_finished(completion, |document, parsed| {
                        let generation = generation.ok_or(LayoutError::GenerationExhausted)?;
                        let mut index = ReaderIndex::from_parsed(
                            parsed,
                            MAX_READER_ROWS,
                            document.text_budget(),
                        )?;
                        if let Some(restore) = &this.visual_input_restore {
                            restore.prepare_reader(document, parsed, &mut index)?;
                        }
                        Ok((
                            ReaderSurface::from_index(
                                index,
                                document.core().document_id().clone(),
                                document.core().mode(),
                                generation,
                                this.evidence.clone(),
                                this.media_budget,
                            ),
                            generation,
                        ))
                    });
                this.parse_task = None;
                if let Some((reader, generation)) = prepared {
                    this.restore_visual_input();
                    if let Some(document) = this.state.session.document()
                        && let Err(error) = reader
                            .scroll_to_source(document, document.core().current_selection().head())
                    {
                        this.state.feedback = Some(layout_error_key(error));
                    }
                    reader.schedule_height_hint(window, cx);
                    this.reader = Some(reader);
                    this.next_reader_generation = generation;
                    this.update_menu(cx);
                    this.refresh_statistics(window, cx);
                }
                if let Some(job) = next {
                    this.start_refresh(job, window, cx);
                }
                cx.notify();
            });
        }));
    }
}

#[derive(Clone, Copy)]
pub(super) enum EditAction {
    Cut,
    Copy,
    Paste,
    /// 纯文本粘贴：沿用文本路由，图片剪贴板不插入图片。
    PastePlain,
    SelectAll,
    Undo,
    Redo,
}

struct EditEffect {
    clipboard: Option<String>,
    refresh: bool,
    notice: Option<&'static str>,
}

/// 一次剪贴板粘贴的内容；图片不再回退为文本或静默丢弃。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ClipboardPaste {
    /// 无可用内容。
    None,
    /// 纯文本。
    Text(String),
    /// 单张图片字节；显示名只用于替代文字与资产词干。
    Image {
        /// 建议显示名（含扩展名）。
        display_name: String,
        /// 原始图片字节；格式由插入时按签名判定。
        bytes: Vec<u8>,
    },
}

impl NativeOpenCoordinator {
    fn edit(
        &mut self,
        action: EditAction,
        read_clipboard: impl FnOnce(TextBudget) -> Result<ClipboardPaste, DocError>,
        now_millis: u64,
    ) -> Result<EditEffect, DocError> {
        let document = self.session.document_mut().ok_or(DocError::ClosedTarget)?;
        let budget = document.text_budget();
        let mut clipboard = None;
        let mut image_notice = None;
        let (before, changed) = if matches!(action, EditAction::Undo | EditAction::Redo) {
            // 撤销/重做走文档受检入口：Visual 下触及保护区间时零副作用拒绝。
            let before = {
                let core = document.edit_core()?;
                if core.composition().is_some() {
                    return Err(DocError::CompositionActive);
                }
                core.version()
            };
            match action {
                EditAction::Undo => document.checked_undo()?,
                _ => document.checked_redo()?,
            }
            (before, document.edit_core()?.version())
        } else {
            // 组合与只读先于剪贴板读取判定：拒绝路径不得产生平台读取。
            let before = {
                let core = document.edit_core()?;
                if core.composition().is_some() {
                    return Err(DocError::CompositionActive);
                }
                core.version()
            };
            match action {
                EditAction::Cut => {
                    let core = document.edit_core()?;
                    let identity = core.current_input_identity()?;
                    let text = core.cut_selection(&identity, budget, now_millis)?;
                    clipboard = (!text.is_empty()).then_some(text);
                }
                EditAction::Copy => {
                    let core = document.edit_core()?;
                    let text = core.copy_selection(budget)?;
                    clipboard = (!text.is_empty()).then_some(text);
                }
                EditAction::Paste | EditAction::PastePlain => match read_clipboard(budget)? {
                    ClipboardPaste::Text(text) if !text.is_empty() => {
                        let core = document.edit_core()?;
                        let identity = core.current_input_identity()?;
                        core.replace_selection(&identity, &text, now_millis)?;
                    }
                    // 纯文本粘贴即使收到图片也按无内容处理：命令语义不插入图片。
                    ClipboardPaste::Image {
                        display_name,
                        bytes,
                    } if matches!(action, EditAction::Paste) => {
                        if let Err(error) = document.insert_image(
                            &display_name,
                            &bytes,
                            image_max_bytes(),
                            now_millis,
                        ) {
                            image_notice = Some(image_insert_error_key(error));
                        }
                    }
                    ClipboardPaste::None
                    | ClipboardPaste::Text(_)
                    | ClipboardPaste::Image { .. } => {}
                },
                EditAction::SelectAll => {
                    let core = document.edit_core()?;
                    let identity = core.current_input_identity()?;
                    core.select_all(&identity)?;
                }
                EditAction::Undo | EditAction::Redo => {}
            }
            (before, document.edit_core()?.version())
        };
        let refresh = before != changed;
        let notice = image_notice.or_else(|| {
            (refresh
                && matches!(action, EditAction::Undo | EditAction::Redo)
                && document.note_history_move())
            .then_some("document.reload_undo_paused")
        });
        Ok(EditEffect {
            clipboard,
            refresh,
            notice,
        })
    }

    fn queue_refresh(&mut self) -> Result<Option<ParseRefreshJob>, DocError> {
        self.queued_refresh = Some(self.session.begin_parse_refresh()?);
        Ok(self.take_next_refresh())
    }

    fn take_next_refresh(&mut self) -> Option<ParseRefreshJob> {
        if self.refreshing || self.closed {
            return None;
        }
        let job = self.queued_refresh.take()?;
        self.refreshing = true;
        Some(job)
    }

    fn refresh_finished<T>(
        &mut self,
        completion: ParseRefreshCompletion,
        prepare: impl FnOnce(&ReadonlyDocument, &Parsed) -> Result<T, LayoutError>,
    ) -> (Option<ParseRefreshJob>, Option<T>) {
        self.refreshing = false;
        let prepared = match self.session.accept_parse_refresh_with(completion, prepare) {
            Ok((ParseRefreshDisposition::Applied, prepared)) => prepared,
            Ok((ParseRefreshDisposition::Failed(_), _)) => {
                self.feedback = Some("document.refresh_failed");
                None
            }
            Ok(_) => None,
            Err(error) => {
                self.feedback = Some(layout_error_key(error));
                None
            }
        };
        (self.take_next_refresh(), prepared)
    }
}

fn clipboard_paste(
    cx: &App,
    budget: TextBudget,
    allow_image: bool,
) -> Result<ClipboardPaste, DocError> {
    let Some(item) = cx.read_from_clipboard() else {
        return Ok(ClipboardPaste::None);
    };
    clipboard_paste_from_item(&item, budget, allow_image)
}

/// 按 V-03 冻结路由从剪贴板条目提取一次粘贴内容。
///
/// 文件与文本路由都取字符串项：macOS 文件分支会附带纯文本路径，宿主因此
/// 可以把路径当文本粘贴。图片路由只取与冻结优先级一致的那一项图片字节，
/// 保留原始编码；显示名只影响替代文字与资产词干。`allow_image` 为 false
/// 时（纯文本粘贴）不复制图片字节，直接按无内容处理。
fn clipboard_paste_from_item(
    item: &ClipboardItem,
    budget: TextBudget,
    allow_image: bool,
) -> Result<ClipboardPaste, DocError> {
    match clipboard_route(item) {
        ClipboardRoute::Files | ClipboardRoute::Text => Ok(collect_clipboard_text(item, budget)?
            .map_or(ClipboardPaste::None, ClipboardPaste::Text)),
        ClipboardRoute::Image(format) if allow_image => {
            let bytes = item.entries().iter().find_map(|entry| {
                let ClipboardEntry::Image(image) = entry else {
                    return None;
                };
                (clipboard_image_format(image.format) == format).then(|| image.bytes.clone())
            });
            Ok(
                bytes.map_or(ClipboardPaste::None, |bytes| ClipboardPaste::Image {
                    display_name: format!("pasted.{}", format.extension()),
                    bytes,
                }),
            )
        }
        ClipboardRoute::Image(_) | ClipboardRoute::None => Ok(ClipboardPaste::None),
    }
}

/// 由读回的条目推导冻结路由；不做实际读取或解码。
fn clipboard_route(item: &ClipboardItem) -> ClipboardRoute {
    let mut has_files = false;
    let mut has_text = false;
    let mut images = Vec::new();
    for entry in item.entries() {
        match entry {
            ClipboardEntry::String(_) => has_text = true,
            ClipboardEntry::Image(image) => images.push(clipboard_image_format(image.format)),
            ClipboardEntry::ExternalPaths(_) => has_files = true,
        }
    }
    ClipboardRoute::from_entries(has_files, has_text, &images)
}

fn clipboard_image_format(format: ImageFormat) -> ClipboardImageFormat {
    match format {
        ImageFormat::Png => ClipboardImageFormat::Png,
        ImageFormat::Jpeg => ClipboardImageFormat::Jpeg,
        ImageFormat::Webp => ClipboardImageFormat::Webp,
        ImageFormat::Gif => ClipboardImageFormat::Gif,
        ImageFormat::Svg => ClipboardImageFormat::Svg,
        ImageFormat::Bmp => ClipboardImageFormat::Bmp,
        ImageFormat::Tiff => ClipboardImageFormat::Tiff,
        ImageFormat::Ico => ClipboardImageFormat::Ico,
        ImageFormat::Pnm => ClipboardImageFormat::Pnm,
    }
}

/// 按写入计划构造剪贴板条目；只有纯文本能由本路径写出。
fn clipboard_item(plan: ClipboardWritePlan) -> Option<ClipboardItem> {
    match plan {
        ClipboardWritePlan::Text(text) => Some(ClipboardItem::new_string(text)),
        ClipboardWritePlan::Image(_) | ClipboardWritePlan::None => None,
    }
}

fn collect_clipboard_text(
    item: &ClipboardItem,
    budget: TextBudget,
) -> Result<Option<String>, DocError> {
    let mut text = String::new();
    for entry in item.entries() {
        let ClipboardEntry::String(entry) = entry else {
            continue;
        };
        let fragment = entry.text();
        let needed = text
            .len()
            .checked_add(fragment.len())
            .ok_or(DocError::Buffer(BufferError::LengthOverflow))?;
        if needed > budget.max_bytes() {
            return Err(DocError::Buffer(BufferError::BudgetExceeded {
                needed,
                limit: budget.max_bytes(),
            }));
        }
        text.try_reserve(fragment.len())
            .map_err(|error| DocError::Buffer(BufferError::Allocation(error)))?;
        text.push_str(fragment);
    }
    Ok((!text.is_empty()).then_some(text))
}

pub(super) fn edit_error_key(error: DocError) -> &'static str {
    match error {
        DocError::CompositionActive => "document.composition_pending",
        DocError::Edit(
            aline_edit::EditRefusal::ProtectedRange { .. }
            | aline_edit::EditRefusal::HtmlBoundary { .. },
        ) => "document.edit_protected",
        _ => "document.edit_refused",
    }
}

/// 图片插入失败分类到本地化反馈键；成功路径不产生反馈。
pub(super) fn image_insert_error_key(error: ImageInsertError) -> &'static str {
    match error {
        ImageInsertError::NotEditable => "document.edit_refused",
        ImageInsertError::UnsupportedFormat => "document.image_unsupported",
        ImageInsertError::TooLarge => "document.image_too_large",
        ImageInsertError::Write => "document.resource_insert_failed",
        ImageInsertError::Reference => "document.image_insert_failed",
    }
}

/// 生产粘贴/拖入的单张图片上限；与文档资源预算同源。
fn image_max_bytes() -> u64 {
    u64::try_from(MAX_RESOURCE_BYTES).unwrap_or(u64::MAX)
}

/// 把拖入的文件路径逐张插入为正文图片；组合进行中零副作用拒绝。
///
/// 返回（是否有提交，首个失败反馈键）。单张失败不影响其它可插入文件；
/// 读盘前先按元数据拒绝目录、特殊文件与超限文件，避免无谓读取。
pub(super) fn insert_dropped_images(
    document: &mut ReadonlyDocument,
    paths: &[PathBuf],
    max_bytes: u64,
    now_millis: u64,
) -> (bool, Option<&'static str>) {
    if !document.is_editable() {
        return (false, Some("document.edit_refused"));
    }
    if document
        .edit_core()
        .is_ok_and(|core| core.composition().is_some())
    {
        return (false, Some("document.composition_pending"));
    }
    let limit = max_bytes.min(image_max_bytes());
    let mut inserted = false;
    let mut first_error = None;
    for path in paths {
        match insert_dropped_image(document, path, limit, now_millis) {
            Ok(()) => inserted = true,
            Err(key) => {
                first_error.get_or_insert(key);
            }
        }
    }
    (inserted, first_error)
}

fn insert_dropped_image(
    document: &mut ReadonlyDocument,
    path: &Path,
    limit: u64,
    now_millis: u64,
) -> Result<(), &'static str> {
    let metadata = std::fs::metadata(path).map_err(|_| "document.image_insert_failed")?;
    if !metadata.is_file() {
        return Err("document.image_insert_failed");
    }
    if metadata.len() > limit {
        return Err("document.image_too_large");
    }
    let bytes = std::fs::read(path).map_err(|_| "document.image_insert_failed")?;
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    document
        .insert_image(display_name, &bytes, limit, now_millis)
        .map_err(image_insert_error_key)
}

#[cfg(test)]
mod tests {
    use std::{error::Error, io::Cursor, path::Path};

    use std::path::PathBuf;

    use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec};
    use aline_markdown::{ParseBudget, ParseConfig};
    use aline_source::StorageBudget;

    use super::*;
    use crate::native_window::open_limits;
    use crate::{
        DocLimits, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenJob, ReadonlyOpenLimits,
    };

    struct Files(&'static [u8]);

    impl ReadonlyFileAccess for Files {
        type Reader = Cursor<&'static [u8]>;

        fn open(&self, _: &Path) -> std::io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }

    fn open_job(state: &mut NativeOpenCoordinator) -> Result<ReadonlyOpenJob, Box<dyn Error>> {
        let request = state.request_open()?;
        state
            .accept_dialog(&request, Ok(Some(vec![PathBuf::from("fixture.md")])))
            .ok_or_else(|| "missing read job".into())
    }

    /// 构造只读会话预算；生产 `open_limits()` 自 M2 起可编辑，只读路径需显式固定。
    fn read_only_limits() -> ReadonlyOpenLimits {
        ReadonlyOpenLimits::new(
            StorageBudget::new(4096),
            DocLimits::new(
                TextBudget::new(4096),
                LineIndexBudget::new(128),
                HistoryBudget::new(0, 0),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
    }

    fn open(bytes: &'static [u8], editable: bool) -> Result<NativeOpenCoordinator, Box<dyn Error>> {
        let limits = if editable {
            ReadonlyOpenLimits::new(
                StorageBudget::new(4096),
                DocLimits::new(
                    TextBudget::new(4096),
                    LineIndexBudget::new(128),
                    HistoryBudget::new(16, 4096),
                ),
                ParseConfig::m0(),
                ParseBudget::m0(),
            )
            .with_editing()
        } else {
            read_only_limits()
        };
        let mut state = NativeOpenCoordinator::new(limits);
        let job = open_job(&mut state)?;
        let (_, applied) = state.read_finished(job.read_with(&Files(bytes)), false, |_| Ok(()));
        assert_eq!(applied, Some(()));
        state
            .session
            .document_mut()
            .ok_or("missing document")?
            .switch_mode(EditorMode::Source)?;
        Ok(state)
    }

    /// 系统临时目录下独占创建的测试目录；Drop 时递归清理。
    #[cfg(target_os = "macos")]
    struct ScratchHome(PathBuf);

    #[cfg(target_os = "macos")]
    impl ScratchHome {
        fn create(prefix: &str) -> Result<Self, Box<dyn Error>> {
            use std::time::{SystemTime, UNIX_EPOCH};

            let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            for attempt in 0..100u32 {
                let candidate = std::env::temp_dir().join(format!(
                    "{prefix}-{}-{nonce:x}-{attempt}",
                    std::process::id()
                ));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => return Ok(Self(candidate)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err("无法创建编辑动作测试目录".into())
        }

        fn temp_root(&self) -> Result<crate::document::TempRoot, Box<dyn Error>> {
            std::fs::create_dir_all(self.0.join("Library/Application Support"))?;
            Ok(crate::document::TempRoot::open_or_create(&self.0)
                .map_err(|error| format!("{error:?}"))?)
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for ScratchHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 打开一份注入临时根的未命名可编辑文档（相对路径不建立写关联）。
    #[cfg(target_os = "macos")]
    fn open_untitled(
        temp_root: crate::document::TempRoot,
    ) -> Result<NativeOpenCoordinator, Box<dyn Error>> {
        let mut state = NativeOpenCoordinator::new(open_limits());
        state.session.configure_temp_root(Some(temp_root));
        let job = open_job(&mut state)?;
        let (_, applied) = state.read_finished(job.read_with(&Files(b"x\n")), false, |_| Ok(()));
        assert_eq!(applied, Some(()));
        state
            .session
            .document_mut()
            .ok_or("missing document")?
            .switch_mode(EditorMode::Source)?;
        Ok(state)
    }

    #[cfg(target_os = "macos")]
    fn sample_png() -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/fixtures/m4-v03/samples/plain.png"),
        )?)
    }

    #[cfg(target_os = "macos")]
    fn resource_file_count(root: &Path) -> Result<usize, Box<dyn Error>> {
        fn walk(path: &Path) -> Result<usize, Box<dyn Error>> {
            let mut count = 0;
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_dir() {
                    count += walk(&entry.path())?;
                } else if metadata.is_file() {
                    count += 1;
                }
            }
            Ok(count)
        }
        walk(root)
    }

    #[test]
    fn protected_edit_offers_explicit_source_location_without_changing_history()
    -> Result<(), Box<dyn Error>> {
        const RAW: &str = "前\n\n[toc]\n\n尾";
        let mut state = open(RAW.as_bytes(), true)?;
        let document = state.session.document_mut().ok_or("应存在文档")?;
        document.switch_mode(EditorMode::Visual)?;
        let identity = document.input_identity()?;
        document.edit_core()?.select_all(&identity)?;
        let selection = document.core().current_selection();

        let error = state
            .edit(
                EditAction::Paste,
                |_| Ok(ClipboardPaste::Text("替换".to_owned())),
                1,
            )
            .err()
            .ok_or("跨块粘贴应受保护")?;
        let document = state.session.document_mut().ok_or("应存在文档")?;
        let target = ProtectedSourceLocation::capture(document, &error).ok_or("应提供定位")?;
        assert_eq!(document.core().mode(), EditorMode::Visual);
        assert_eq!(document.core().current_selection(), selection);
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(document.core().buffer_text()?, RAW);

        let offset = target.apply(document)?;
        let start = RAW.find("[toc]").ok_or("应存在目录")?;
        assert_eq!(offset.byte_offset(), start);
        assert_eq!(document.core().mode(), EditorMode::Source);
        assert_eq!(
            document.core().current_selection(),
            SelectionSpec::new(start, start + 5)
        );
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(document.core().buffer_text()?, RAW);
        Ok(())
    }

    #[test]
    fn explicit_protected_source_location_rebinds_after_settling_existing_draft()
    -> Result<(), Box<dyn Error>> {
        const RAW: &str = "前\n\n[toc]\n\n尾";
        let mut state = open(RAW.as_bytes(), true)?;
        let document = state.session.document_mut().ok_or("应存在文档")?;
        document.switch_mode(EditorMode::Visual)?;
        let identity = document.input_identity()?;
        let current = document.current_snapshot()?;
        assert!(document.open_visual_at(&identity, current.selection())?);
        let identity = document.input_identity()?;
        let core = document.edit_core()?;
        core.type_selection(&identity, "草稿", 0)?;
        let identity = core.current_input_identity()?;
        core.select_all(&identity)?;
        let selection = core.current_selection();

        let error = state
            .edit(
                EditAction::Paste,
                |_| Ok(ClipboardPaste::Text("替换".to_owned())),
                1,
            )
            .err()
            .ok_or("跨块粘贴应受保护")?;
        let document = state.session.document_mut().ok_or("应存在文档")?;
        let target = ProtectedSourceLocation::capture(document, &error).ok_or("应提供定位")?;
        assert_eq!(document.core().current_selection(), selection);
        assert_eq!(document.core().buffer_text()?, RAW);
        assert_eq!(document.core().open_block_draft(), Some("草稿"));
        assert_eq!(document.core().history_node_count(), 0);

        let anchor = target.apply(document)?;
        let expected = format!("草稿{RAW}");
        let start = expected.find("[toc]").ok_or("应存在目录")?;
        assert_eq!(anchor.byte_offset(), start);
        assert_eq!(document.core().mode(), EditorMode::Source);
        assert_eq!(
            document.core().current_selection(),
            SelectionSpec::new(start, start + 5)
        );
        assert_eq!(document.core().buffer_text()?, expected);
        assert_eq!(document.core().history_node_count(), 1);
        Ok(())
    }

    #[test]
    fn protected_source_location_rejects_foreign_or_changed_document() -> Result<(), Box<dyn Error>>
    {
        let mut original = open(b"[toc]\n\nword", true)?;
        let mut foreign = open(b"[toc]\n\nword", true)?;
        let first = original.session.document_mut().ok_or("应存在文档")?;
        let second = foreign.session.document_mut().ok_or("应存在另一文档")?;
        first.switch_mode(EditorMode::Visual)?;
        second.switch_mode(EditorMode::Visual)?;
        let error = DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
            kind: aline_markdown::ProtectionKind::Toc,
            range: 0..5,
        });
        let target = ProtectedSourceLocation::capture(first, &error).ok_or("应提供定位")?;
        assert!(matches!(target.apply(second), Err(DocError::StaleResult)));
        assert_eq!(second.core().mode(), EditorMode::Visual);
        assert_eq!(second.core().history_node_count(), 0);

        let target = ProtectedSourceLocation::capture(first, &error).ok_or("应提供定位")?;
        let selection = first.core().current_selection();
        first.switch_mode(EditorMode::Source)?;
        first.switch_mode(EditorMode::Visual)?;
        assert!(matches!(target.apply(first), Err(DocError::Identity(_))));
        assert_eq!(first.core().mode(), EditorMode::Visual);
        assert_eq!(first.core().current_selection(), selection);
        assert_eq!(first.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn visual_mode_restore_waits_for_current_parse_then_reopens_without_editing()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"alpha\n\nbeta\n", true)?;
        state.edit(
            EditAction::Paste,
            |_| Ok(ClipboardPaste::Text("X".to_owned())),
            1,
        )?;
        let document = state.session.document_mut().ok_or("missing document")?;
        document.switch_mode(EditorMode::Visual)?;
        let version = document.core().version();
        let selection = document.core().current_selection();
        let history = document.core().history_node_count();
        let before = document.core().buffer_text()?;
        assert!(!document.is_parse_current());
        assert!(matches!(
            VisualInputRestore::new(document).apply(document),
            Err(DocError::StaleResult)
        ));
        assert!(!document.core().has_open_block());
        let restore = VisualInputRestore::new(document);
        let job = state.queue_refresh()?.ok_or("missing parse job")?;
        let (_, applied) = state.refresh_finished(job.run(), |_, _| Ok(()));
        assert_eq!(applied, Some(()));
        let document = state.session.document_mut().ok_or("missing document")?;
        restore.apply(document)?;
        assert!(document.core().has_open_block());
        assert_eq!(document.core().buffer_text()?, before);
        assert_eq!(document.core().version(), version);
        assert_eq!(document.core().current_selection(), selection);
        assert_eq!(document.core().history_node_count(), history);
        Ok(())
    }

    #[test]
    fn list_exit_restores_blank_input_site_and_starts_a_new_paragraph() -> Result<(), Box<dyn Error>>
    {
        use aline_edit::{BlockCommand, plan_block_command};

        for (source, baseline, typed) in [
            ("- 父\n- ", "- 父\n", "- 父\n\n正文"),
            ("- 父\r\n- ", "- 父\r\n", "- 父\r\n\r\n正文"),
            ("- ", "", "正文"),
        ] {
            for ime in [false, true] {
                let mut state = open(source.as_bytes(), true)?;
                let document = state.session.document_mut().ok_or("missing document")?;
                document.switch_mode(EditorMode::Visual)?;
                let identity = document.input_identity()?;
                let current = document.current_snapshot()?;
                let caret = current.text().src_offset(source.len())?;
                let selection = aline_buffer::SrcSelection::new(caret.clone(), caret)?;
                let core = document.edit_core()?;
                core.select(&identity, &selection)?;
                let prepared = core
                    .prepare_command(|parsed, selection, context, budget| {
                        plan_block_command(
                            parsed,
                            selection,
                            context,
                            BlockCommand::ListEnter,
                            budget,
                        )
                    })?
                    .ok_or("missing list exit plan")?;
                core.publish_command(&prepared, 1)?;
                assert_eq!(core.buffer_text()?, baseline);
                assert_eq!(core.history_node_count(), 1);
                let restore = VisualInputRestore::new(document);
                let job = state.queue_refresh()?.ok_or("missing parse job")?;
                let (_, reader) = state.refresh_finished(job.run(), |document, parsed| {
                    let mut index = ReaderIndex::from_parsed(parsed, 100, document.text_budget())?;
                    restore.prepare_reader(document, parsed, &mut index)?;
                    Ok(index)
                });
                let reader = reader.ok_or("missing visual reader")?;
                let document = state.session.document_mut().ok_or("missing document")?;
                restore.apply(document)?;
                let current = document.current_snapshot()?;
                let open = document
                    .core()
                    .visual_block_range(&current)?
                    .ok_or("missing visual site")?;
                assert_eq!(open.byte_range(), baseline.len()..baseline.len());
                let row = reader.row_count(EditorMode::Visual) - 1;
                let range = reader.source_range(document.parsed(), EditorMode::Visual, row)?;
                assert!(
                    super::super::editable_input::InputSite::for_row(document, range.clone(), None)
                        .is_some(),
                    "source={source:?} baseline={baseline:?} row={range:?} rows={}",
                    reader.row_count(EditorMode::Visual)
                );
                let identity = document.input_identity()?;
                let core = document.edit_core()?;
                if ime {
                    core.update_native_candidate(&identity, "候选", None, 2)?;
                    let mark = core.composition().ok_or("missing composition")?;
                    assert_eq!(
                        mark.marked_range(),
                        Some(typed.len() - "正文".len()..typed.len())
                    );
                    core.composition_cancel(3)?;
                    assert_eq!(core.buffer_text()?, baseline);
                    let identity = core.current_input_identity()?;
                    core.update_native_candidate(&identity, "候选", None, 4)?;
                    let identity = core.current_input_identity()?;
                    let commit = core.prepare_native_confirmation(&identity, Some("正文"), 5)?;
                    core.publish_native_input(commit.run()?)?;
                } else {
                    core.type_selection(&identity, "正", 2)?;
                    let identity = core.current_input_identity()?;
                    core.type_selection(&identity, "文", 3)?;
                    core.commit_open_block(4)?;
                }
                assert_eq!(core.buffer_text()?, typed);
                assert_eq!(core.history_node_count(), 2);
                assert_eq!(
                    core.current_selection(),
                    SelectionSpec::new(typed.len(), typed.len())
                );
                core.undo()?;
                assert_eq!(core.buffer_text()?, baseline);
                core.undo()?;
                assert_eq!(core.buffer_text()?, source);
            }
        }
        Ok(())
    }

    #[test]
    fn visual_mode_restore_rejects_changed_selection_and_foreign_document()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"alpha\n\nbeta\n", true)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        document.switch_mode(EditorMode::Visual)?;
        let selection_restore = VisualInputRestore::new(document);
        let foreign_restore = VisualInputRestore::new(document);
        let identity = document.input_identity()?;
        let current = document.current_snapshot()?;
        let selection = aline_buffer::SrcSelection::new(
            current.text().src_offset(3)?,
            current.text().src_offset(1)?,
        )?;
        document.edit_core()?.select(&identity, &selection)?;
        let version = document.core().version();
        assert!(matches!(
            selection_restore.apply(document),
            Err(DocError::StaleResult)
        ));
        assert_eq!(
            document.core().current_selection(),
            SelectionSpec::new(3, 1)
        );
        assert_eq!(document.core().version(), version);
        assert!(!document.core().has_open_block());

        assert!(state.close_document());
        let job = open_job(&mut state)?;
        let (_, applied) =
            state.read_finished(job.read_with(&Files(b"alpha\n\nbeta\n")), false, |_| Ok(()));
        assert_eq!(applied, Some(()));
        let document = state.session.document_mut().ok_or("missing document")?;
        assert!(matches!(
            foreign_restore.apply(document),
            Err(DocError::StaleResult)
        ));
        assert!(!document.core().has_open_block());
        assert_eq!(document.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn visual_mode_restore_keeps_protected_source_selection_closed() -> Result<(), Box<dyn Error>> {
        let mut state = open(b"---\nt: x\n---\n\nbody\n", true)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        document.switch_mode(EditorMode::Visual)?;
        let version = document.core().version();
        let selection = document.core().current_selection();
        VisualInputRestore::new(document).apply(document)?;
        assert!(!document.core().has_open_block());
        assert_eq!(document.core().buffer_text()?, "---\nt: x\n---\n\nbody\n");
        assert_eq!(document.core().version(), version);
        assert_eq!(document.core().current_selection(), selection);
        assert_eq!(document.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn visual_mode_restore_preserves_directional_selection_without_new_history()
    -> Result<(), Box<dyn Error>> {
        for (anchor, head) in [(1, 3), (3, 1)] {
            let mut state = open(b"alpha\n\nbeta\n", true)?;
            let document = state.session.document_mut().ok_or("missing document")?;
            let identity = document.input_identity()?;
            let current = document.current_snapshot()?;
            let selection = aline_buffer::SrcSelection::new(
                current.text().src_offset(anchor)?,
                current.text().src_offset(head)?,
            )?;
            document.edit_core()?.select(&identity, &selection)?;
            document.switch_mode(EditorMode::Visual)?;
            let version = document.core().version();
            VisualInputRestore::new(document).apply(document)?;
            assert!(document.core().has_open_block());
            assert_eq!(
                document.core().current_selection(),
                SelectionSpec::new(anchor, head)
            );
            assert_eq!(document.core().version(), version);
            assert_eq!(document.core().buffer_text()?, "alpha\n\nbeta\n");
            assert_eq!(document.core().history_node_count(), 0);
        }
        Ok(())
    }

    #[test]
    fn visual_mode_restore_is_invalidated_by_input_config_new_block_or_reverse_switch()
    -> Result<(), Box<dyn Error>> {
        for scenario in ["input", "config", "new-block", "source"] {
            let mut state = open(b"alpha\n\nbeta\n", true)?;
            let document = state.session.document_mut().ok_or("missing document")?;
            document.switch_mode(EditorMode::Visual)?;
            let restore = VisualInputRestore::new(document);
            match scenario {
                "input" => {
                    state.edit(
                        EditAction::Paste,
                        |_| Ok(ClipboardPaste::Text("Q".to_owned())),
                        1,
                    )?;
                }
                "config" => document.edit_core()?.advance_parse_config()?,
                "new-block" => {
                    let identity = document.input_identity()?;
                    let current = document.current_snapshot()?;
                    assert!(document.open_visual_at(&identity, current.selection())?);
                }
                _ => {
                    document.switch_mode(EditorMode::Source)?;
                }
            }
            let document = state.session.document_mut().ok_or("missing document")?;
            let before = (
                document.core().version(),
                document.core().current_selection(),
                document.core().buffer_text()?,
                document.core().has_open_block(),
                document.core().history_node_count(),
            );
            assert!(
                matches!(restore.apply(document), Err(DocError::StaleResult)),
                "{scenario}"
            );
            assert_eq!(
                before,
                (
                    document.core().version(),
                    document.core().current_selection(),
                    document.core().buffer_text()?,
                    document.core().has_open_block(),
                    document.core().history_node_count(),
                ),
                "{scenario}"
            );
        }
        Ok(())
    }

    #[test]
    fn clipboard_paste_uses_the_frozen_route_and_preserves_image_bytes()
    -> Result<(), Box<dyn Error>> {
        use gpui::{Image, ImageFormat};

        use crate::media::{ClipboardImageFormat, ClipboardWritePlan};

        let text = ClipboardItem::new_string("甲".to_owned());
        assert_eq!(
            clipboard_paste_from_item(&text, TextBudget::new(64), true)?,
            ClipboardPaste::Text("甲".to_owned())
        );

        let png = vec![0x89, b'P', b'N', b'G'];
        let image = ClipboardItem {
            entries: vec![
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Jpeg, vec![0xFF, 0xD8])),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, png.clone())),
            ],
        };
        assert_eq!(
            clipboard_paste_from_item(&image, TextBudget::new(64), true)?,
            ClipboardPaste::Image {
                display_name: "pasted.png".to_owned(),
                bytes: png,
            },
            "图片路由按冻结优先级取 PNG，而不是条目里先出现的 JPEG"
        );
        assert_eq!(
            clipboard_paste_from_item(&image, TextBudget::new(64), false)?,
            ClipboardPaste::None,
            "纯文本粘贴不得复制图片字节"
        );

        assert_eq!(
            clipboard_paste_from_item(
                &ClipboardItem {
                    entries: Vec::new()
                },
                TextBudget::new(64),
                true
            )?,
            ClipboardPaste::None
        );

        let oversized = ClipboardItem::new_string("abcdef".to_owned());
        assert!(matches!(
            clipboard_paste_from_item(&oversized, TextBudget::new(3), true),
            Err(DocError::Buffer(BufferError::BudgetExceeded { .. }))
        ));

        let item = clipboard_item(ClipboardWritePlan::Text("甲".to_owned())).ok_or("缺少文本项")?;
        assert_eq!(item.text().as_deref(), Some("甲"));
        assert!(
            clipboard_item(ClipboardWritePlan::Image(ClipboardImageFormat::Png)).is_none(),
            "图片写入不属于文本复制路径"
        );
        assert!(clipboard_item(ClipboardWritePlan::None).is_none());
        Ok(())
    }

    #[test]
    fn image_insert_errors_map_to_distinct_feedback_keys() {
        assert_eq!(
            image_insert_error_key(ImageInsertError::UnsupportedFormat),
            "document.image_unsupported"
        );
        assert_eq!(
            image_insert_error_key(ImageInsertError::TooLarge),
            "document.image_too_large"
        );
        assert_eq!(
            image_insert_error_key(ImageInsertError::Write),
            "document.resource_insert_failed"
        );
        assert_eq!(
            image_insert_error_key(ImageInsertError::Reference),
            "document.image_insert_failed"
        );
        assert_eq!(
            image_insert_error_key(ImageInsertError::NotEditable),
            "document.edit_refused"
        );
    }

    /// 图片粘贴走生产插入路径：提交临时引用、触发刷新且不回写剪贴板。
    #[cfg(target_os = "macos")]
    #[test]
    fn paste_image_commits_reference_and_reports_no_notice() -> Result<(), Box<dyn Error>> {
        let scratch = ScratchHome::create("aline-edit-image-paste")?;
        let root = scratch.temp_root()?;
        let mut state = open_untitled(root.clone())?;
        let png = sample_png()?;

        let effect = state.edit(
            EditAction::Paste,
            |_| {
                Ok(ClipboardPaste::Image {
                    display_name: "pasted.png".to_owned(),
                    bytes: png.clone(),
                })
            },
            1,
        )?;
        assert!(effect.refresh, "插入必须触发派生视图刷新");
        assert_eq!(effect.notice, None);
        assert!(effect.clipboard.is_none(), "图片粘贴不回写剪贴板");

        let document = state.session.document().ok_or("missing document")?;
        let text = document.core().buffer_text()?;
        assert!(
            text.starts_with("![pasted](aline-temp:"),
            "未命名文档应提交受检临时引用，实际: {text:?}"
        );
        assert_eq!(resource_file_count(root.path())?, 1, "临时图片必须落盘");
        Ok(())
    }

    /// 纯文本粘贴即使收到图片内容也零副作用：不插入引用、不落盘、不刷新。
    #[cfg(target_os = "macos")]
    #[test]
    fn paste_plain_ignores_image_content() -> Result<(), Box<dyn Error>> {
        let scratch = ScratchHome::create("aline-edit-image-plain")?;
        let root = scratch.temp_root()?;
        let mut state = open_untitled(root.clone())?;
        let png = sample_png()?;

        let effect = state.edit(
            EditAction::PastePlain,
            |_| {
                Ok(ClipboardPaste::Image {
                    display_name: "pasted.png".to_owned(),
                    bytes: png,
                })
            },
            1,
        )?;
        assert!(!effect.refresh);
        assert_eq!(effect.notice, None);
        let document = state.session.document().ok_or("missing document")?;
        assert_eq!(document.core().buffer_text()?, "x\n");
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(resource_file_count(root.path())?, 0, "纯文本粘贴不得落盘");
        Ok(())
    }

    /// 不支持的图片内容给出专用反馈，且不改文本、不产生临时文件。
    #[cfg(target_os = "macos")]
    #[test]
    fn paste_image_unsupported_reports_notice_without_side_effects() -> Result<(), Box<dyn Error>> {
        let scratch = ScratchHome::create("aline-edit-image-reject")?;
        let root = scratch.temp_root()?;
        let mut state = open_untitled(root.clone())?;

        let effect = state.edit(
            EditAction::Paste,
            |_| {
                Ok(ClipboardPaste::Image {
                    display_name: "pasted.png".to_owned(),
                    bytes: b"not an image".to_vec(),
                })
            },
            1,
        )?;
        assert_eq!(effect.notice, Some("document.image_unsupported"));
        assert!(!effect.refresh, "拒绝路径不得触发刷新");
        let document = state.session.document().ok_or("missing document")?;
        assert_eq!(document.core().buffer_text()?, "x\n");
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(resource_file_count(root.path())?, 0, "拒绝路径不得落盘");
        Ok(())
    }

    /// 拖入多个文件：可插入的逐张插入，首个错误保留为反馈；缺失路径与目录被拒。
    #[cfg(target_os = "macos")]
    #[test]
    fn dropped_images_insert_supported_files_and_report_first_error() -> Result<(), Box<dyn Error>>
    {
        let scratch = ScratchHome::create("aline-edit-image-drop")?;
        let root = scratch.temp_root()?;
        let mut state = open_untitled(root.clone())?;
        let png_path = scratch.0.join("dropped.png");
        std::fs::write(&png_path, sample_png()?)?;
        let text_path = scratch.0.join("notes.txt");
        std::fs::write(&text_path, b"plain text")?;
        let missing = scratch.0.join("missing.png");

        let document = state.session.document_mut().ok_or("missing document")?;
        let (inserted, notice) = insert_dropped_images(
            document,
            &[png_path.clone(), text_path, missing, scratch.0.clone()],
            u64::MAX,
            1,
        );
        assert!(inserted, "受支持图片应插入");
        assert_eq!(
            notice,
            Some("document.image_unsupported"),
            "首个失败必须是文本文件，之后的缺失/目录不覆盖"
        );
        let document = state.session.document().ok_or("missing document")?;
        let text = document.core().buffer_text()?;
        assert!(
            text.starts_with("![dropped](aline-temp:"),
            "拖入图片必须提交受检临时引用，实际: {text:?}"
        );
        assert_eq!(resource_file_count(root.path())?, 1, "只应写一张临时图片");

        // 只含失败项的批次：不插入、给出写入失败反馈。
        let mut state = open_untitled(scratch.temp_root()?)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        let (inserted, notice) =
            insert_dropped_images(document, &[scratch.0.join("missing.png")], u64::MAX, 2);
        assert!(!inserted);
        assert_eq!(notice, Some("document.image_insert_failed"));

        // 超出调用方上限的文件在读入前被拒。
        let document = state.session.document_mut().ok_or("missing document")?;
        let (inserted, notice) = insert_dropped_images(document, &[png_path], 1, 3);
        assert!(!inserted);
        assert_eq!(notice, Some("document.image_too_large"));
        assert_eq!(
            state
                .session
                .document()
                .ok_or("missing document")?
                .core()
                .buffer_text()?,
            "x\n"
        );
        Ok(())
    }

    /// 组合进行中拖入文件零副作用拒绝，且不读取磁盘。
    #[cfg(target_os = "macos")]
    #[test]
    fn dropped_images_are_rejected_while_composition_is_active() -> Result<(), Box<dyn Error>> {
        let scratch = ScratchHome::create("aline-edit-image-compose")?;
        let root = scratch.temp_root()?;
        let mut state = open_untitled(root.clone())?;
        let png_path = scratch.0.join("dropped.png");
        std::fs::write(&png_path, sample_png()?)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        let core = document.edit_core()?;
        let identity = core.current_input_identity()?;
        core.type_selection(&identity, "x", 1)?;
        core.composition_start("候选", 2)?;

        let (inserted, notice) = insert_dropped_images(document, &[png_path], u64::MAX, 3);
        assert!(!inserted);
        assert_eq!(notice, Some("document.composition_pending"));
        let document = state.session.document().ok_or("missing document")?;
        assert_eq!(document.core().buffer_text()?, "xx\n");
        assert!(document.core().composition().is_some());
        assert_eq!(resource_file_count(root.path())?, 0, "拒绝路径不得落盘");
        Ok(())
    }

    #[test]
    fn clipboard_never_reads_without_an_editable_document() -> Result<(), Box<dyn Error>> {
        let mut empty = NativeOpenCoordinator::new(open_limits());
        let mut readonly = open(b"readonly", false)?;
        let mut reads = 0;
        for (state, expected) in [
            (&mut empty, DocError::ClosedTarget),
            (&mut readonly, DocError::ReadOnly),
        ] {
            let error = state
                .edit(
                    EditAction::Paste,
                    |_| {
                        reads += 1;
                        Ok(ClipboardPaste::Text(String::from("new")))
                    },
                    0,
                )
                .err()
                .ok_or("unexpected paste success")?;
            assert_eq!(
                std::mem::discriminant(&error),
                std::mem::discriminant(&expected)
            );
        }
        assert_eq!(reads, 0);
        let doc = readonly.session.document().ok_or("missing document")?;
        assert_eq!(doc.core().buffer_text()?, "readonly");
        assert_eq!(doc.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn clipboard_cut_only_returns_frozen_text_after_a_commit() -> Result<(), Box<dyn Error>> {
        let mut state = open("甲🙂乙".as_bytes(), true)?;
        let collapsed = state.edit(EditAction::Cut, |_| Ok(ClipboardPaste::None), 0)?;
        assert!(collapsed.clipboard.is_none());
        assert!(!collapsed.refresh);
        let core = state
            .session
            .document_mut()
            .ok_or("missing doc")?
            .edit_core()?;
        let snapshot = core.current_snapshot(TextBudget::new(4096))?;
        let text = snapshot.text();
        let selection = aline_buffer::SrcSelection::new(text.src_offset(7)?, text.src_offset(3)?)?;
        let identity = core.current_input_identity()?;
        core.select(&identity, &selection)?;
        let copied = state.edit(EditAction::Copy, |_| Ok(ClipboardPaste::None), 1)?;
        assert_eq!(copied.clipboard.as_deref(), Some("🙂"));
        assert!(!copied.refresh);
        let unchanged = state.edit(
            EditAction::Paste,
            |_| Ok(ClipboardPaste::Text(String::from("🙂"))),
            2,
        )?;
        assert!(unchanged.clipboard.is_none());
        assert!(!unchanged.refresh);
        let cut = state.edit(EditAction::Cut, |_| Ok(ClipboardPaste::None), 3)?;
        assert_eq!(cut.clipboard.as_deref(), Some("🙂"));
        assert!(cut.refresh);
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲乙");
        assert_eq!(doc.core().undo_depth(), 1);
        assert!(!state.close_document());
        let replacement = open_job(&mut state)?;
        let (_, applied) =
            state.read_finished(replacement.read_with(&Files(b"replacement")), false, |_| {
                Ok(())
            });
        assert_eq!(applied, None);
        assert!(
            state
                .edit(EditAction::Undo, |_| Ok(ClipboardPaste::None), 4)?
                .refresh
        );
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲🙂乙");
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(7, 3));
        assert!(
            state
                .edit(EditAction::Redo, |_| Ok(ClipboardPaste::None), 5)?
                .refresh
        );
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲乙");
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(3, 3));
        Ok(())
    }

    #[test]
    fn clipboard_cut_stages_pending_input_before_its_own_undo_step() -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let core = state
            .session
            .document_mut()
            .ok_or("missing doc")?
            .edit_core()?;
        let identity = core.current_input_identity()?;
        core.type_selection(&identity, "甲", 1)?;
        let selected = state.edit(EditAction::SelectAll, |_| Ok(ClipboardPaste::None), 2)?;
        assert!(selected.clipboard.is_none());
        assert!(!selected.refresh);
        let cut = state.edit(EditAction::Cut, |_| Ok(ClipboardPaste::None), 3)?;
        assert_eq!(cut.clipboard.as_deref(), Some("甲base"));
        assert!(cut.refresh);
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "");
        assert_eq!(core.undo_depth(), 2);
        state.edit(EditAction::Undo, |_| Ok(ClipboardPaste::None), 4)?;
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "甲base");
        assert_eq!(core.current_selection(), SelectionSpec::new(0, 7));
        state.edit(EditAction::Undo, |_| Ok(ClipboardPaste::None), 5)?;
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "base");
        assert_eq!(core.current_selection(), SelectionSpec::new(0, 0));
        assert!(state.close_document());
        Ok(())
    }

    #[test]
    fn edit_actions_during_composition_preserve_input_and_never_read_clipboard()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"before", true)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        let core = document.edit_core()?;
        let identity = core.current_input_identity()?;
        core.type_selection(&identity, "x", 1)?;
        core.composition_start("候选", 2)?;
        let version = core.version();
        let selection = core.current_selection();
        let depth = core.history_node_count();
        let mut reads = 0;
        for action in [
            EditAction::Cut,
            EditAction::Copy,
            EditAction::Paste,
            EditAction::PastePlain,
            EditAction::SelectAll,
            EditAction::Undo,
            EditAction::Redo,
        ] {
            assert!(matches!(
                state.edit(
                    action,
                    |_| {
                        reads += 1;
                        Ok(ClipboardPaste::Text(String::from("clipboard")))
                    },
                    3
                ),
                Err(DocError::CompositionActive)
            ));
        }
        assert_eq!(reads, 0);
        let core = state.session.document().ok_or("missing doc")?.core();
        assert!(core.composition().is_some());
        assert_eq!(core.version(), version);
        assert_eq!(core.current_selection(), selection);
        assert_eq!(core.history_node_count(), depth);
        Ok(())
    }

    #[test]
    fn parse_refresh_bounds_concurrency_and_preserves_view_on_preparation_failure()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let original = state
            .session
            .document()
            .ok_or("missing document")?
            .parsed()
            .snapshot()
            .id()
            .clone();
        let loading = state.queue_refresh()?.ok_or("missing first refresh")?;
        state.edit(
            EditAction::Paste,
            |_| Ok(ClipboardPaste::Text(String::from("甲"))),
            1,
        )?;
        assert!(state.queue_refresh()?.is_none());
        state.edit(
            EditAction::Paste,
            |_| Ok(ClipboardPaste::Text(String::from("🙂"))),
            2,
        )?;
        assert!(state.queue_refresh()?.is_none());
        let (next, applied) = state.refresh_finished(loading.run(), |_, _| Ok(()));
        assert_eq!(applied, None);
        let latest = next.ok_or("missing latest refresh")?;
        let (_, applied) =
            state.refresh_finished(latest.run(), |_, _| Err::<(), _>(LayoutError::Budget));
        assert_eq!(applied, None);
        let doc = state.session.document().ok_or("missing document")?;
        assert_eq!(doc.parsed().snapshot().id(), &original);
        assert!(!doc.is_parse_current());
        assert_eq!(doc.core().buffer_text()?, "甲🙂base");
        let retry = state.queue_refresh()?.ok_or("missing retry")?;
        let (_, applied) = state.refresh_finished(retry.run(), |_, _| Ok(()));
        assert_eq!(applied, Some(()));
        let doc = state.session.document().ok_or("missing document")?;
        assert!(doc.is_parse_current());
        assert_ne!(doc.parsed().snapshot().id(), &original);
        Ok(())
    }

    #[test]
    fn closed_document_discards_queued_and_late_refresh_without_preparation()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let completion = state.queue_refresh()?.ok_or("missing first refresh")?.run();
        assert!(state.queue_refresh()?.is_none());
        assert!(state.close_document());
        let mut prepared = false;
        let (next, applied) = state.refresh_finished(completion, |_, _| {
            prepared = true;
            Ok(())
        });
        assert!(next.is_none());
        assert!(applied.is_none());
        assert!(!prepared);
        assert!(state.session.document().is_none());
        Ok(())
    }
}
