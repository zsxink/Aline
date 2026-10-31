//! 宿主声明的资源引用所有权；M0 不读取文件、不发现 Markdown 图片或清理资源。

use std::fmt;
use std::sync::{Arc, Weak};

use super::DocumentId;

/// 文档所需资源的不可变强引用；消费方是宿主、当前快照及发布/恢复任务。
///
/// 宿主必须在引用生效前登记，定位值由宿主解释；它不代表文件已经存在或获准迁移。
#[derive(Clone)]
pub struct ResourceReference(Arc<ResourceData>);

struct ResourceData {
    document: DocumentId,
    locator: String,
}

impl ResourceReference {
    /// 为给定文档创建引用；相同定位值的独立构造仍是独立资源身份。
    pub fn new(document: DocumentId, locator: impl Into<String>) -> Self {
        Self(Arc::new(ResourceData {
            document,
            locator: locator.into(),
        }))
    }

    /// 返回该引用的文档身份，防止跨文档误登记。
    pub fn document_id(&self) -> &DocumentId {
        &self.0.document
    }

    /// 返回宿主解释的定位值；诊断不应直接记录这个值。
    pub fn locator(&self) -> &str {
        &self.0.locator
    }

    /// 创建不延长资源生命周期的句柄，供宿主尝试重新取得强引用。
    pub fn downgrade(&self) -> WeakResourceReference {
        WeakResourceReference(Arc::downgrade(&self.0))
    }
}

impl PartialEq for ResourceReference {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ResourceReference {}
impl fmt::Debug for ResourceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResourceReference(..)")
    }
}

/// 不持有资源的生命周期观察句柄；不触发删除或其他文件副作用。
#[derive(Clone, Debug)]
pub struct WeakResourceReference(Weak<ResourceData>);
impl WeakResourceReference {
    /// 尝试重新持有资源；失败仅表示引用已释放，不授权删除任何实际文件。
    pub fn upgrade(&self) -> Option<ResourceReference> {
        self.0.upgrade().map(ResourceReference)
    }
}
