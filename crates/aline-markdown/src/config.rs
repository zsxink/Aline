//! Markdown 解析配置：显式运行期选项与配置代次，不依赖 Cargo feature 推断。

use pulldown_cmark::Options;

/// 显式启用的 Markdown 语法位；每一位对应一个 pulldown-cmark 运行期选项。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseFeatures(u32);

impl ParseFeatures {
    /// GFM 表格。
    pub const TABLES: Self = Self(1 << 1);
    /// GitHub 脚注。
    pub const FOOTNOTES: Self = Self(1 << 2);
    /// 删除线。
    pub const STRIKETHROUGH: Self = Self(1 << 3);
    /// 任务列表。
    pub const TASKLISTS: Self = Self(1 << 4);
    /// 公式事件；M0 只保留源码，不渲染公式。
    pub const MATH: Self = Self(1 << 10);
    /// GitHub Alert（引用块种类）。
    pub const GFM: Self = Self(1 << 11);

    /// M0 默认启用的语法；front matter 由本 crate 识别，不启用原生 metadata 选项。
    pub const M0: Self = Self::TABLES
        .union(Self::FOOTNOTES)
        .union(Self::STRIKETHROUGH)
        .union(Self::TASKLISTS)
        .union(Self::MATH)
        .union(Self::GFM);

    /// 合并两组语法位。
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// 判断是否包含全部给定语法位。
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// 返回原始位值，供配置日志和回填记录。
    pub const fn bits(self) -> u32 {
        self.0
    }

    pub(crate) fn options(self) -> Options {
        let mut options = Options::empty();
        if self.contains(Self::TABLES) {
            options.insert(Options::ENABLE_TABLES);
        }
        if self.contains(Self::FOOTNOTES) {
            options.insert(Options::ENABLE_FOOTNOTES);
        }
        if self.contains(Self::STRIKETHROUGH) {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }
        if self.contains(Self::TASKLISTS) {
            options.insert(Options::ENABLE_TASKLISTS);
        }
        if self.contains(Self::MATH) {
            options.insert(Options::ENABLE_MATH);
        }
        if self.contains(Self::GFM) {
            options.insert(Options::ENABLE_GFM);
        }
        options
    }
}

/// 一次解析使用的语法集合和配置代次；代次变化会使旧 Parsed 不可复用。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseConfig {
    version: u32,
    features: ParseFeatures,
}

impl ParseConfig {
    /// M0 默认配置；版本 1，显式启用表格、脚注、删除线、任务列表、公式和 Alert。
    pub const fn m0() -> Self {
        Self {
            version: 1,
            features: ParseFeatures::M0,
        }
    }

    /// 构造指定代次和语法集合的配置；版本 0 在解析入口被拒绝。
    pub const fn new(version: u32, features: ParseFeatures) -> Self {
        Self { version, features }
    }

    /// 返回配置代次。
    pub const fn version(self) -> u32 {
        self.version
    }

    /// 返回显式语法集合。
    pub const fn features(self) -> ParseFeatures {
        self.features
    }
}
