//! 生产恢复运行时的纯装配：受检创建恢复根、打开存储与临时根，
//! 失败一律失败关闭并给出反馈键，不影响内存文档与保存能力。

use std::path::Path;
use std::sync::Arc;

use crate::document::{
    RecoveryConfig, RecoveryResourceSource, RecoveryStore, TempResourceSource, TempRoot,
    UnavailableResources, create_default_root,
};

/// 生产恢复接线结果；两侧都为 None 时只反馈、不扫描、不建面板。
pub(crate) struct RecoveryRuntime {
    pub(crate) config: Option<RecoveryConfig>,
    pub(crate) temp_root: Option<TempRoot>,
    pub(crate) feedback: Option<&'static str>,
}

/// 由注入的 HOME 派生生产恢复；`None`、非 macOS 或根建立失败时失败关闭。
pub(crate) fn production_recovery(home: Option<&Path>) -> RecoveryRuntime {
    production_recovery_with(home, cfg!(target_os = "macos"))
}

/// 平台开关显式化以便在任意目标上验证“无副作用地禁用”。
pub(crate) fn production_recovery_with(home: Option<&Path>, supported: bool) -> RecoveryRuntime {
    if !supported {
        return disabled();
    }
    let Some(home) = home else {
        return disabled();
    };
    let root = match create_default_root(home) {
        Ok(root) => root,
        Err(_) => return disabled(),
    };
    let store = match RecoveryStore::open(root) {
        Ok(store) => store,
        Err(_) => return disabled(),
    };
    let temp_root = TempRoot::open_or_create(home).ok();
    let resources: Arc<dyn RecoveryResourceSource> = match temp_root.as_ref() {
        Some(root) => Arc::new(TempResourceSource::new(root.clone())),
        None => Arc::new(UnavailableResources),
    };
    RecoveryRuntime {
        config: Some(RecoveryConfig { store, resources }),
        temp_root,
        feedback: None,
    }
}

fn disabled() -> RecoveryRuntime {
    RecoveryRuntime {
        config: None,
        temp_root: None,
        feedback: Some("document.recovery_disabled"),
    }
}
