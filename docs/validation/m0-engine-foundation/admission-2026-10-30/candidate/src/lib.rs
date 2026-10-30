//! 隔离候选仅验证依赖可解析与类型可消费，不启动应用或窗口。
#![forbid(unsafe_code)]

pub fn candidate_size() -> usize {
    std::mem::size_of::<gpui::SharedString>()
}
