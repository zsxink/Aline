//! 正文输入适配持有局部只读投影和布局；输入文本、组合、选区和历史仍归 DocCore。
//!
//! 所有者：DocCore 拥有文本、组合候选、选区与历史；窗口协调器
//! `NativeDocumentWindow` 拥有 `InputScheduler` 的在途/等待队列、后台任务与
//! `composing_input_frame`；`InputView` 只持有可重建的局部投影和布局。
//!
//! 生命周期：按键先在主线程经 `InputView::step` 做同步决策——组内输入与选区移动
//! 直接应用，独立删除、组合确认、900ms 边界封口与 Escape 只冻结 `NativeInputJob`；
//! job 在后台 `run`，结果回主线程由 `finish_input_job` 复验发布；封口成功后触发键
//! 放回队首，再由 `drain_inputs` 依次消费。
//!
//! 副作用与反馈：准备段只读，后台只读，同步应用与发布只改 DocCore 的既定状态。
//! 发布失败先在同一最新状态重试一次；封口再失败时按 D2b.5 允许一次同步退化，成功
//! 反馈 `document.input_applied_after_failure`，失败反馈 `document.input_dropped`；
//! 独立 job 再失败清空队列并反馈 `document.input_dropped`。文档已关闭或替换时按
//! D3.6 只丢弃在途与等待输入、不设 feedback（文档已不存在，反馈无处呈现）。
//!
//! 菜单/快捷键显式命令（撤销、重做、剪切、粘贴、全选、模式切换）不经过
//! `InputScheduler`；若在途 job 因此过期，重试与同步退化前会按最新文档状态重建
//! 输入帧，命令前已排队的输入仍按序应用。帧无法重建且文档仍可编辑时，按键与
//! IME 输入反馈 `document.input_dropped`；文档已关闭或只读时不反馈。
//! Escape 成功后清空排队输入并反馈 `document.input_dropped`。
//!
//! `prepare_seal` 返回 Err 时直接拒绝（`InputStep::Rejected`）：该错误来自主线程
//! 冻结的前置条件（身份/可写/组合/范围），在同步调用内是确定性的；重试或 D2b.5
//! 同步退化会在同一状态上得到同一错误，不会多保下一个按键。代价是该按键被拒绝并
//! 显示编辑拒绝反馈（组合类错误显示组合待定），不会静默丢失。D2b.5 的「重试一次→
//! 同步退化」只适用于后台 job 结果失败与准备段 `Ok(None)` 的内部不一致（`recover`），
//! 不覆盖这一分支。

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ops::Range,
    rc::Rc,
};

use aline_buffer::{BufferError, SrcOffset, SrcRange, SrcSelection, TextBudget};
use aline_markdown::{MappingBudget, Projection, ProjectionId};
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, FocusHandle, InputHandler, MouseButton,
    Pixels, Point, StyledText, UTF16Selection, WeakEntity, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

use crate::document::native_input_coordinates::{BoundInputCoordinates, NativeInputCoordinates};
use crate::document::{NativeInputJob, PreparedNativeInput};
use crate::{
    CurrentSnapshot, DocError, DocumentId, EditorMode, InputIdentity, PublicationOutcome,
    ReadonlyDocument,
};

use super::{NativeDocumentWindow, PreparedLayout, SaveIntent, edit_error_key, tokens};

const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAPPING: MappingBudget = MappingBudget::new(MAX_INPUT_BYTES, MAX_INPUT_BYTES + 1);
const LINE_HEIGHT: f32 = 25.0;

/// Source 的虚拟行以该帧的绝对源码范围为入口，只接受范围内的同一局部输入。
#[derive(Clone)]
pub(super) struct InputSite {
    mode: EditorMode,
    source: Range<usize>,
    document_len: usize,
    reader_row: SrcRange,
}

impl InputSite {
    pub(super) fn for_row(
        document: &mut ReadonlyDocument,
        row: SrcRange,
        active: Option<&SourceInputSite>,
    ) -> Option<Self> {
        if !document.is_editable() {
            return None;
        }
        let mode = document.core().mode();
        let current = document.current_snapshot().ok()?;
        let bytes = row.byte_range();
        let source = if mode == EditorMode::Visual {
            let range = document
                .core()
                .visual_block_range(&current)
                .ok()??
                .byte_range();
            (range.start == bytes.start).then_some(range)?
        } else if row.snapshot_id() == current.snapshot_id() {
            let head = document.core().current_selection().head();
            let snapshot = current.text();
            let ends_at_separator = || {
                snapshot
                    .src_range(head..head.checked_add(1)?)
                    .ok()
                    .and_then(|range| snapshot.slice(&range, document.text_budget()).ok())
                    .is_some_and(|separator| separator == "\n" || separator == "\r")
                    .then_some(())
            };
            if !bytes.contains(&head)
                && (head != bytes.end
                    || (head != current.len_bytes() && ends_at_separator().is_none()))
            {
                return None;
            }
            bytes
        } else {
            // 渲染旧列表时只沿用经同一输入回调更新的局部绑定。
            let active = active?;
            let identity = document.input_identity().ok()?;
            active.identity.validate(&identity).ok()?;
            return (active.site.reader_row == row).then(|| active.site.clone());
        };
        Some(Self {
            mode,
            source,
            document_len: current.len_bytes(),
            reader_row: row,
        })
    }

    /// 按最新快照重建输入站点；无法证明同一逻辑行时返回 None 并由调用方清空帧。
    ///
    /// Source 分支不再只按长度差平移：普通按键先按长度差沿用原行（只做 O(1) 字节
    /// 校验），候选不成立（显式命令改动当前行之前的文本、换行、行被合并等）时，
    /// 才扫描选区所在逻辑行。该扫描不建全文行索引，最坏只遍历快照一遍，仍在命令
    /// 之后的重建路径内，不进入普通按键路径。
    fn rebound(&self, document: &mut ReadonlyDocument) -> Option<Self> {
        if !document.is_editable() || document.core().mode() != self.mode {
            return None;
        }
        let current = document.current_snapshot().ok()?;
        let source = match self.mode {
            EditorMode::Visual => {
                let source = document
                    .core()
                    .visual_block_range(&current)
                    .ok()??
                    .byte_range();
                (self.source.start == source.start).then_some(source)?
            }
            EditorMode::Source => {
                let candidate =
                    resized(self.source.clone(), self.document_len, current.len_bytes())?;
                if valid_source_line(document, &current, &candidate) {
                    candidate
                } else {
                    let line = source_line(&current)?;
                    // 选区所在行起点前移说明原输入行已被并入更早的行；
                    // 无法证明仍是同一逻辑行时不得把排队输入写到新偏移。
                    if line.start < self.source.start
                        || !valid_source_line(document, &current, &line)
                    {
                        return None;
                    }
                    line
                }
            }
        };
        let selection = current.selection().range().byte_range();
        if selection.start < source.start || selection.end > source.end {
            return None;
        }
        Some(Self {
            mode: self.mode,
            source,
            document_len: current.len_bytes(),
            reader_row: self.reader_row.clone(),
        })
    }
}

