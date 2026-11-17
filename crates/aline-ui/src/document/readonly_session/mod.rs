//! 单窗口文档打开生命周期；唯一持有 Ready 文档与至多一个打开请求。
//!
//! 输入为文件选择结果和后台完成候选，输出为当前只读文档与明确结算结果。
//! 请求资格在候选发布前复验；失败/取消保留原文档。本模块不调度线程、不写盘。
//! 读取适配器见 read 子模块，实际窗口宿主负责有限并发与任务回收。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aline_markdown::{CancelToken, ParseBudget, ParseConfig, Parsed};
use aline_source::{SourceBaseline, StorageBudget};

use super::{
    DocCore, DocError, DocLimits, EditorMode, InputIdentity, ModeSwitchOutcome, WaitStatus,
};
use aline_buffer::SrcSelection;

mod read;
mod refresh;
mod timing;
pub use read::{ReadonlyFileAccess, ReadonlyFileInput};
pub use refresh::{ParseRefreshCompletion, ParseRefreshDisposition, ParseRefreshJob};
pub(crate) use timing::{StageTimer, StageTiming};

/// 打开输入、文档与解析的显式注入预算；默认只读，可为 M2 选择可编辑访问。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadonlyOpenLimits {
    storage: StorageBudget,
    document: DocLimits,
    config: ParseConfig,
    parse: ParseBudget,
    editable: bool,
}

impl ReadonlyOpenLimits {
    /// 构造完整打开预算；均为宿主注入值，不代表已验证的发布限额。
    pub const fn new(
        storage: StorageBudget,
        document: DocLimits,
        config: ParseConfig,
        parse: ParseBudget,
    ) -> Self {
        Self {
            storage,
            document,
            config,
            parse,
            editable: false,
        }
    }

    /// 将此组打开预算用于 M2 可编辑文档；历史预算仍由宿主显式注入。
    pub const fn with_editing(mut self) -> Self {
        self.editable = true;
        self
    }
}

/// 打开请求的实例身份；不能由路径或裸整数构造。
#[derive(Clone)]
pub struct OpenRequestId {
    session: Arc<()>,
    generation: u64,
}
impl PartialEq for OpenRequestId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session) && self.generation == other.generation
    }
}
impl Eq for OpenRequestId {}
impl fmt::Debug for OpenRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("OpenRequestId")
            .field(&self.generation)
            .finish()
    }
}

/// 当前打开请求的实际阶段；Ready 文档与加载状态可以并存。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadonlyOpenPhase {
    /// 等待本地文件选择。
    Selecting,
    /// 受限读取、解码与解析候选；尚未发布。
    Loading,
}

struct PendingOpen {
    id: OpenRequestId,
    cancel: CancelToken,
    phase: ReadonlyOpenPhase,
}

