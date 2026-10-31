//! 存储基线：原 bytes、对应冻结源码与行尾策略的只读绑定。
//!
//! 基线只能由 source 自身的解码或受检写盘事实创建；字段私有，调用方不能用
//! 相同文本、路径或自报布尔值伪造“未修改”或替换原 bytes。

use std::fmt;
use std::sync::Arc;

use crate::decode::StorageMetadata;
use crate::storage::{BaselineId, StorageSnapshot};

/// 原 bytes 与其解码所得冻结源码的只读存储基线。
///
/// `source` 是解码后的精确编辑源码，不是按行尾策略重建的副本；补丁、历史、
/// 工作版本与无变化判定都使用这份精确文本。存储身份由持有的
/// [`StorageSnapshot`] 决定，clone 保留同一身份。
///
/// 不变量：`source` 必须恰为 `storage.bytes()` 的解码结果，`metadata` 必须是
/// 该次解码的事实。`new` 为 crate 内构造，目前只由 `decode` 调用；4.3/4.4
/// 增加成功写盘基线时也 MUST 从实际写出的 bytes 解码得到，不能按调用方陈述
/// 拼接三部分。
#[derive(Clone)]
pub struct SourceBaseline {
    storage: StorageSnapshot,
    source: Arc<str>,
    metadata: StorageMetadata,
}

impl fmt::Debug for SourceBaseline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceBaseline")
            .field("id", self.storage.id())
            .field("version", &self.storage.version())
            .field("source_len", &self.source.len())
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl SourceBaseline {
    pub(crate) fn new(storage: StorageSnapshot, source: String, metadata: StorageMetadata) -> Self {
        Self {
            storage,
            source: Arc::from(source),
            metadata,
        }
    }

    /// 返回原 bytes 的只读存储视图。
    pub fn storage(&self) -> &StorageSnapshot {
        &self.storage
    }

    /// 返回与该原 bytes 对应的精确冻结源码。
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 返回原始字节的 BOM 与行尾统计；重编码策略也由它决定。
    pub const fn metadata(&self) -> StorageMetadata {
        self.metadata
    }

    /// 返回存储基线身份。
    pub fn id(&self) -> &BaselineId {
        self.storage.id()
    }
}
