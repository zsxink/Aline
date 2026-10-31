//! UI 持有的实例文档身份和单调工作版本；引擎仅提供低层版本与快照身份。

use std::fmt;
use std::sync::Arc;

use aline_buffer::{DocumentRevision, SnapshotId, TextSnapshot};

/// 文档或输入身份拒绝原因，供未来 DocCore 事务入口匹配。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityError {
    /// 结果或请求属于另一文档。
    WrongDocument,
    /// 工作版本已变化。
    StaleWorkingVersion,
    /// 当前源码视图已变化。
    StaleSnapshot,
    /// 解析配置已经变化。
    StaleParseConfig,
    /// 已有较新的派生请求。
    StaleRequest,
    /// 草稿代次已耗尽。
    GenerationExhausted,
    /// 提交未推进已提交 revision。
    RevisionNotAdvanced,
    /// 信封的 revision 不属于给定源码快照。
    SnapshotRevisionMismatch,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for IdentityError {}

/// 会话内唯一文档身份；不由路径、内容或全局计数构造。
#[derive(Clone)]
pub struct DocumentId(Arc<()>);

impl DocumentId {
    /// 为新文档所有者创建身份；clone 用于其请求和结果信封。
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for DocumentId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for DocumentId {}
impl fmt::Debug for DocumentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DocumentId(..)")
    }
}

/// 不因文本恢复或组合取消而复用的草稿代次。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DraftGeneration(u64);

impl DraftGeneration {
    /// 创建会话初始草稿代次。
    pub const fn initial() -> Self {
        Self(0)
    }

    /// 构造只读代次值，不赋予调用者文档身份。
    pub const fn from_value(value: u64) -> Self {
        Self(value)
    }

    /// 返回草稿代次数值。
    pub const fn value(self) -> u64 {
        self.0
    }

    /// 受检推进，耗尽时拒绝。
    pub fn next(self) -> Result<Self, IdentityError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(IdentityError::GenerationExhausted)
    }
}

/// 当前已提交 revision 与草稿代次的组合；仅由 UI 文档所有者发布。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingVersion {
    revision: DocumentRevision,
    draft: DraftGeneration,
}

impl WorkingVersion {
    /// 构造不可变工作版本值，实际采纳仍须验证文档和源码身份。
    pub const fn new(revision: DocumentRevision, draft: DraftGeneration) -> Self {
        Self { revision, draft }
    }

    /// 返回已提交文本的代次。
    pub const fn revision(self) -> DocumentRevision {
        self.revision
    }

    /// 返回当前草稿代次。
    pub const fn draft_generation(self) -> DraftGeneration {
        self.draft
    }

    /// 普通输入/组合更新或取消后推进草稿代次。
    pub fn next_draft(self) -> Result<Self, IdentityError> {
        Ok(Self {
            revision: self.revision,
            draft: self.draft.next()?,
        })
    }

    /// 提交、Undo/Redo 或重载后受检推进；不重置草稿代次。
    pub fn after_commit(self, revision: DocumentRevision) -> Result<Self, IdentityError> {
        if revision <= self.revision {
            return Err(IdentityError::RevisionNotAdvanced);
        }
        Ok(Self {
            revision,
            draft: self.draft.next()?,
        })
    }
}

/// 解析配置的单调代次；具体配置由宿主所有者维护。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseConfigGeneration(u64);
impl ParseConfigGeneration {
    /// 创建初始配置身份。
    pub const fn initial() -> Self {
        Self(0)
    }
    /// 返回代次，供宿主与证据记录。
    pub const fn value(self) -> u64 {
        self.0
    }
    pub(crate) fn next(self) -> Result<Self, IdentityError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(IdentityError::GenerationExhausted)
    }
}

/// 单文档派生采纳的单调身份；新请求、目标关闭或模式切换均推进，不与文本版本混用。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DerivedRequestId(u64);
impl DerivedRequestId {
    #[cfg(test)]
    pub(super) const fn exhausted_for_test() -> Self {
        Self(u64::MAX)
    }

    /// 创建尚未发起请求的初始身份。
    pub const fn initial() -> Self {
        Self(0)
    }
    /// 返回请求编号。
    pub const fn value(self) -> u64 {
        self.0
    }
    pub(crate) fn next(self) -> Result<Self, IdentityError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(IdentityError::GenerationExhausted)
    }
}

/// 具体的文档输入身份信封；不提供泛型消息总线或引擎反向依赖。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputIdentity {
    document: DocumentId,
    version: WorkingVersion,
    snapshot: SnapshotId,
    config: ParseConfigGeneration,
    request: DerivedRequestId,
}

impl InputIdentity {
    /// 绑定初始配置/尚未请求的视图；revision 不一致时拒绝，不复制全文。
    /// 活跃 DocCore 任务应使用 current_input_identity/begin_derived_request，避免漏配身份。
    pub fn new(
        document: DocumentId,
        version: WorkingVersion,
        snapshot: &TextSnapshot,
    ) -> Result<Self, IdentityError> {
        Self::for_request(
            document,
            version,
            snapshot,
            ParseConfigGeneration::initial(),
            DerivedRequestId::initial(),
        )
    }

    /// 绑定宿主实际配置与请求身份，供派生任务传递冻结输入键。
    pub fn for_request(
        document: DocumentId,
        version: WorkingVersion,
        snapshot: &TextSnapshot,
        config: ParseConfigGeneration,
        request: DerivedRequestId,
    ) -> Result<Self, IdentityError> {
        if version.revision != snapshot.revision() {
            return Err(IdentityError::SnapshotRevisionMismatch);
        }
        Ok(Self {
            document,
            version,
            snapshot: snapshot.id().clone(),
            config,
            request,
        })
    }

    /// 返回文档身份。
    pub fn document_id(&self) -> &DocumentId {
        &self.document
    }

    /// 返回当前工作版本。
    pub const fn working_version(&self) -> WorkingVersion {
        self.version
    }

    /// 返回当前源码快照身份。
    pub fn snapshot_id(&self) -> &SnapshotId {
        &self.snapshot
    }

    /// 返回冻结的解析配置代次。
    pub const fn parse_config_generation(&self) -> ParseConfigGeneration {
        self.config
    }
    /// 返回冻结的派生请求身份。
    pub const fn request_id(&self) -> DerivedRequestId {
        self.request
    }

    /// 采纳请求/结果前逐项比对；不同文档、旧工作版本或旧视图明确拒绝。
    pub fn validate(&self, current: &Self) -> Result<(), IdentityError> {
        if self.document != current.document {
            return Err(IdentityError::WrongDocument);
        }
        if self.version != current.version {
            return Err(IdentityError::StaleWorkingVersion);
        }
        if self.snapshot != current.snapshot {
            return Err(IdentityError::StaleSnapshot);
        }
        if self.config != current.config {
            return Err(IdentityError::StaleParseConfig);
        }
        if self.request != current.request {
            return Err(IdentityError::StaleRequest);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DerivedRequestId, IdentityError, ParseConfigGeneration};

    #[test]
    fn derived_counters_reject_exhaustion_without_wrapping() {
        assert_eq!(
            DerivedRequestId(u64::MAX).next(),
            Err(IdentityError::GenerationExhausted)
        );
        assert_eq!(
            ParseConfigGeneration(u64::MAX).next(),
            Err(IdentityError::GenerationExhausted)
        );
    }
}