/// 校验区间是否为当前快照中的完整逻辑行：起点在文件开头或紧邻 `\n`，
/// 终点在文件末尾或紧邻 `\n`/`\r`，且区间在快照内、UTF-8 边界合法。
///
/// 额外拒绝在 CRLF 的 `\r` 与 `\n` 之间截断，避免把行尾符算作行内容。
fn valid_source_line(
    document: &ReadonlyDocument,
    current: &CurrentSnapshot,
    range: &Range<usize>,
) -> bool {
    let len = current.len_bytes();
    if range.start > range.end || range.end > len {
        return false;
    }
    let text = current.text();
    // 受检范围同时覆盖越界与半字符端点。
    if text.src_range(range.clone()).is_err() {
        return false;
    }
    let budget = document.text_budget();
    if range.start > 0 && text_byte(text, range.start - 1, budget) != Some(b'\n') {
        return false;
    }
    if range.end < len {
        match text_byte(text, range.end, budget) {
            Some(b'\n') => {
                // 行内容不得包含 CRLF 的 `\r`。
                if range.end > range.start && text_byte(text, range.end - 1, budget) == Some(b'\r')
                {
                    return false;
                }
            }
            // 单独的回车不是行分隔符，只有 CRLF 的 `\r` 才能作为行内容终点。
            Some(b'\r') => {
                if text_byte(text, range.end + 1, budget) != Some(b'\n') {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// 读取快照中的一个字节；越界、半字符或预算不足时返回 None。
fn text_byte(text: &aline_buffer::TextSnapshot, index: usize, budget: TextBudget) -> Option<u8> {
    let range = text.src_range(index..index.checked_add(1)?).ok()?;
    text.slice(&range, budget).ok()?.as_bytes().first().copied()
}

/// 扫描选区所在逻辑行；行起止只在本行内判定，不依赖全文行索引。
///
/// 换行字节归属其所在行（与 `LineIndex` 语义一致）：起点取选区前最后一个 `\n`
/// 之后，终点取选区处或之后的第一个 `\n` 之前，并排除 CRLF 的 `\r`。
fn source_line(current: &CurrentSnapshot) -> Option<Range<usize>> {
    let len = current.len_bytes();
    let head = current.selection().head().byte_offset();
    if head > len {
        return None;
    }
    let text = current.text();
    let mut start = 0usize;
    let mut end = len;
    let mut seen = 0usize;
    let mut previous: Option<u8> = None;
    let mut found = false;
    for chunk in text.iter_chunks().ok()? {
        for (index, byte) in chunk.bytes().enumerate() {
            let at = seen.checked_add(index)?;
            if byte == b'\n' {
                if at < head {
                    start = at.checked_add(1)?;
                } else if !found {
                    end = at;
                    found = true;
                    if previous == Some(b'\r') {
                        end = end.checked_sub(1)?;
                    }
                }
            }
            previous = Some(byte);
        }
        seen = seen.checked_add(chunk.len())?;
    }
    (start <= end).then_some(start..end)
}

pub(super) struct SourceInputSite {
    identity: InputIdentity,
    site: InputSite,
}

fn resized(range: Range<usize>, before: usize, after: usize) -> Option<Range<usize>> {
    let end = if after >= before {
        range.end.checked_add(after.checked_sub(before)?)?
    } else {
        range.end.checked_sub(before.checked_sub(after)?)?
    };
    (range.start <= end).then_some(range.start..end)
}

struct InputView {
    site: InputSite,
    identity: InputIdentity,
    coordinates: NativeInputCoordinates,
}

impl InputView {
    fn new(document: &mut ReadonlyDocument, site: InputSite) -> Option<Self> {
        let (identity, current) = Self::current(document)?;
        Self::from_snapshot(site, identity, &current)
    }

    fn from_snapshot(
        site: InputSite,
        identity: InputIdentity,
        current: &CurrentSnapshot,
    ) -> Option<Self> {
        let range = current.text().src_range(site.source.clone()).ok()?;
        let coordinates =
            NativeInputCoordinates::new(identity.clone(), current.text(), &range, MAPPING).ok()?;
        let view = Self {
            site,
            identity,
            coordinates,
        };
        view.bound(&view.identity)?
            .utf16_from_source_selection(current.selection())
            .ok()?;
        Some(view)
    }

    fn current(document: &mut ReadonlyDocument) -> Option<(InputIdentity, CurrentSnapshot)> {
        if !document.is_editable() {
            return None;
        }
        let identity = document.input_identity().ok()?;
        let current = document.current_snapshot().ok()?;
        Some((identity, current))
    }

    fn bound(&self, identity: &InputIdentity) -> Option<BoundInputCoordinates<'_>> {
        self.coordinates
            .bind(identity, self.coordinates.projection_id())
            .ok()
    }

    fn active(
        &self,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        mode: EditorMode,
    ) -> Option<BoundInputCoordinates<'_>> {
        if current.snapshot_id() != identity.snapshot_id() || mode != self.site.mode {
            return None;
        }
        let bound = self.bound(identity)?;
        // 选区移动不推进工作版本。跨行或跨块后，旧投影仍须停止消费回调。
        bound
            .utf16_from_source_selection(current.selection())
            .ok()?;
        Some(bound)
    }

    fn marked(document: &ReadonlyDocument, current: &CurrentSnapshot) -> Option<SrcSelection> {
        let mark = document.core().composition()?;
        let start = mark.range().start;
        let end = start.checked_add(mark.candidate().len())?;
        SrcSelection::new(
            current.text().src_offset(start).ok()?,
            current.text().src_offset(end).ok()?,
        )
        .ok()
    }

    fn replacement(
        &self,
        current: &CurrentSnapshot,
        document: &ReadonlyDocument,
        range: Option<Range<usize>>,
        replacement_bytes: usize,
    ) -> Result<(), DocError> {
        let bound = self.bound(&self.identity).ok_or(DocError::StaleResult)?;
        let marked = Self::marked(document, current);
        let target = marked.as_ref().unwrap_or(current.selection());
        let target_units = bound
            .utf16_from_source_selection(target)
            .map_err(|_| DocError::OutsideOpenBlock)?;
        if let Some(range) = range
            && bound
                .utf16_range(range)
                .map_err(|_| DocError::OutsideOpenBlock)?
                != target_units
        {
            return Err(DocError::OutsideOpenBlock);
        }
        if marked.is_none() {
            bound
                .editable_selection(&target_units)
                .ok()
                .flatten()
                .ok_or(DocError::OutsideOpenBlock)?;
        }
        let candidate_bytes = bound
            .drawn_text()
            .len()
            .checked_sub(target.range().byte_range().len())
            .and_then(|remaining| remaining.checked_add(replacement_bytes))
            .ok_or(DocError::Buffer(BufferError::LengthOverflow))?;
        if candidate_bytes > MAX_INPUT_BYTES {
            return Err(DocError::Buffer(BufferError::BudgetExceeded {
                needed: candidate_bytes,
                limit: MAX_INPUT_BYTES,
            }));
        }
        Ok(())
    }

    /// 主线程的一次输入决策：只做 prepare 或既有同步路径，不解析。
    fn step(
        &self,
        intent: &InputIntent,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> InputStep {
        if self
            .active(identity, current, document.core().mode())
            .is_none()
        {
            return InputStep::Inactive;
        }
        match intent {
            // Escape 由窗口关闭路径处理，不经过视图投影。
            InputIntent::Escape => InputStep::Rejected(DocError::NoOpenBlock),
            InputIntent::LineBreak => {
                if document.core().composition().is_some() {
                    return InputStep::Rejected(DocError::CompositionActive);
                }
                if let Err(error) = self.replacement(current, document, None, 1) {
                    return InputStep::Rejected(error);
                }
                self.step_text(identity, current, document, "\n", now_millis)
            }
            InputIntent::Delete { forward, count } => {
                if document.core().composition().is_some() {
                    return InputStep::Rejected(DocError::CompositionActive);
                }
                let range = match self.delete_range(identity, current, *forward, *count) {
                    Ok(Some(range)) => range,
                    // 组合活动时删除已拒绝；到达边界不会改变组合状态。
                    Ok(None) => {
                        return InputStep::Applied {
                            changed: false,
                            composition: None,
                        };
                    }
                    Err(error) => return InputStep::Rejected(error),
                };
                let before = document.core().version();
                let prepared = document
                    .edit_core()
                    .and_then(|core| core.prepare_native_delete(identity, &range, now_millis));
                match prepared {
                    Ok(Some(job)) => InputStep::Prepared(job),
                    // 局部草稿内删除已同步完成；组合活动时删除已在上面拒绝，不会结束组合。
                    Ok(None) => InputStep::Applied {
                        changed: document.core().version() != before,
                        composition: None,
                    },
                    Err(error) => InputStep::Rejected(error),
                }
            }
            InputIntent::Move { .. } | InputIntent::Select(_) => {
                self.apply_checked(intent, current, document, now_millis)
            }
            InputIntent::Paste(text) => {
                if let Err(error) = self.replacement(current, document, None, text.len()) {
                    return InputStep::Rejected(error);
                }
                self.apply_checked(intent, current, document, now_millis)
            }
            InputIntent::Replace(range, text) => {
                if let Err(error) = self.replacement(current, document, range.clone(), text.len()) {
                    return InputStep::Rejected(error);
                }
                if document.core().composition().is_some() {
                    // 组合确认统一走后台冻结；发布段不重做候选拼接。
                    let prepared = document.edit_core().and_then(|core| {
                        core.prepare_native_confirmation(identity, Some(text), now_millis)
                    });
                    return match prepared {
                        Ok(job) => InputStep::Prepared(job),
                        Err(error) => InputStep::Rejected(error),
                    };
                }
                if text.is_empty() {
                    self.apply_checked(intent, current, document, now_millis)
                } else {
                    self.step_text(identity, current, document, text, now_millis)
                }
            }
            InputIntent::Mark(range, text, _) => {
                if let Err(error) = self.replacement(current, document, range.clone(), text.len()) {
                    return InputStep::Rejected(error);
                }
                if starts_composition(document, text) {
                    match self.prepare_seal(identity, current, document, now_millis) {
                        Ok(Some(job)) => InputStep::Sealed(job),
                        Ok(None) => self.recover(intent, current, document, now_millis),
                        // 冻结前置条件失败即拒绝：同步重试或退化会得到同一错误。
                        Err(error) => InputStep::Rejected(error),
                    }
                } else {
                    self.apply_checked(intent, current, document, now_millis)
                }
            }
        }
    }

    /// 普通文本输入：900ms 边界由核心判定，只有需要封口时才冻结后台 job。
    fn step_text(
        &self,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        text: &str,
        now_millis: u64,
    ) -> InputStep {
        let required = match document
            .edit_core()
            .and_then(|core| core.native_typing_commit_required(identity, text, now_millis))
        {
            Ok(required) => required,
            Err(error) => return InputStep::Rejected(error),
        };
        if !required {
            let intent = InputIntent::Replace(None, text.to_owned());
            return self.apply_checked(&intent, current, document, now_millis);
        }
        match self.prepare_seal(identity, current, document, now_millis) {
            Ok(Some(job)) => InputStep::Sealed(job),
            // 准备段判定需封口却拿不到 job：内部不一致，按 D2b.4 同步应用并反馈。
            Ok(None) => self.recover(
                &InputIntent::Replace(None, text.to_owned()),
                current,
                document,
                now_millis,
            ),
            // 冻结前置条件失败即拒绝：同步重试或退化会得到同一错误。
            Err(error) => InputStep::Rejected(error),
        }
    }

    /// 冻结「封口旧输入组」，并携带按键时刻的选区作为下一目标。
    fn prepare_seal(
        &self,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> Result<Option<NativeInputJob>, DocError> {
        let target = current.selection().range();
        document
            .edit_core()?
            .prepare_native_typing_commit(identity, now_millis, Some(&target))
    }

    /// 计算一次删除覆盖的连续源码范围；到达边界时返回 `None`。
    fn delete_range(
        &self,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        forward: bool,
        count: usize,
    ) -> Result<Option<SrcRange>, DocError> {
        let bound = self.bound(identity).ok_or(DocError::StaleResult)?;
        let units = bound
            .utf16_from_source_selection(current.selection())
            .map_err(|_| DocError::OutsideOpenBlock)?;
        let checked = bound
            .editable_selection(&units)
            .ok()
            .flatten()
            .ok_or(DocError::OutsideOpenBlock)?;
        let (anchor, head) = (checked.anchor().clone(), checked.head().clone());
        let (start, end) = if anchor.byte_offset() <= head.byte_offset() {
            (anchor, head)
        } else {
            (head, anchor)
        };
        let has_selection = start.byte_offset() != end.byte_offset();
        // 有选区时首步删除选区本身，其余步按方向外扩；无选区时全部外扩。
        let steps = count.max(1).saturating_sub(usize::from(has_selection));
        let (from, edge_forward) = if forward {
            (end.clone(), true)
        } else {
            (start.clone(), false)
        };
        let (edge, moved) = step_graphemes(&bound, from, edge_forward, steps);
        if !has_selection && moved == 0 {
            return Ok(None);
        }
        let selected = if forward {
            SrcSelection::new(start, edge)
        } else {
            SrcSelection::new(edge, end)
        }?;
        Ok(Some(selected.range()))
    }

    fn apply_checked(
        &self,
        intent: &InputIntent,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> InputStep {
        let was_composing = document.core().composition().is_some();
        let before = document.core().version();
        match intent.apply(self, current, document, now_millis) {
            Ok(()) => InputStep::Applied {
                changed: document.core().version() != before,
                composition: composition_end(
                    intent,
                    was_composing,
                    document.core().composition().is_some(),
                ),
            },
            Err(error) => InputStep::Rejected(error),
        }
    }

    /// 准备结果与判定不一致时的同步退化路径；调用方须报告反馈。
    fn recover(
        &self,
        intent: &InputIntent,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> InputStep {
        let was_composing = document.core().composition().is_some();
        let before = document.core().version();
        match intent.apply(self, current, document, now_millis) {
            Ok(()) => InputStep::Recovered {
                changed: document.core().version() != before,
                composition: composition_end(
                    intent,
                    was_composing,
                    document.core().composition().is_some(),
                ),
            },
            Err(error) => InputStep::Rejected(error),
        }
    }
}

/// 从 `edge` 沿方向前进最多 `steps` 个字素；返回终点与实际步数。
fn step_graphemes(
    bound: &BoundInputCoordinates<'_>,
    mut edge: SrcOffset,
    forward: bool,
    steps: usize,
) -> (SrcOffset, usize) {
    let mut moved = 0usize;
    while moved < steps {
        let Some(next) = bound.adjacent_grapheme(&edge, forward) else {
            break;
        };
        edge = next;
        moved += 1;
    }
    (edge, moved)
}

/// 非空 Mark 在无组合且有普通输入组时会开启组合；窗口先封口旧组。
fn starts_composition(document: &ReadonlyDocument, text: &str) -> bool {
    !text.is_empty()
        && document.core().composition().is_none()
        && document.core().open_block_draft().is_some()
}

/// 组合由本次输入结算结束时给出平台结束结果；未结束或结算前无组合返回 `None`。
///
/// 判定依据：空候选 `Mark` 在 `InputView::step` 中走 `composition_cancel`，属于
/// 平台取消；其余结束（确认提交、IME 替换确认等）都按平台确认继续原保存意图。
/// 只有「结算前组合活动且结算后结束」才产生结果，组合仍活动时窗口不得结算等待。
fn composition_end(
    intent: &InputIntent,
    was_composing: bool,
    is_composing: bool,
) -> Option<PublicationOutcome> {
    if !was_composing || is_composing {
        return None;
    }
    Some(
        if matches!(intent, InputIntent::Mark(_, text, _) if text.is_empty()) {
            PublicationOutcome::PlatformCancelled
        } else {
            PublicationOutcome::PlatformConfirmed
        },
    )
}

/// 自持输入意图：跨越在途 job 排队时不得借用平台回调文本。
#[derive(Clone)]
enum InputIntent {
    LineBreak,
    Delete { forward: bool, count: usize },
    Move { forward: bool, extend: bool },
    Replace(Option<Range<usize>>, String),
    Mark(Option<Range<usize>>, String, Option<Range<usize>>),
    Select(Range<usize>),
    Paste(String),
    Escape,
}

/// 帧不可消费时对一次输入的处置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputRecovery {
    /// 帧仍可消费，直接提交。
    Proceed,
    /// 显式命令类意图：按最新状态重建帧后继续按序应用。
    Rebind,
    /// 携带平台坐标的意图：不得换投影解释，静默拒收并保留上下文。
    Reject,
}

/// 输入结算是否清除现有反馈：只清输入类提示。
///
/// 保存失败、外部冲突、不确定发布与关闭/替换拒绝必须保留到用户处理，
/// 否则紧随输入回调的结算会把刚出现的保存提示抹掉。
fn input_feedback_cleared(key: &str) -> bool {
    !matches!(
        key,
        "document.save_failed" | "document.save_conflict" | "document.save_uncertain"
    ) && !key.starts_with("document.external_")
        && !key.starts_with("document.close_")
}

/// 判定一次输入在当前帧是否可继续，供窗口入口与三分类矩阵测试共用。
///
/// 平台 UTF-16 范围只对其注册的帧有效；命令推进工作版本后，同一数值在新投影上
/// 指向不同文本，重建后解释旧坐标会写到错误位置，因此必须拒收而不是重绑。
/// 只携带语义（按键、删除、移动、粘贴）的意图与平台坐标无关，可在重建后按序应用。
fn input_recovery(accepted: bool, intent: &InputIntent) -> InputRecovery {
    if accepted {
        return InputRecovery::Proceed;
    }
    match intent {
        InputIntent::Replace(..) | InputIntent::Mark(..) | InputIntent::Select(..) => {
            InputRecovery::Reject
        }
        InputIntent::LineBreak
        | InputIntent::Delete { .. }
        | InputIntent::Move { .. }
        | InputIntent::Paste(_)
        | InputIntent::Escape => InputRecovery::Rebind,
    }
}

/// 一次输入决策的结果；只有 Prepared/Sealed 需要后台 job。
enum InputStep {
    Applied {
        changed: bool,
        /// 本次同步结算是否结束了组合；窗口据此继续组合期保存等待。
        composition: Option<PublicationOutcome>,
    },
    /// 准备结果与判定不一致时同步应用；窗口须报告内部错误反馈。
    Recovered {
        changed: bool,
        /// 同步退化同样可能结束组合；窗口据此继续组合期保存等待。
        composition: Option<PublicationOutcome>,
    },
    Prepared(NativeInputJob),
    Sealed(NativeInputJob),
    Rejected(DocError),
    Inactive,
}

/// 一次调度的执行状态；Started 表示输入已交给后台 job。
#[derive(Clone, Copy, PartialEq, Eq)]
enum InputExecution {
    Applied,
    Started,
    Rejected,
    Inactive,
}

impl InputIntent {
    fn apply(
        &self,
        view: &InputView,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> Result<(), DocError> {
        let identity = &view.identity;
        let mode = document.core().mode();
        if view
            .active(&document.input_identity()?, current, mode)
            .is_none()
        {
            return Err(DocError::StaleResult);
        }
        let composing = document.core().composition().is_some();
        if view.site.mode == EditorMode::Source
            && let Some(draft) = document.core().open_block_draft()
        {
            let source_start = document
                .core()
                .open_block_range()
                .ok_or(DocError::NoOpenBlock)?
                .start;
            let source_end = source_start
                .checked_add(draft.len())
                .ok_or(DocError::OutsideOpenBlock)?;
            if source_start < view.site.source.start || source_end > view.site.source.end {
                return Err(DocError::OutsideOpenBlock);
            }
        }
        match self {
            Self::LineBreak => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                view.replacement(current, document, None, 1)?;
                document
                    .edit_core()?
                    .type_selection(identity, "\n", now_millis)
            }
            Self::Delete { forward, count } => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                let Some(range) = view.delete_range(identity, current, *forward, *count)? else {
                    return Ok(());
                };
                document
                    .edit_core()?
                    .delete_native_range(identity, &range, now_millis)
                    .map(|_| ())
            }
            Self::Move { forward, extend } => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                let selection = current.selection();
                let bound = view.bound(identity).ok_or(DocError::StaleResult)?;
                let head = if !*extend && !selection.range().is_empty() {
                    if *forward == selection.is_reversed() {
                        selection.anchor().clone()
                    } else {
                        selection.head().clone()
                    }
                } else {
                    let Some(head) = bound.adjacent_grapheme(selection.head(), *forward) else {
                        return Ok(());
                    };
                    head
                };
                let anchor = if *extend { selection.anchor() } else { &head };
                let selected = SrcSelection::new(anchor.clone(), head)?;
                document.edit_core()?.select(identity, &selected)
            }
            Self::Select(range) => {
                let bound = view.bound(identity).ok_or(DocError::StaleResult)?;
                let range = bound
                    .utf16_range(range.clone())
                    .map_err(|_| DocError::OutsideOpenBlock)?;
                let selection = bound
                    .editable_selection(&range)
                    .ok()
                    .flatten()
                    .ok_or(DocError::OutsideOpenBlock)?;
                document.edit_core()?.select(identity, &selection)
            }
            Self::Paste(text) => {
                view.replacement(current, document, None, text.len())?;
                document
                    .edit_core()?
                    .replace_selection(identity, text, now_millis)
                    .map(|_| ())
            }
            Self::Replace(range, text) => {
                view.replacement(current, document, range.clone(), text.len())?;
                let core = document.edit_core()?;
                if composing {
                    core.commit_native_candidate(identity, text, now_millis)
                        .map(|_| ())
                } else if text.is_empty() {
                    core.replace_selection(identity, text, now_millis)
                        .map(|_| ())
                } else {
                    core.type_selection(identity, text, now_millis)
                }
            }
            Self::Mark(range, text, selection) => {
                view.replacement(current, document, range.clone(), text.len())?;
                if text.is_empty() {
                    if selection.clone().is_some_and(|range| range != (0..0)) {
                        return Err(DocError::OutsideOpenBlock);
                    }
                    if composing {
                        return document.edit_core()?.composition_cancel(now_millis);
                    }
                    return Ok(());
                }
                if text.len() > MAX_INPUT_BYTES {
                    return Err(DocError::Buffer(BufferError::BudgetExceeded {
                        needed: text.len(),
                        limit: MAX_INPUT_BYTES,
                    }));
                }
                let selected = selection
                    .clone()
                    .map(|range| {
                        BoundInputCoordinates::candidate_selection(text, range, MAX_INPUT_BYTES)
                            .ok_or(DocError::OutsideOpenBlock)
                    })
                    .transpose()?;
                document
                    .edit_core()?
                    .update_native_candidate(identity, text, selected, now_millis)
            }
            // Escape 由窗口关闭路径处理，不经过视图投影。
            Self::Escape => Err(DocError::NoOpenBlock),
        }
    }
}

/// 单条自持输入意图及其按键时刻。
#[derive(Clone)]
pub(super) struct ScheduledInput {
    intent: InputIntent,
    now_millis: u64,
    retried: bool,
}

impl ScheduledInput {
    fn new(intent: InputIntent, now_millis: u64) -> Self {
        Self {
            intent,
            now_millis,
            retried: false,
        }
    }
}

/// 立即执行，或已在等待队列中（含合并）。
pub(super) enum InputSubmission {
    Now(ScheduledInput),
    Queued,
}

/// 同一个在途输入 job 的类别。
enum ActiveInput {
    /// job 自身提交该意图；发布后不重放。
    Direct(ScheduledInput),
    /// 封口 job；发布成功后把触发意图放回队首重放。
    Seal(ScheduledInput),
}

impl ActiveInput {
    fn input(&self) -> &ScheduledInput {
        match self {
            Self::Direct(input) | Self::Seal(input) => input,
        }
    }

    fn into_input(self) -> ScheduledInput {
        match self {
            Self::Direct(input) | Self::Seal(input) => input,
        }
    }

    fn is_seal(&self) -> bool {
        matches!(self, Self::Seal(_))
    }

    /// 标记该输入已重试过；后续失败不得再次重试。
    fn mark_retried(&mut self) {
        match self {
            Self::Direct(input) | Self::Seal(input) => input.retried = true,
        }
    }
}

/// 后台失败后的恢复动作；由纯判定给出，窗口薄壳只负责执行。
enum Recovery {
    /// 在同一最新状态上重试一次；判定时已标记为已重试。
    Retry,
    /// 同步退化：先应用触发意图，再按序应用等待队列。
    SyncFallback,
    /// 放弃活动输入并清空等待队列。
    Clear,
}

/// 薄壳观察到的失败场景；把窗口状态映射为纯输入，判定本身不依赖 GPUI。
#[derive(Clone, Copy)]
enum InputFailure {
    /// 文档仍在，后台 job 或重试失败。
    OpenDocument,
    /// 文档已关闭或替换；结果只丢弃。
    DocumentGone,
}

/// 不依赖 GPUI 的输入调度：至多一个在途 job，其余有序等待。
///
/// 顺序、连续同向删除合并与封口重放都在这里完成；窗口层只做 spawn/update_in 薄壳。
/// 无头测试可直接驱动本类型，不需要窗口或平台输入栈。
pub(super) struct InputScheduler {
    document: Option<DocumentId>,
    active: Option<ActiveInput>,
    pending: VecDeque<ScheduledInput>,
}

impl InputScheduler {
    pub(super) fn new() -> Self {
        Self {
            document: None,
            active: None,
            pending: VecDeque::new(),
        }
    }

    /// 先登记文档身份；关闭或替换后到达的输入不得进入新文档队列。
    pub(super) fn submit(
        &mut self,
        document: &DocumentId,
        input: ScheduledInput,
    ) -> InputSubmission {
        if self.document.as_ref() != Some(document) {
            self.document = Some(document.clone());
            self.active = None;
            self.pending.clear();
        }
        if self.active.is_none() && self.pending.is_empty() {
            return InputSubmission::Now(input);
        }
        self.enqueue(input);
        InputSubmission::Queued
    }

    fn enqueue(&mut self, input: ScheduledInput) {
        let merged = match self.pending.back_mut() {
            Some(back) => match (&mut back.intent, &input.intent) {
                (
                    InputIntent::Delete {
                        forward: back_forward,
                        count: back_count,
                    },
                    InputIntent::Delete { forward, count },
                ) if *back_forward == *forward => {
                    *back_count = back_count.saturating_add(*count);
                    back.now_millis = input.now_millis;
                    true
                }
                _ => false,
            },
            None => false,
        };
        if !merged {
            self.pending.push_back(input);
        }
    }

    pub(super) fn start_direct(&mut self, input: ScheduledInput) {
        self.active = Some(ActiveInput::Direct(input));
    }

    pub(super) fn start_seal(&mut self, input: ScheduledInput) {
        self.active = Some(ActiveInput::Seal(input));
    }

    fn take_active(&mut self) -> Option<ActiveInput> {
        self.active.take()
    }

    /// 失败后的纯恢复判定：先重试一次；再失败时封口退化、其余清空。
    ///
    /// `Retry` 会先把活动输入标记为已重试，薄壳按意图重新执行即可；
    /// `Clear` 会清空等待队列，薄壳只负责反馈；`SyncFallback` 保留队列供同步应用。
    fn on_failure(&mut self, active: &mut ActiveInput, scene: InputFailure) -> Recovery {
        if matches!(scene, InputFailure::DocumentGone) {
            self.pending.clear();
            return Recovery::Clear;
        }
        if !active.input().retried {
            active.mark_retried();
            return Recovery::Retry;
        }
        match active {
            ActiveInput::Seal(_) => Recovery::SyncFallback,
            ActiveInput::Direct(_) => {
                self.pending.clear();
                Recovery::Clear
            }
        }
    }

    pub(super) fn take_pending(&mut self) -> Option<ScheduledInput> {
        self.pending.pop_front()
    }

    pub(super) fn requeue_front(&mut self, input: ScheduledInput) {
        self.pending.push_front(input);
    }

    pub(super) fn clear(&mut self) {
        self.active = None;
        self.pending.clear();
    }

    pub(super) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(super) fn document(&self) -> Option<&DocumentId> {
        self.document.as_ref()
    }

    #[cfg(test)]
    pub(super) fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

struct InputGeometry {
    projection: ProjectionId,
    bounds: Bounds<Pixels>,
    layout: PreparedLayout,
    viewport: gpui::Size<Pixels>,
    scale: u32,
}

#[derive(Clone)]
pub(super) struct InputFrame(Rc<RefCell<Option<InputView>>>);

/// 输入行与阅读行共享的实际文本几何；只读投影和布局来自同一次 render。
/// 窗口弱引用只用于复验身份，不保活文档，也不拥有第二个滚动状态。
pub(super) struct InputRowLayout {
    pub(super) projection: Projection,
    pub(super) prepared: PreparedLayout,
    pub(super) text_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    identity: InputIdentity,
    host: WeakEntity<NativeDocumentWindow>,
}

fn input_geometry_matches(identity: &InputIdentity, document: &ReadonlyDocument) -> bool {
    let core = document.core();
    core.is_document_alive()
        && identity.document_id() == core.document_id()
        && identity.working_version() == core.version()
        && identity.parse_config_generation() == core.parse_config_generation()
        && identity.request_id() == core.derived_request_id()
}

impl InputRowLayout {
    pub(super) fn matches(&self, document: &ReadonlyDocument) -> bool {
        input_geometry_matches(&self.identity, document)
    }

    pub(super) fn is_current(&self, cx: &App) -> bool {
        self.host
            .read_with(cx, |host, _| {
                host.state
                    .session
                    .document()
                    .is_some_and(|document| self.matches(document))
            })
            .unwrap_or(false)
    }
}

impl InputFrame {
    /// 在最新快照上重建帧内局部投影；没有可重建站点或站点失效时清空该帧。
    ///
    /// 显式命令不经过调度：在途 job 因命令过期后，重试与同步退化取帧前都依赖
    /// 本方法把旧投影换到当前身份与选区，而不是直接判为不可消费。
    fn rebind(&self, document: &mut ReadonlyDocument) -> bool {
        let Ok(mut active) = self.0.try_borrow_mut() else {
            return false;
        };
        let view = active.take().and_then(|previous| {
            let site = previous.site.rebound(document)?;
            InputView::new(document, site)
        });
        *active = view;
        active.is_some()
    }

    /// 清空帧内视图；之后只能由重新渲染或显式重建恢复。
    fn clear(&self) {
        if let Ok(mut active) = self.0.try_borrow_mut() {
            *active = None;
        }
    }

    /// 当前 Source 绑定；供列表延迟重建与输入行命中复用。
    fn source_site(&self) -> Option<SourceInputSite> {
        let active = self.0.try_borrow().ok()?;
        let view = active.as_ref()?;
        (view.site.mode == EditorMode::Source).then(|| SourceInputSite {
            identity: view.identity.clone(),
            site: view.site.clone(),
        })
    }

    /// 帧内视图是否仍能消费当前文档状态；显式命令推进版本或移动选区后为 false。
    fn accepts(&self, document: &mut ReadonlyDocument) -> bool {
        let Ok(active) = self.0.try_borrow() else {
            return false;
        };
        let Some(view) = active.as_ref() else {
            return false;
        };
        let Some((identity, current)) = InputView::current(document) else {
            return false;
        };
        view.active(&identity, &current, document.core().mode())
            .is_some()
    }
}

struct DocumentInput {
    host: WeakEntity<NativeDocumentWindow>,
    focus: FocusHandle,
    view: InputFrame,
    geometry: Option<InputGeometry>,
}

/// 组合活动时保留离屏输入帧；否则释放，避免组合预览与输入绑定滞留。
fn composing_frame(frame: Option<&InputFrame>, composing: bool) -> Option<InputFrame> {
    match frame {
        Some(frame) => composing.then(|| frame.clone()),
        None => None,
    }
}

impl DocumentInput {
    fn with_current<T>(
        &self,
        window: &Window,
        cx: &mut App,
        query: impl FnOnce(
            &InputView,
            &BoundInputCoordinates<'_>,
            &CurrentSnapshot,
            &ReadonlyDocument,
        ) -> Option<T>,
    ) -> Option<T> {
        if !self.focus.is_focused(window) {
            return None;
        }
        let active = self.view.0.try_borrow().ok()?;
        let view = active.as_ref()?;
        self.host
            .update(cx, |host, _| {
                let document = host.state.session.document_mut()?;
                let (identity, current) = InputView::current(document)?;
                let bound = view.active(&identity, &current, document.core().mode())?;
                query(view, &bound, &current, document)
            })
            .ok()
            .flatten()
    }

    fn apply(&mut self, intent: InputIntent, window: &Window, cx: &mut App) {
        if self.focus.is_focused(window) {
            let _ = self.host.update(cx, |host, cx| {
                host.apply_native_input(&self.view, intent, window, cx);
            });
        }
    }
}

impl NativeDocumentWindow {
    pub(super) fn document_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.alt
            || modifiers.platform
            || modifiers.control
            || modifiers.function
            || !self.focus.is_focused(window)
        {
            return;
        }
        let intent = match event.keystroke.key.as_str() {
            "enter" => InputIntent::LineBreak,
            "backspace" => InputIntent::Delete {
                forward: false,
                count: 1,
            },
            "delete" => InputIntent::Delete {
                forward: true,
                count: 1,
            },
            "left" => InputIntent::Move {
                forward: false,
                extend: modifiers.shift,
            },
            "right" => InputIntent::Move {
                forward: true,
                extend: modifiers.shift,
            },
            _ => return,
        };
        if let Some(frame) = self.input_frame.clone()
            && self.apply_native_input(&frame, intent, window, cx)
        {
            cx.stop_propagation();
        }
    }

    /// 提交一次输入意图：空闲时立即执行，在途时入队（连续同向删除合并）。
    ///
    /// 显式命令可能先推进版本或移动选区而使帧失效；命令类意图先按最新状态重建
    /// 再提交，携带平台坐标的意图（Replace/Mark/Select）则按过期回调静默拒收：
    /// 不重建、不提交、不设反馈，保留现有上下文，避免旧坐标按新投影解释。
    /// 命令类意图无法重建时不再静默忽略：文档仍在且可编辑时反馈
    /// `document.input_dropped`，文档已关闭或只读时按 D3.6 不反馈。
    fn apply_native_input(
        &mut self,
        frame: &InputFrame,
        intent: InputIntent,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match input_recovery(self.frame_accepts(frame), &intent) {
            InputRecovery::Proceed => {}
            // 平台坐标只对其注册帧有效；命令推进版本后必须拒收。
            InputRecovery::Reject => return false,
            InputRecovery::Rebind => {
                if !self.rebind_frame(frame) {
                    if self
                        .state
                        .session
                        .document()
                        .is_some_and(ReadonlyDocument::is_editable)
                    {
                        self.state.feedback = Some("document.input_dropped");
                        cx.notify();
                    }
                    return false;
                }
            }
        }
        let Some(document) = self.state.session.document_mut() else {
            return false;
        };
        let document = document.core().document_id().clone();
        let input = ScheduledInput::new(intent, self.input_millis());
        match self.input_scheduler.submit(&document, input) {
            InputSubmission::Queued => true,
            InputSubmission::Now(input) => !matches!(
                self.execute_step(frame, input, window, cx),
                InputExecution::Inactive
            ),
        }
    }

    /// 主线程执行一次已登记输入；准备、同步应用或启动后台 job。
    fn execute_step(
        &mut self,
        frame: &InputFrame,
        input: ScheduledInput,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> InputExecution {
        if matches!(&input.intent, InputIntent::Escape) {
            return self.execute_escape(input, window, cx);
        }
        let step = {
            let Ok(active) = frame.0.try_borrow() else {
                return InputExecution::Inactive;
            };
            let Some(view) = active.as_ref() else {
                return InputExecution::Inactive;
            };
            let Some(document) = self.state.session.document_mut() else {
                return InputExecution::Inactive;
            };
            let Some((identity, current)) = InputView::current(document) else {
                return InputExecution::Inactive;
            };
            let mode = document.core().mode();
            if view.active(&identity, &current, mode).is_none() {
                return InputExecution::Inactive;
            }
            view.step(
                &input.intent,
                &identity,
                &current,
                document,
                input.now_millis,
            )
        };
        match step {
            InputStep::Applied {
                changed,
                composition,
            } => {
                self.settle_after_input(Some(frame), changed, composition, window, cx);
                InputExecution::Applied
            }
            InputStep::Recovered {
                changed,
                composition,
            } => {
                self.settle_after_input(Some(frame), changed, composition, window, cx);
                self.state.feedback = Some("document.input_applied_after_failure");
                cx.notify();
                InputExecution::Applied
            }
            InputStep::Prepared(job) => {
                self.input_scheduler.start_direct(input);
                self.start_input_job(job, window, cx);
                InputExecution::Started
            }
            InputStep::Sealed(job) => {
                self.input_scheduler.start_seal(input);
                self.start_input_job(job, window, cx);
                InputExecution::Started
            }
            InputStep::Rejected(error) => {
                self.state.feedback = Some(edit_error_key(error));
                cx.notify();
                InputExecution::Rejected
            }
            InputStep::Inactive => InputExecution::Inactive,
        }
    }

    /// Escape 冻结「提交待定组 → 关闭块」后转后台；发布段不再解析。
    fn execute_escape(
        &mut self,
        input: ScheduledInput,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> InputExecution {
        let prepared = {
            let Some(document) = self.state.session.document_mut() else {
                self.input_scheduler.clear();
                return InputExecution::Inactive;
            };
            match document.input_identity() {
                Ok(identity) => document
                    .edit_core()
                    .and_then(|core| core.prepare_native_close(&identity, input.now_millis)),
                Err(error) => Err(error),
            }
        };
        match prepared {
            Ok(job) => {
                self.input_scheduler.start_direct(input);
                self.start_input_job(job, window, cx);
                InputExecution::Started
            }
            Err(error) => {
                self.state.feedback = Some(edit_error_key(error));
                cx.notify();
                InputExecution::Rejected
            }
        }
    }

    /// 把冻结好的输入 job 交给后台执行器；结果回到主线程发布。
    fn start_input_job(&mut self, job: NativeInputJob, window: &Window, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.input_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let result = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.input_task = None;
                this.finish_input_job(result, window, cx);
            });
        }));
    }

    /// 后台结果发布：独立输入就地结束，封口则重放触发意图并继续排水。
    fn finish_input_job(
        &mut self,
        result: Result<PreparedNativeInput, DocError>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.input_scheduler.take_active() else {
            return;
        };
        let document = self
            .state
            .session
            .document()
            .map(|document| document.core().document_id().clone());
        if document.as_ref() != self.input_scheduler.document() {
            // 文档已关闭或替换；旧结果不得进入新文档。
            self.input_scheduler.clear();
            return;
        }
        if matches!(active.input().intent, InputIntent::Escape) {
            match result {
                Ok(prepared) => match self.publish_native(prepared) {
                    Ok(_) => self.finish_escape(window, cx),
                    Err(error) => self.retry_or_fallback(active, error, window, cx),
                },
                Err(error) => self.retry_or_fallback(active, error, window, cx),
            }
            return;
        }
        let settled = match result {
            Ok(prepared) => self.publish_native(prepared),
            Err(error) => Err(error),
        };
        match settled {
            Ok((changed, was_composing, is_composing)) => {
                let composition =
                    composition_end(&active.input().intent, was_composing, is_composing);
                if active.is_seal() {
                    // 触发意图尚未应用：放回队首，在新版本上重放。
                    self.input_scheduler.requeue_front(active.into_input());
                }
                let frame = self.active_input_frame();
                self.settle_after_input(frame.as_ref(), changed, composition, window, cx);
                self.drain_inputs(window, cx);
            }
            Err(error) => self.retry_or_fallback(active, error, window, cx),
        }
    }

    /// 在最新文档状态上发布后台结果；返回版本是否变化及发布前后的组合状态。
    fn publish_native(
        &mut self,
        prepared: PreparedNativeInput,
    ) -> Result<(bool, bool, bool), DocError> {
        let Some(document) = self.state.session.document_mut() else {
            return Err(DocError::ClosedTarget);
        };
        let before = document.core().version();
        let was_composing = document.core().composition().is_some();
        document.edit_core()?.publish_native_input(prepared)?;
        Ok((
            document.core().version() != before,
            was_composing,
            document.core().composition().is_some(),
        ))
    }

    /// 发布或同步应用后的统一收尾：刷新、重绑、组合帧、保存等待与通知。
    fn settle_after_input(
        &mut self,
        frame: Option<&InputFrame>,
        changed: bool,
        composition: Option<PublicationOutcome>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // 输入结算只清输入类反馈；保存/冲突/关闭提示保留到用户处理。
        if self.state.feedback.is_some_and(input_feedback_cleared) {
            self.state.feedback = None;
        }
        if changed {
            self.refresh_find(cx);
            // 待定组可跨原行展开。输入仍用原虚拟行的绑定，封口再采纳新列表。
            let has_draft = self
                .state
                .session
                .document()
                .is_some_and(|document| document.core().open_block_draft().is_some());
            if !has_draft {
                self.refresh_after_edit(window, cx);
            }
            if let Some(frame) = frame {
                self.rebind_frame(frame);
            }
        }
        // 每次输入结算都重新布防自动保存；组合结束/取消可能 changed == false，
        // 也必须靠本次结算恢复 1 秒计时。
        self.note_document_activity(window, cx);
        self.refresh_composing_input_frame(frame);
        // 组合自然结束后立即以该结束结果结算组合期保存等待并继续原保存意图。
        // 组合仍活动时 `composition` 必为 None，这里不会误结算。
        if let Some(outcome) = composition
            && self.state.save_waiting()
        {
            match self
                .state
                .finish_save_wait_intent(outcome, self.input_millis())
            {
                Some(SaveIntent::Save(job)) => self.start_save(job, window, cx),
                Some(SaveIntent::Migrate(job)) => self.start_association(job, window, cx),
                Some(SaveIntent::ChooseTarget(_)) => {
                    // 等待必有目标；选择器不可达，按内部失败反馈且不改菜单。
                    self.state.feedback = Some("document.save_failed");
                }
                // 等待已由协调器按拒绝/取消路径结算，保留其反馈。
                None => {}
            }
        }
        cx.notify();
    }

    /// 按当前组合状态同步离屏输入帧；组合结束即释放，避免预览滞留。
    fn refresh_composing_input_frame(&mut self, frame: Option<&InputFrame>) {
        let composing = self
            .state
            .session
            .document()
            .is_some_and(|document| document.core().composition().is_some());
        self.composing_input_frame = composing_frame(frame, composing);
    }

    /// 在最新快照上重建局部投影；失败时清空该帧（视图已失效）。
    fn rebind_frame(&mut self, frame: &InputFrame) -> bool {
        let rebound = match self.state.session.document_mut() {
            Some(document) => frame.rebind(document),
            None => {
                frame.clear();
                false
            }
        };
        self.source_input_site = frame.source_site();
        rebound
    }

    /// 帧内视图是否仍能消费当前文档状态；显式命令推进版本或移动选区后为 false。
    fn frame_accepts(&mut self, frame: &InputFrame) -> bool {
        self.state
            .session
            .document_mut()
            .is_some_and(|document| frame.accepts(document))
    }

    /// 当前可消费输入的帧；已有帧失效时先按最新状态重建，Source 列表延迟重建时从最近绑定恢复。
    fn active_input_frame(&mut self) -> Option<InputFrame> {
        if let Some(frame) = self.input_frame.clone() {
            // 显式命令可能已推进版本或移动选区；恢复前先重建，避免旧帧被判 Inactive。
            if self.frame_accepts(&frame) || self.rebind_frame(&frame) {
                return Some(frame);
            }
            return None;
        }
        let site = self.source_input_site.as_ref()?.site.clone();
        let document = self.state.session.document_mut()?;
        let view = InputView::new(document, site)?;
        let frame = InputFrame(Rc::new(RefCell::new(Some(view))));
        self.input_frame = Some(frame.clone());
        Some(frame)
    }

    /// 依次消费等待队列；遇到新的后台 job 即停，由发布收尾继续。
    fn drain_inputs(&mut self, window: &Window, cx: &mut Context<Self>) {
        loop {
            if self.input_scheduler.is_active() {
                return;
            }
            let Some(input) = self.input_scheduler.take_pending() else {
                return;
            };
            let Some(frame) = self.active_input_frame() else {
                self.input_scheduler.clear();
                self.state.feedback = Some("document.input_dropped");
                cx.notify();
                return;
            };
            match self.execute_step(&frame, input, window, cx) {
                InputExecution::Applied | InputExecution::Rejected => {}
                InputExecution::Started => return,
                InputExecution::Inactive => {
                    self.input_scheduler.clear();
                    self.state.feedback = Some("document.input_dropped");
                    cx.notify();
                    return;
                }
            }
        }
    }

    /// 失败后执行纯判定给出的恢复动作；重试仅一次，绝不静默丢输入。
    ///
    /// 显式命令不经过调度，可能在 job 在途时推进版本或移动选区；重试取帧时
    /// `active_input_frame` 会先按最新文档状态重建失效帧，使触发键与排队输入仍能按序应用。
    fn retry_or_fallback(
        &mut self,
        mut active: ActiveInput,
        error: DocError,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let scene = if self.state.session.document().is_none() {
            InputFailure::DocumentGone
        } else {
            InputFailure::OpenDocument
        };
        let mut retried_now = false;
        loop {
            let had_pending = self.input_scheduler.has_pending();
            match self.input_scheduler.on_failure(&mut active, scene) {
                Recovery::Retry => {
                    retried_now = true;
                    let escape = matches!(active.input().intent, InputIntent::Escape);
                    let retried = active.input().clone();
                    let started = if escape {
                        matches!(
                            self.execute_escape(retried, window, cx),
                            InputExecution::Started
                        )
                    } else {
                        match self.active_input_frame() {
                            Some(frame) => match self.execute_step(&frame, retried, window, cx) {
                                InputExecution::Started => true,
                                InputExecution::Applied => {
                                    self.drain_inputs(window, cx);
                                    true
                                }
                                InputExecution::Rejected | InputExecution::Inactive => false,
                            },
                            None => false,
                        }
                    };
                    if started {
                        return;
                    }
                }
                Recovery::SyncFallback => {
                    // D2b.5：仅此拒绝路径允许一次同步应用，保证触发键不丢。
                    let applied = self.replay_pending_sync(active.into_input(), window, cx);
                    if !applied {
                        self.input_scheduler.clear();
                    }
                    self.state.feedback = Some(if applied {
                        "document.input_applied_after_failure"
                    } else {
                        "document.input_dropped"
                    });
                    cx.notify();
                    return;
                }
                Recovery::Clear => {
                    if matches!(scene, InputFailure::DocumentGone) {
                        return;
                    }
                    let escape = matches!(active.input().intent, InputIntent::Escape);
                    if escape && !retried_now {
                        // 入口即已重试的关闭意图再次失败：报告本次失败。
                        self.state.feedback = Some(edit_error_key(error));
                    } else if had_pending || !escape {
                        self.state.feedback = Some("document.input_dropped");
                    }
                    cx.notify();
                    return;
                }
            }
        }
    }

    /// 同步退化：先应用封口触发键，再按序应用队列；任一失败即停止并报告。
    ///
    /// 取帧同样经过 `active_input_frame` 的重建，避免显式命令作废投影后整队被判失效。
    fn replay_pending_sync(
        &mut self,
        first: ScheduledInput,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(frame) = self.active_input_frame() else {
            self.composing_input_frame = None;
            return false;
        };
        let mut next = Some(first);
        let applied = 'replay: loop {
            let input = match next.take() {
                Some(input) => input,
                None => match self.input_scheduler.take_pending() {
                    Some(input) => input,
                    None => break 'replay true,
                },
            };
            let changed = {
                let Ok(active) = frame.0.try_borrow() else {
                    break 'replay false;
                };
                let Some(view) = active.as_ref() else {
                    break 'replay false;
                };
                let Some(document) = self.state.session.document_mut() else {
                    break 'replay false;
                };
                let Some((identity, current)) = InputView::current(document) else {
                    break 'replay false;
                };
                if view
                    .active(&identity, &current, document.core().mode())
                    .is_none()
                {
                    break 'replay false;
                }
                let before = document.core().version();
                if input
                    .intent
                    .apply(view, &current, document, input.now_millis)
                    .is_err()
                {
                    break 'replay false;
                }
                document.core().version() != before
            };
            if changed {
                self.refresh_find(cx);
                let has_draft = self
                    .state
                    .session
                    .document()
                    .is_some_and(|document| document.core().open_block_draft().is_some());
                if !has_draft {
                    self.refresh_after_edit(window, cx);
                }
                if !self.rebind_frame(&frame) {
                    break 'replay !self.input_scheduler.has_pending();
                }
            }
        };
        // 同步退化可能开始或结束组合；刷新离屏帧，避免组合预览滞留或缺失。
        self.refresh_composing_input_frame(Some(&frame));
        applied
    }

    /// Escape 发布后的收尾：关闭块会移除正文帧，必要时刷新列表。
    fn finish_escape(&mut self, window: &Window, cx: &mut Context<Self>) {
        let refresh = self
            .state
            .session
            .document()
            .is_some_and(|document| !document.is_parse_current());
        self.input_frame = None;
        self.source_input_site = None;
        self.visual_input_restore = None;
        self.composing_input_frame = None;
        self.state.feedback = None;
        if refresh {
            self.refresh_find(cx);
            self.refresh_after_edit(window, cx);
        }
        if self.input_scheduler.has_pending() {
            self.input_scheduler.clear();
            self.state.feedback = Some("document.input_dropped");
        }
        cx.notify();
    }

    /// 关闭或替换文档时清除全部未完成输入；在途结果由文档身份校验拦截。
    pub(super) fn clear_input_state(&mut self) {
        self.input_scheduler.clear();
        self.input_task = None;
        self.input_frame = None;
        self.source_input_site = None;
        self.visual_input_restore = None;
        self.composing_input_frame = None;
    }

    fn input_millis(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(super) fn escape_document_input(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) {
            return false;
        }
        let Some(document) = self
            .state
            .session
            .document_mut()
            .filter(|document| document.is_editable() && document.core().has_open_block())
        else {
            return false;
        };
        let document = document.core().document_id().clone();
        let input = ScheduledInput::new(InputIntent::Escape, self.input_millis());
        match self.input_scheduler.submit(&document, input) {
            InputSubmission::Queued => {
                cx.notify();
                true
            }
            InputSubmission::Now(input) => {
                let _ = self.execute_escape(input, window, cx);
                true
            }
        }
    }

    pub(super) fn move_window_focus(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus.is_focused(window)
            && self
                .state
                .session
                .document()
                .is_some_and(|doc| doc.core().composition().is_some())
        {
            self.state.feedback = Some("document.composition_pending");
            cx.notify();
            return;
        }
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
    }

    pub(super) fn offscreen_input(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let frame = self.composing_input_frame.clone()?;
        let focus = self.focus.clone();
        let host = cx.entity().downgrade();
        Some(
            canvas(
                |_, _, _| (),
                move |_, (), window, cx| {
                    if focus.is_focused(window) {
                        window.handle_input(
                            &focus,
                            DocumentInput {
                                host,
                                focus: focus.clone(),
                                view: frame,
                                geometry: None,
                            },
                            cx,
                        );
                    }
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}

impl InputHandler for DocumentInput {
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.with_current(window, cx, |_, bound, current, _| {
            Some(UTF16Selection {
                range: bound
                    .utf16_from_source_selection(current.selection())
                    .ok()?
                    .code_unit_range(),
                reversed: current.selection().is_reversed(),
            })
        })
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.with_current(window, cx, |_, bound, current, doc| {
            let selected = InputView::marked(doc, current)?;
            Some(
                bound
                    .utf16_from_source_selection(&selected)
                    .ok()?
                    .code_unit_range(),
            )
        })
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.with_current(window, cx, |_, bound, current, doc| {
            let checked = bound.utf16_range(range.clone()).ok()?;
            let source = bound.source_from_utf16(&checked).ok()?;
            let text = current
                .text()
                .slice(&source, doc.text_budget())
                .ok()?
                .into_owned();
            // vendored GPUI 把该回填写入 `NSTextInputClient` 的
            // `attributedSubstring(forProposedRange:actualRange:)` 的 `actualRange`
            // （`vendor/gpui-pre-0.3.9/src/platform.rs:2255-2268` 的注释）；返回
            // 实际范围即请求范围是契约允许的实现。真实 IME 对「请求与返回不一致」
            // 的行为未取证，故不回填其他范围，也不据此声称已覆盖平台契约。
            *adjusted = Some(range);
            Some(text)
        })
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.apply(InputIntent::Replace(range, text.to_owned()), window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.apply(
            InputIntent::Mark(range, text.to_owned(), selection),
            window,
            cx,
        );
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut App) {
        // 当前 macOS 探针未观察到独立 unmark 的自然结果。保持组合等待平台明确替换。
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut App) {
        if let Some(text) = item.text() {
            self.apply(InputIntent::Paste(text), window, cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.with_current(window, cx, |view, bound, _, _| {
            let geometry = self.geometry.as_ref()?;
            if view.coordinates.projection_id() != &geometry.projection
                || window.viewport_size() != geometry.viewport
                || window.scale_factor().to_bits() != geometry.scale
            {
                return None;
            }
            let drawn = bound
                .drawn_from_utf16(&bound.utf16_range(range).ok()?)
                .ok()?;
            let bytes = drawn.byte_range();
            let start = geometry.layout.position_for_index(bytes.start).ok()??;
            let end = geometry.layout.position_for_index(bytes.end).ok()??;
            let width = if start.y == end.y {
                (end.x - start.x).abs().max(px(1.0))
            } else {
                px(1.0)
            };
            let x = if start.y == end.y {
                start.x.min(end.x)
            } else {
                start.x
            };
            Some(Bounds::new(point(x, start.y), size(width, px(LINE_HEIGHT))))
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.with_current(window, cx, |view, bound, _, _| {
            let geometry = self.geometry.as_ref()?;
            if view.coordinates.projection_id() != &geometry.projection
                || window.viewport_size() != geometry.viewport
                || window.scale_factor().to_bits() != geometry.scale
            {
                return None;
            }
            geometry.bounds.localize(&point)?;
            let byte = geometry.layout.index_for_position(point).ok()?.ok()?;
            Some(
                bound
                    .utf16_from_drawn(&bound.drawn_offset(byte).ok()?)
                    .ok()?
                    .code_units(),
            )
        })
    }

    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        self.apply(InputIntent::Select(range), window, cx);
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.with_current(window, cx, |_, bound, _, _| {
            Some(
                bound
                    .utf16_from_drawn(&bound.drawn_offset(bound.drawn_text().len()).ok()?)
                    .ok()?
                    .code_units(),
            )
        })
    }

    fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.with_current(window, cx, |_, _, _, _| Some(()))
            .is_some()
    }
}

pub(super) fn render(
    document: &mut ReadonlyDocument,
    site: InputSite,
    focus: FocusHandle,
    cx: &Context<NativeDocumentWindow>,
) -> Option<(AnyElement, InputFrame, InputRowLayout)> {
    let (_, current) = InputView::current(document)?;
    let view = InputView::new(document, site)?;
    let bound = view.bound(&view.identity)?;
    let styled = StyledText::new(gpui::SharedString::from(bound.drawn_text().to_owned()));
    let selected = bound
        .utf16_from_source_selection(current.selection())
        .ok()?;
    let selected = bound.drawn_from_utf16(&selected).ok()?.byte_range();
    let marked = InputView::marked(document, &current).and_then(|selected| {
        let selected = bound.utf16_from_source_selection(&selected).ok()?;
        Some(bound.drawn_from_utf16(&selected).ok()?.byte_range())
    });
    let caret = if current.selection().is_reversed() {
        selected.start
    } else {
        selected.end
    };
    let layout = PreparedLayout::new(styled.layout().clone());
    let text_bounds = Rc::new(Cell::new(None));
    let layout_bounds = text_bounds.clone();
    let row_layout = InputRowLayout {
        projection: bound.projection().clone(),
        prepared: layout.clone(),
        text_bounds,
        identity: view.identity.clone(),
        host: cx.entity().downgrade(),
    };
    let prepainted = layout.clone();
    let host = cx.entity().downgrade();
    let click_host = host.clone();
    let click_layout = layout.clone();
    let click_focus = focus.clone();
    let click_identity = view.identity.clone();
    let click_coordinates = view.coordinates.clone();
    let projection = view.coordinates.projection_id().clone();
    let frame = InputFrame(Rc::new(RefCell::new(Some(view))));
    let handler_frame = frame.clone();
    Some((
        div()
            .relative()
            .w_full()
            .min_h(px(LINE_HEIGHT))
            .line_height(px(LINE_HEIGHT))
            .child(styled)
            .on_children_prepainted(move |bounds, _, _| {
                prepainted.mark_prepainted();
                layout_bounds.set(bounds.first().copied());
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let Some(byte) = click_layout
                    .index_for_position(event.position)
                    .ok()
                    .and_then(Result::ok)
                else {
                    return;
                };
                let applied = click_host.update(cx, |host, cx| {
                    let document = host.state.session.document_mut()?;
                    let (now, current) = InputView::current(document)?;
                    click_identity.validate(&now).ok()?;
                    let bound = click_coordinates
                        .bind(&now, click_coordinates.projection_id())
                        .ok()?;
                    bound
                        .utf16_from_source_selection(current.selection())
                        .ok()?;
                    let utf16 = bound
                        .utf16_from_drawn(&bound.drawn_offset(byte).ok()?)
                        .ok()?
                        .code_units();
                    let range = bound.utf16_range(utf16..utf16).ok()?;
                    let selection = bound.editable_selection(&range).ok()??;
                    document.edit_core().ok()?.select(&now, &selection).ok()?;
                    cx.notify();
                    Some(())
                });
                if let Ok(Some(())) = applied {
                    click_focus.focus(window, cx);
                }
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        if !focus.is_focused(window) {
                            return;
                        }
                        for (range, color) in marked
                            .into_iter()
                            .map(|range| (range, tokens::ACCENT))
                            .chain(
                                (selected.start != selected.end)
                                    .then_some((selected, tokens::BORDER)),
                            )
                        {
                            let start = layout.position_for_index(range.start).ok().flatten();
                            let end = layout.position_for_index(range.end).ok().flatten();
                            if let (Some(start), Some(end)) = (start, end)
                                && start.y == end.y
                            {
                                let width = (end.x - start.x).abs();
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(start.x.min(end.x), start.y + px(LINE_HEIGHT - 2.0)),
                                        size(width, px(2.0)),
                                    ),
                                    rgb(color),
                                ));
                            }
                        }
                        if let Ok(Some(caret)) = layout.position_for_index(caret) {
                            window.paint_quad(fill(
                                Bounds::new(caret, size(px(1.5), px(LINE_HEIGHT))),
                                rgb(tokens::ACCENT),
                            ));
                        }
                        let handler = DocumentInput {
                            host,
                            focus: focus.clone(),
                            view: handler_frame,
                            geometry: Some(InputGeometry {
                                projection,
                                bounds,
                                layout,
                                viewport: window.viewport_size(),
                                scale: window.scale_factor().to_bits(),
                            }),
                        };
                        window.handle_input(&focus, handler, cx);
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element(),
        frame,
        row_layout,
    ))
}

#[cfg(test)]
mod tests;