/// 打开失败类别；翻译由原生 UI 边界完成，错误不携带文件正文或敏感路径。
#[derive(Debug)]
pub enum OpenError {
    /// 窗口会话已关闭。
    ClosedWindow,
    /// 请求代次已耗尽，原请求及文档不变。
    RequestExhausted,
    /// 选择结果不属于当前 Selecting 请求。
    StaleRequest,
    /// 必须恰好选择一个路径。
    SelectionCount,
    /// 文件扩展名不是 ASCII 大小写不敏感的 md。
    UnsupportedExtension,
    /// 已打开句柄不是普通文件。
    NotRegularFile,
    /// metadata 或实际读取超出注入的硬字节上限。
    BudgetExceeded,
    /// 读取已取消；不会发布部分候选。
    Cancelled,
    /// 文件打开、metadata 或读取失败。
    Io(std::io::ErrorKind),
    /// 适配器违反 Read 返回长度约束。
    InvalidRead,
    /// 受检存储视图构造失败。
    Storage(aline_source::StorageError),
    /// 不是受支持的有效 UTF-8。
    Decode(aline_source::DecodeError),
    /// 不开块的核心构造或身份复验失败。
    Document(super::DocError),
    /// 解析硬预算、取消或映射失败。
    Parse(aline_markdown::ParseError),
    /// 受检内存分配失败。
    Allocation(std::collections::TryReserveError),
}
impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Document(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// 已完整构造且可阅读的单文档；存储基线与 DocCore 共同保留读取事实。
pub struct ReadonlyDocument {
    path: PathBuf,
    baseline: SourceBaseline,
    core: DocCore,
    parsed: Parsed,
    parsed_identity: InputIdentity,
    pending_parse: Option<refresh::PendingParse>,
    editable: bool,
    limits: DocLimits,
}
impl ReadonlyDocument {
    /// 返回用户选择的文件路径，供标题等显式 UI 使用。
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// 返回原 bytes/BOM/行尾与冻结解码源码的只读存储基线。
    pub fn baseline(&self) -> &SourceBaseline {
        &self.baseline
    }
    /// 借用唯一文档核心；其访问策略由打开预算决定。
    pub fn core(&self) -> &DocCore {
        &self.core
    }
    /// 仅向原生宿主借用当前可编辑核心，核心仍负责每次命令的受检写入。
    #[cfg(target_os = "macos")]
    pub(crate) fn edit_core(&mut self) -> Result<&mut DocCore, DocError> {
        if !self.editable {
            return Err(DocError::ReadOnly);
        }
        Ok(&mut self.core)
    }
    /// 返回该打开请求是否采用可编辑访问策略。
    pub const fn is_editable(&self) -> bool {
        self.editable
    }
    /// 冻结当前文档快照，使用本次打开时注入的文本预算。
    pub fn current_snapshot(&mut self) -> Result<super::CurrentSnapshot, DocError> {
        self.core.current_snapshot(self.limits.text())
    }
    /// 冻结当前文档输入身份，供布局与派生结果受检消费。
    pub fn input_identity(&mut self) -> Result<InputIdentity, DocError> {
        self.core.current_input_identity()
    }

    /// 判断已采用解析是否仍属于当前源码和解析配置。
    pub fn is_parse_current(&self) -> bool {
        self.pending_parse.is_none()
            && self.core.is_document_alive()
            && self.parsed_identity.working_version() == self.core.version()
            && self.parsed_identity.parse_config_generation() == self.core.parse_config_generation()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn parsed_identity(&self) -> &InputIdentity {
        &self.parsed_identity
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn text_budget(&self) -> aline_buffer::TextBudget {
        self.limits.text()
    }
    /// 以当前布局/解析身份导航；文档生命周期仍由 ReadonlySession 唯一持有。
    pub fn navigate(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        if !self.editable {
            return self.core.navigate_readonly(identity, selection);
        }
        let current = self.current_parsed_snapshot(identity)?;
        current
            .text()
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;

        if self.core.mode() == EditorMode::Visual
            && let Some(open) = self.core.visual_block_range(&current)?
        {
            let extent = open.byte_range();
            let selected = selection.range().byte_range();
            if extent.start > selected.start || selected.end > extent.end {
                return self
                    .core
                    .retarget_visual(identity, &self.parsed, None, selection);
            }
        }
        self.core.select(identity, selection)
    }

    /// 将一次普通可视化点击升级为块编辑；目录/脚注导航继续只移动选区。
    pub fn open_visual_at(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<bool, DocError> {
        if !self.editable || self.core.mode() != EditorMode::Visual {
            return Ok(false);
        }
        let current = self.current_parsed_snapshot(identity)?;
        current
            .text()
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;
        let selected = selection.range().byte_range();
        let mut matching = self.parsed.blocks().iter().filter(|block| {
            let extent = block.range().byte_range();
            extent.start <= selected.start && selected.end <= extent.end
        });
        let first = matching.next();
        let Some(block) = first.filter(|_| matching.next().is_none()) else {
            return Ok(false);
        };
        let protected = self.parsed.protected_ranges().iter().any(|range| {
            let protected = range.range().byte_range();
            selected.start <= protected.end && protected.start <= selected.end
        });
        if protected {
            return Ok(false);
        }

        if let Some(open) = self.core.visual_block_range(&current)?
            && open == *block.range()
        {
            self.core.select(identity, selection)?;
            return Ok(true);
        }
        if self.core.has_open_block() {
            self.core
                .retarget_visual(identity, &self.parsed, Some(block.range()), selection)?;
        } else {
            self.core
                .open_visual_block(identity, &self.parsed, block.range(), selection)?;
        }
        Ok(true)
    }
    /// 在两种视图间切换，不改变文档生命周期。
    pub fn switch_mode(&mut self, mode: EditorMode) -> Result<ModeSwitchOutcome, DocError> {
        let previous = self.core.mode();
        let outcome = self.core.switch_mode(mode, 0)?;
        if self.core.mode() != previous {
            self.cancel_pending_parse();
        }
        Ok(outcome)
    }
    /// 返回上一份已采用的解析。命中或编辑前还须复验当前文档身份。
    pub fn parsed(&self) -> &Parsed {
        &self.parsed
    }

    fn current_parsed_snapshot(
        &mut self,
        identity: &InputIdentity,
    ) -> Result<super::CurrentSnapshot, DocError> {
        if !self.is_parse_current() || self.parsed.snapshot().id() != identity.snapshot_id() {
            return Err(DocError::StaleResult);
        }
        self.core.accept_document_result(identity)?;
        self.current_snapshot()
    }

    fn can_discard_for_replacement(&mut self) -> bool {
        if !self.editable {
            return true;
        }
        if self.core.composition().is_some()
            || self
                .core
                .wait()
                .is_some_and(|wait| wait.status() == WaitStatus::Pending)
        {
            return false;
        }
        let Ok(snapshot) = self.current_snapshot() else {
            return false;
        };
        snapshot
            .full_text(self.limits.text())
            .is_ok_and(|text| text == self.baseline.source())
    }
}

/// 背景读取工作的最小不可变输入；不持有窗口或当前 Ready 文档。
pub struct ReadonlyOpenJob {
    request: OpenRequestId,
    path: PathBuf,
    cancel: CancelToken,
    limits: ReadonlyOpenLimits,
}
impl ReadonlyOpenJob {
    /// 返回是否已取消；宿主在实际执行前也可提前释放已取消工作。
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// 后台读取三段的真实起止边界，供验证探针采集分段成本。
///
/// 单调耗时与墙钟边界分开捕获，不参与业务判定、不调度线程、不影响失败语义；
/// 打开失败时各段为零。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OpenTimings {
    pub(crate) read_decode: StageTiming,
    pub(crate) snapshot: StageTiming,
    pub(crate) parse_mapping: StageTiming,
    pub(crate) dispatch_wait: StageTiming,
    pub(crate) result_wait: StageTiming,
}

/// 后台完成候选；只能由只读 job 产生，必须交回所属会话复验后发布。
pub struct ReadonlyOpenCompletion {
    request: OpenRequestId,
    result: Result<ReadonlyDocument, OpenError>,
    pub(crate) timings: OpenTimings,
}

/// 候选的最终结算；只有 Applied 表示替换成功。
pub enum OpenDisposition {
    /// 当前有效候选已原子替换 Ready 文档。
    Applied,
    /// 当前请求合作取消；旧文档保留且不显示错误。
    Cancelled,
    /// 旧请求、另一会话或关闭后的结果已释放，不能复活目标。
    Stale,
    /// 当前请求失败；旧文档/视图/选区保留。
    Failed(OpenError),
    /// 完成候选有效，但当前可编辑文档有未结算内容，因此拒绝替换。
    RejectedUnresolvedDocument,
}

/// 关闭文档的结果；拒绝时当前文档和在途请求均保留。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseDisposition {
    /// 文档已关闭。
    Closed,
    /// 当前可编辑文档仍有未结算内容或发布等待。
    Refused,
}

/// 无窗口单文档宿主；当前 Ready 与一个 Selecting/Loading 请求分别持有。
pub struct ReadonlySession {
    identity: Arc<()>,
    next_generation: u64,
    limits: ReadonlyOpenLimits,
    pending: Option<PendingOpen>,
    document: Option<ReadonlyDocument>,
    alive: bool,
}
impl ReadonlySession {
    /// 建立无文档状态，不创建编辑块、任务或文件句柄。
    pub fn new(limits: ReadonlyOpenLimits) -> Self {
        Self {
            identity: Arc::new(()),
            next_generation: 0,
            limits,
            pending: None,
            document: None,
            alive: true,
        }
    }
    /// 返回当前完整 Ready 文档；加载候选不会覆盖它。
    pub fn document(&self) -> Option<&ReadonlyDocument> {
        self.document.as_ref()
    }
    /// 借用当前 Ready 的受检导航/模式能力。
    pub fn document_mut(&mut self) -> Option<&mut ReadonlyDocument> {
        self.document.as_mut()
    }
    /// 是否存在尚未结算的文件选择或加载候选。
    pub const fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
    /// 返回当前候选的实际阶段，供加载反馈使用。
    pub fn pending_phase(&self) -> Option<ReadonlyOpenPhase> {
        self.pending.as_ref().map(|pending| pending.phase)
    }

    /// 发起新选择意图；身份预检成功后取消旧请求资格，保留 Ready 文档。
    pub fn request_open(&mut self) -> Result<OpenRequestId, OpenError> {
        if !self.alive {
            return Err(OpenError::ClosedWindow);
        }
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or(OpenError::RequestExhausted)?;
        self.cancel_pending();
        let id = OpenRequestId {
            session: self.identity.clone(),
            generation,
        };
        self.pending = Some(PendingOpen {
            id: id.clone(),
            cancel: CancelToken::new(),
            phase: ReadonlyOpenPhase::Selecting,
        });
        self.next_generation = generation;
        Ok(id)
    }

    /// 复验单路径选择并生成只读工作；None 为用户取消，其他失败不会替换 Ready。
    pub fn accept_selection(
        &mut self,
        request: &OpenRequestId,
        selection: Option<Vec<PathBuf>>,
    ) -> Result<Option<ReadonlyOpenJob>, OpenError> {
        if !self.alive {
            return Err(OpenError::ClosedWindow);
        }
        let pending = self.pending.as_ref().ok_or(OpenError::StaleRequest)?;
        if &pending.id != request || pending.phase != ReadonlyOpenPhase::Selecting {
            return Err(OpenError::StaleRequest);
        }
        let Some(mut paths) = selection else {
            self.cancel_pending();
            return Ok(None);
        };
        if paths.len() != 1 {
            self.cancel_pending();
            return Err(OpenError::SelectionCount);
        }
        let path = paths.pop().ok_or(OpenError::SelectionCount)?;
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            self.cancel_pending();
            return Err(OpenError::UnsupportedExtension);
        }
        let pending = self.pending.as_mut().ok_or(OpenError::StaleRequest)?;
        pending.phase = ReadonlyOpenPhase::Loading;
        Ok(Some(ReadonlyOpenJob {
            request: pending.id.clone(),
            path,
            cancel: pending.cancel.clone(),
            limits: self.limits,
        }))
    }

    /// 取消仍属当前的意图；旧取消不能影响更新的请求。
    pub fn cancel_open(&mut self, request: &OpenRequestId) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| &pending.id == request)
        {
            self.cancel_pending();
            return true;
        }
        false
    }

    /// 仅发布仍存活会话的最新 Loading 候选；失败/取消/旧结果都保留原文档。
    pub fn accept_open(&mut self, completion: ReadonlyOpenCompletion) -> OpenDisposition {
        match self.accept_open_with(completion, |_| Ok::<(), std::convert::Infallible>(())) {
            Ok((disposition, _)) => disposition,
            Err(never) => match never {},
        }
    }

    /// 复验请求后准备宿主所需的派生状态。准备失败时结算请求并保留当前文档。
    pub(crate) fn accept_open_with<T, E>(
        &mut self,
        completion: ReadonlyOpenCompletion,
        prepare: impl FnOnce(&mut ReadonlyDocument) -> Result<T, E>,
    ) -> Result<(OpenDisposition, Option<T>), E> {
        if !self.alive
            || !self.pending.as_ref().is_some_and(|pending| {
                pending.id == completion.request && pending.phase == ReadonlyOpenPhase::Loading
            })
        {
            return Ok((OpenDisposition::Stale, None));
        }
        self.pending = None;
        match completion.result {
            Ok(mut document) => {
                if self
                    .document
                    .as_mut()
                    .is_some_and(|current| !current.can_discard_for_replacement())
                {
                    return Ok((OpenDisposition::RejectedUnresolvedDocument, None));
                }
                let prepared = prepare(&mut document)?;
                self.document = Some(document);
                Ok((OpenDisposition::Applied, Some(prepared)))
            }
            Err(OpenError::Cancelled) => Ok((OpenDisposition::Cancelled, None)),
            Err(error) => Ok((OpenDisposition::Failed(error), None)),
        }
    }

    /// 关闭当前文档并取消在途候选；回到可再次打开的无文档状态。
    /// 关闭获准后、释放文档前执行一次由宿主提供的本地结算回调。
    pub(crate) fn close_document_with<T>(
        &mut self,
        before_close: impl FnOnce(&ReadonlyDocument) -> T,
    ) -> Result<Option<T>, CloseDisposition> {
        let close_value = if let Some(document) = self.document.as_mut() {
            if !document.can_discard_for_replacement() {
                return Err(CloseDisposition::Refused);
            }
            Some(before_close(document))
        } else {
            None
        };
        self.cancel_pending();
        self.document = None;
        Ok(close_value)
    }

    /// 关闭当前文档并取消在途候选；未结算编辑时保留文档。
    pub fn close_document(&mut self) -> CloseDisposition {
        match self.close_document_with(|_| ()) {
            Ok(_) => CloseDisposition::Closed,
            Err(disposition) => disposition,
        }
    }
    /// 系统关闭窗口时失效所有候选、释放文档，之后不再接受新意图。
    pub fn close_window(&mut self) -> CloseDisposition {
        if self.close_document() == CloseDisposition::Refused {
            return CloseDisposition::Refused;
        }
        self.alive = false;
        CloseDisposition::Closed
    }
    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.cancel();
        }
    }
}
impl Drop for ReadonlySession {
    fn drop(&mut self) {
        self.cancel_pending();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};

