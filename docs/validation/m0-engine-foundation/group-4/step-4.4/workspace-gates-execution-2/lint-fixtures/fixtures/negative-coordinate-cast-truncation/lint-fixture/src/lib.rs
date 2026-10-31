//! 隔离源码/绘制坐标窄化样例。
/// 故意把绘制字节偏移窄化。
pub fn invalid_drawn_offset(drawn_off: u64) -> u16 { drawn_off as u16 }
