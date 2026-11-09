//! 单窗口只读打开生命周期；唯一持有 Ready 文档与至多一个打开请求。
//!
//! 输入为文件选择结果和后台完成候选，输出为当前只读文档与明确结算结果。
//! 请求资格在候选发布前复验；失败/取消保留原文档。本模块不调度线程、不写盘。
//! 读取适配器见 read 子模块，实际窗口宿主负责有限并发与任务回收。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aline_markdown::{CancelToken, ParseBudget, ParseConfig, Parsed};
use aline_source::{SourceBaseline, StorageBudget};

use super::{DocCore, DocError, DocLimits, EditorMode, InputIdentity, ModeSwitchOutcome};
use aline_buffer::SrcSelection;

mod read;
mod timing;
pub use read::{ReadonlyFileAccess, ReadonlyFileInput};
pub(crate) use timing::{StageTimer, StageTiming};

/// 只读打开输入、文档与解析的显式注入预算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadonlyOpenLimits {
    storage: StorageBudget,
    document: DocLimits,
    config: ParseConfig,
    parse: ParseBudget,
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
        }
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
    /// 借用唯一只读文档核心。
    pub fn core(&self) -> &DocCore {
        &self.core
    }
    /// 冻结当前阅读输入身份；不开放配置、编辑或核心关闭能力。
    pub fn input_identity(&mut self) -> Result<InputIdentity, DocError> {
        self.core.current_input_identity()
    }
    /// 以当前布局/解析身份导航；文档生命周期仍由 ReadonlySession 唯一持有。
    pub fn navigate(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        self.core.navigate_readonly(identity, selection)
    }
    /// 在两种只读视图间切换，不改变源码、历史或文档存活状态。
    pub fn switch_mode(&mut self, mode: EditorMode) -> Result<ModeSwitchOutcome, DocError> {
        self.core.switch_mode(mode, 0)
    }
    /// 返回与初始核心源码快照绑定的完整解析结果。
    pub fn parsed(&self) -> &Parsed {
        &self.parsed
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
        if !self.alive
            || !self.pending.as_ref().is_some_and(|pending| {
                pending.id == completion.request && pending.phase == ReadonlyOpenPhase::Loading
            })
        {
            return OpenDisposition::Stale;
        }
        self.pending = None;
        match completion.result {
            Ok(document) => {
                self.document = Some(document);
                OpenDisposition::Applied
            }
            Err(OpenError::Cancelled) => OpenDisposition::Cancelled,
            Err(error) => OpenDisposition::Failed(error),
        }
    }

    /// 关闭当前文档并取消在途候选；回到可再次打开的无文档状态。
    pub fn close_document(&mut self) {
        self.cancel_pending();
        self.document = None;
    }
    /// 系统关闭窗口时失效所有候选、释放文档，之后不再接受新意图。
    pub fn close_window(&mut self) {
        self.close_document();
        self.alive = false;
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
    use super::*;
    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};

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
}
