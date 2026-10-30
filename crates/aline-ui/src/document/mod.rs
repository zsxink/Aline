//! 宿主文档身份与输入信封；不实现窗口、输入适配或文件协调器。

mod identity;

pub use identity::{DocumentId, DraftGeneration, IdentityError, InputIdentity, WorkingVersion};