    use super::*;
    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};

    struct Files(&'static [u8]);

    impl ReadonlyFileAccess for Files {
        type Reader = Cursor<&'static [u8]>;

        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }

    fn editable_session(bytes: &'static [u8]) -> Result<ReadonlySession, Box<dyn Error>> {
        let max = 4096;
        let limits = ReadonlyOpenLimits::new(
            StorageBudget::new(max),
            DocLimits::new(
                TextBudget::new(max),
                LineIndexBudget::new(128),
                HistoryBudget::new(16, max),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing();
        let mut session = ReadonlySession::new(limits);
        let request = session.request_open()?;
        let job = session
            .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
            .ok_or("missing open job")?;
        assert!(matches!(
            session.accept_open(job.read_with(&Files(bytes))),
            OpenDisposition::Applied
        ));
        Ok(session)
    }

    #[test]
    fn request_id_exhaustion_keeps_the_existing_pending_request() -> Result<(), OpenError> {
        let limits = ReadonlyOpenLimits::new(
            StorageBudget::new(1024),
            DocLimits::new(
                TextBudget::new(1024),
                LineIndexBudget::new(32),
                HistoryBudget::new(32, 1024),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        );
        let mut session = ReadonlySession::new(limits);
        let current = session.request_open()?;
        session.next_generation = u64::MAX;
        assert!(matches!(
            session.request_open(),
            Err(OpenError::RequestExhausted)
        ));
        assert_eq!(session.pending_phase(), Some(ReadonlyOpenPhase::Selecting));
        assert!(session.cancel_open(&current));
        Ok(())
    }

    #[test]
    fn dirty_editable_document_refuses_close_and_replacement_until_restored()
    -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"base")?;
        let document = session.document_mut().ok_or("document")?;
        let identity = document.input_identity()?;
        let source = document.parsed().snapshot().src_offset(0)?;
        let selection = SrcSelection::new(source.clone(), source)?;
        assert!(document.open_visual_at(&identity, &selection)?);
        document.core.type_text(0, "x", 1)?;
        let original_id = document.core.document_id().clone();

        let request = session.request_open()?;
        let replacement = session
            .accept_selection(&request, Some(vec![PathBuf::from("replacement.md")]))?
            .ok_or("replacement job")?;
        assert!(matches!(
            session.accept_open(replacement.read_with(&Files(b"replacement"))),
            OpenDisposition::RejectedUnresolvedDocument
        ));
        let document = session.document_mut().ok_or("preserved document")?;
        assert_eq!(document.core.document_id(), &original_id);
        assert_eq!(document.path(), Path::new("fixture.md"));
        let snapshot = document.current_snapshot()?;
        assert_eq!(snapshot.full_text(document.limits.text())?, "xbase");
        assert_eq!(session.close_document(), CloseDisposition::Refused);
        assert!(session.document().is_some());
        let side_effect = Cell::new(false);
        assert_eq!(
            session.close_document_with(|_| side_effect.set(true)),
            Err(CloseDisposition::Refused)
        );
        assert!(!side_effect.get());

        session
            .document_mut()
            .ok_or("preserved document")?
            .core
            .undo()?;
        let closed = Cell::new(false);
        assert_eq!(
            session.close_document_with(|_| closed.set(true)),
            Ok(Some(()))
        );
        assert!(closed.get());
        assert!(session.document().is_none());
        Ok(())
    }

    #[test]
    fn cancelled_publication_wait_does_not_block_clean_close() -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"base")?;
        let document = session.document_mut().ok_or("document")?;
        let identity = document.input_identity()?;
        let source = document.parsed().snapshot().src_offset(0)?;
        let selection = SrcSelection::new(source.clone(), source)?;
        assert!(document.open_visual_at(&identity, &selection)?);
        document.core.composition_start("候选", 1)?;
        let wait = document
            .core
            .begin_publication_wait(super::super::PublicationKind::Save)?;
        document.core.request_cancel_wait(wait)?;
        document.core.composition_cancel(2)?;
        assert!(
            document
                .core
                .wait()
                .is_some_and(|wait| wait.status() == WaitStatus::Cancelled)
        );

        assert_eq!(session.close_document(), CloseDisposition::Closed);
        assert!(session.document().is_none());
        Ok(())
    }
}
