//! 插件启停状态与代次。
//!
//! 有效启用集合由宿主的配置服务发布；本模块只维护由其驱动的能力状态，
//! 不读取设置文件，也不决定默认值。

use crate::contracts::CapabilityId;

/// 宿主发布的有效启用集合；构造时排序去重，比较与遍历均确定。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EnabledCapabilities {
    ids: Vec<CapabilityId>,
}

impl EnabledCapabilities {
    /// 空集合。
    pub fn none() -> Self {
        Self::default()
    }

    /// 由任意顺序的能力标识构造；重复项只保留一次。
    pub fn from_ids(ids: impl IntoIterator<Item = CapabilityId>) -> Self {
        let mut ids: Vec<CapabilityId> = ids.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        Self { ids }
    }

    /// 判断能力是否启用。
    pub fn contains(&self, capability: CapabilityId) -> bool {
        self.ids.binary_search(&capability).is_ok()
    }

    /// 按标识顺序遍历。
    pub fn iter(&self) -> impl Iterator<Item = CapabilityId> + '_ {
        self.ids.iter().copied()
    }
}

/// 插件启停状态。
///
/// 集合真正变化时推进代次；关闭后再启用即使回到相同集合，也因中间
/// 变化而得到新代次，关闭前的在途请求不能复活。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lifecycle {
    generation: u64,
    enabled: EnabledCapabilities,
}

impl Lifecycle {
    /// 初始代次为 1，能力全部关闭。
    pub fn new() -> Self {
        Self {
            generation: 1,
            enabled: EnabledCapabilities::none(),
        }
    }

    /// 当前插件代次。
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// 当前有效启用集合。
    pub fn enabled(&self) -> &EnabledCapabilities {
        &self.enabled
    }

    /// 应用新的有效启用集合，返回应用后的代次。
    ///
    /// 集合与当前一致时不推进代次，避免无变化的重建作废在途结果。
    pub fn apply(&mut self, enabled: EnabledCapabilities) -> u64 {
        if enabled != self.enabled {
            self.generation = self.generation.saturating_add(1);
            self.enabled = enabled;
        }
        self.generation
    }
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}
