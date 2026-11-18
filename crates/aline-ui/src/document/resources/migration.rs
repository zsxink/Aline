//! 首次保存/另存为的资源迁移：把临时资源复制到文档资产目录，并生成引用重写
//! 计划。迁移只读写资产目录与临时根，不修改文本；引用重写由会话层通过编辑协议
//! 提交，保证一次历史步且可撤销。
//!
//! 复制阶段在后台执行，只持有快照、条目快照、设置与取消句柄。

use std::fs;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use aline_buffer::TextBudget;
use aline_source::Cancellation;
use aline_source::fs::{self as platform_fs, FileIdentity, FsError};

use super::catalog::{TempResource, resource_extension};
use crate::document::CurrentSnapshot;
use crate::document::persistence::WriteKind;
use crate::document::recovery::{
    MAX_GENERATION_RESOURCE_BYTES, MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT,
};

/// 资产目录设置；相对形式以目标文档目录为基准。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ImageDirectory {
    Relative(String),
    Absolute(PathBuf),
}

/// 内部注入的图片资产设置；4.2 不提供 UI、不持久化。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImageSettings {
    pub(crate) directory: ImageDirectory,
}

impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            directory: ImageDirectory::Relative("assets".to_owned()),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl ImageSettings {
    /// 校验目录设置；相对形式不得为空、不得为绝对路径、不得含 `..`。
    pub(crate) fn validate(&self) -> Result<(), MigrationError> {
        match &self.directory {
            ImageDirectory::Relative(dir) => {
                if dir.is_empty()
                    || dir.contains('\0')
                    || Path::new(dir).is_absolute()
                    || Path::new(dir)
                        .components()
                        .any(|part| matches!(part, Component::ParentDir))
                {
                    return Err(MigrationError::InvalidSettings);
                }
            }
            ImageDirectory::Absolute(dir) => {
                if dir.as_os_str().is_empty()
                    || !dir.is_absolute()
                    || dir
                        .components()
                        .any(|part| matches!(part, Component::ParentDir))
                {
                    return Err(MigrationError::InvalidSettings);
                }
            }
        }
        Ok(())
    }
}

/// 图片设置被拒绝的原因；配置只能在没有文档时注入。
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ImageSettingsError {
    DocumentOpen,
    Invalid,
}

/// 一条 locator 到新引用文本的映射；`from` 逐字匹配当前文本。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceMapping {
    pub(crate) from: String,
    pub(crate) to: String,
}

/// 一次迁移的成果：目标路径、引用映射与已就位的资产路径。
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResourceMigration {
    /// 目标文档路径；会话结算使用迁移前冻结的 association 状态。
    #[allow(dead_code)]
    pub(crate) target: PathBuf,
    pub(crate) mappings: Vec<ResourceMapping>,
    pub(crate) copied: Vec<PathBuf>,
}

/// 迁移失败分类；任何失败都不报告成功，已发布资产按身份撤回。
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum MigrationError {
    InvalidTarget,
    InvalidSettings,
    UnsupportedPlatform,
    TooManyResources,
    BudgetExceeded,
    NameExhausted,
    Io,
    Cancelled,
    StaleResourceSet,
}

/// 复制阶段的后台包；只持有快照、条目、设置与取消句柄。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResourceMigrationJob {
    document: crate::document::DocumentId,
    operation: u64,
    kind: WriteKind,
    target: PathBuf,
    current_dir: Option<PathBuf>,
    snapshot: CurrentSnapshot,
    entries: Vec<TempResource>,
    settings: ImageSettings,
    temp_directory: PathBuf,
    text_budget: TextBudget,
    cancellation: Cancellation,
}

/// 后台迁移完成项；由会话按文档与操作序号校验是否过期。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct MigrationCompletion {
    pub(crate) document: crate::document::DocumentId,
    pub(crate) operation: u64,
    pub(crate) result: Result<ResourceMigration, MigrationError>,
    pub(crate) cancelled: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ResourceMigrationJob {
    /// 与结构字段一一对应；调用方负责冻结快照与条目。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        document: crate::document::DocumentId,
        operation: u64,
        kind: WriteKind,
        target: PathBuf,
        current_dir: Option<PathBuf>,
        snapshot: CurrentSnapshot,
        entries: Vec<TempResource>,
        settings: ImageSettings,
        temp_directory: PathBuf,
        text_budget: TextBudget,
        cancellation: Cancellation,
    ) -> Self {
        Self {
            document,
            operation,
            kind,
            target,
            current_dir,
            snapshot,
            entries,
            settings,
            temp_directory,
            text_budget,
            cancellation,
        }
    }

    /// 执行复制阶段；不做任何文本修改。
    pub(crate) fn run(self) -> MigrationCompletion {
        let document = self.document.clone();
        let operation = self.operation;
        let result = self.execute();
        let cancelled = self.cancellation.is_cancelled();
        MigrationCompletion {
            document,
            operation,
            result,
            cancelled,
        }
    }

    fn execute(&self) -> Result<ResourceMigration, MigrationError> {
        // 非 macOS 必须在任何文件系统副作用之前失败关闭。`cfg!` 是编译期常量，
        // 非 macOS 分支只会在对应平台实际执行，macOS 上不改变任何正常路径。
        if !cfg!(target_os = "macos") {
            return Err(MigrationError::UnsupportedPlatform);
        }
        self.settings.validate()?;
        if !self.target.is_absolute() {
            return Err(MigrationError::InvalidTarget);
        }
        match fs::symlink_metadata(&self.target) {
            Ok(metadata) if metadata.is_dir() => return Err(MigrationError::InvalidTarget),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(MigrationError::InvalidTarget),
        }
        let parent = self.target.parent().ok_or(MigrationError::InvalidTarget)?;
        let target_dir = match &self.settings.directory {
            ImageDirectory::Relative(dir) => parent.join(dir),
            ImageDirectory::Absolute(dir) => dir.clone(),
        };
        ensure_asset_directory(&target_dir)?;
        let source = self
            .snapshot
            .full_text(self.text_budget)
            .map_err(|_| MigrationError::Io)?;
        let referenced: Vec<&TempResource> = self
            .entries
            .iter()
            .filter(|entry| source.contains(&entry.locator))
            .collect();
        let discovered = if self.kind == WriteKind::SaveAs {
            let document_dir = self
                .current_dir
                .as_deref()
                .ok_or(MigrationError::InvalidTarget)?;
            discover_relative_assets(&source, document_dir, &self.settings)
        } else {
            Vec::new()
        };
        if referenced.len().saturating_add(discovered.len()) > MAX_RESOURCE_COUNT {
            return Err(MigrationError::TooManyResources);
        }
        let mut created: Vec<(PathBuf, FileIdentity)> = Vec::new();
        let (copied, mappings) =
            match self.copy_all(&target_dir, &referenced, &discovered, &mut created) {
                Ok(result) => result,
                Err(error) => {
                    rollback_assets(&target_dir, &created);
                    return Err(error);
                }
            };
        if platform_fs::sync_directory(&target_dir).is_err() {
            rollback_assets(&target_dir, &created);
            return Err(MigrationError::Io);
        }
        Ok(ResourceMigration {
            target: self.target.clone(),
            mappings,
            copied,
        })
    }

    /// 逐条复制并建立映射；失败时由调用方统一按身份撤回本次新建文件。
    fn copy_all(
        &self,
        target_dir: &Path,
        referenced: &[&TempResource],
        discovered: &[DiscoveredAsset],
        created: &mut Vec<(PathBuf, FileIdentity)>,
    ) -> Result<(Vec<PathBuf>, Vec<ResourceMapping>), MigrationError> {
        let mut copied = Vec::new();
        let mut mappings = Vec::new();
        let mut total_bytes = 0_u64;
        for entry in referenced {
            check_cancelled(&self.cancellation)?;
            let bytes = read_temp_resource(&self.temp_directory, &entry.file_name)?;
            total_bytes = charge_bytes(total_bytes, bytes.len())?;
            let stem = asset_stem(&entry.display_name, entry.id.value());
            let extension = resource_extension(&entry.display_name);
            let (file_name, _created) =
                place_asset(target_dir, &stem, &extension, &bytes, created)?;
            copied.push(target_dir.join(&file_name));
            mappings.push(ResourceMapping {
                from: entry.locator.clone(),
                to: reference_text(&self.settings, target_dir, &file_name),
            });
            check_cancelled(&self.cancellation)?;
        }
        for asset in discovered {
            check_cancelled(&self.cancellation)?;
            let file_name = asset
                .absolute
                .file_name()
                .ok_or(MigrationError::InvalidTarget)?;
            let bytes = read_ordinary_file(&asset.absolute)?;
            total_bytes = charge_bytes(total_bytes, bytes.len())?;
            let destination = target_dir.join(file_name);
            match fs::symlink_metadata(&destination) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(MigrationError::Io);
                    }
                    let existing = fs::read(&destination).map_err(|_| MigrationError::Io)?;
                    if existing != bytes {
                        return Err(MigrationError::Io);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    write_asset(target_dir, &destination, &bytes, created)
                        .map_err(|_| MigrationError::Io)?;
                }
                Err(_) => return Err(MigrationError::Io),
            }
            copied.push(destination);
            check_cancelled(&self.cancellation)?;
        }
        Ok((copied, mappings))
    }
}

/// 仅当设置为相对目录时发现既有相对资产；结果按文件名排序并去重。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn discover_relative_assets(
    source: &str,
    document_dir: &Path,
    settings: &ImageSettings,
) -> Vec<DiscoveredAsset> {
    let ImageDirectory::Relative(dir) = &settings.directory else {
        return Vec::new();
    };
    let dir = dir.replace('\\', "/");
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Vec::new();
    }
    let prefixes = [format!("./{dir}/"), format!("{dir}/")];
    let mut found: Vec<DiscoveredAsset> = Vec::new();
    for (index, _) in source.match_indices("](") {
        let start = index + 2;
        let rest = &source[start..];
        let Some(end) = rest.find(')') else {
            continue;
        };
        let token = &rest[..end];
        let Some(relative) = prefixes
            .iter()
            .find_map(|prefix| token.strip_prefix(prefix.as_str()))
        else {
            continue;
        };
        if !acceptable_asset_token(token) {
            continue;
        }
        let reference = token.to_owned();
        let absolute = document_dir.join(dir).join(relative);
        if !is_ordinary_file(&absolute) {
            continue;
        }
        if found.iter().any(|asset| asset.absolute == absolute) {
            continue;
        }
        found.push(DiscoveredAsset {
            reference,
            absolute,
        });
    }
    found.sort_by(|left, right| {
        let left_name = left.absolute.file_name().unwrap_or_default();
        let right_name = right.absolute.file_name().unwrap_or_default();
        left_name.cmp(right_name)
    });
    found
}

/// 另存为时发现的既有相对资产；文本引用保持不变，不产生映射。
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct DiscoveredAsset {
    /// 诊断用的原引用文本；复制只使用 `absolute`。
    #[allow(dead_code)]
    pub(crate) reference: String,
    pub(crate) absolute: PathBuf,
}

/// 按单遍扫描把已知 locator 替换为新引用；长匹配优先。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn rewrite_locators(source: &str, mappings: &[ResourceMapping]) -> String {
    let mut result = String::with_capacity(source.len());
    let mut position = 0;
    while position < source.len() {
        let Some(ch) = source[position..].chars().next() else {
            break;
        };
        if let Some(index) = longest_match(&source[position..], mappings) {
            result.push_str(&mappings[index].to);
            position += mappings[index].from.len();
        } else {
            result.push(ch);
            position += ch.len_utf8();
        }
    }
    result
}

/// 返回与 [`rewrite_locators`] 相同的单遍匹配位置；供会话生成逐段替换编辑。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn locator_spans(
    source: &str,
    mappings: &[ResourceMapping],
) -> Vec<(Range<usize>, usize)> {
    let mut spans = Vec::new();
    let mut position = 0;
    while position < source.len() {
        let Some(ch) = source[position..].chars().next() else {
            break;
        };
        if let Some(index) = longest_match(&source[position..], mappings) {
            let length = mappings[index].from.len();
            spans.push((position..position + length, index));
            position += length;
        } else {
            position += ch.len_utf8();
        }
    }
    spans
}

#[cfg_attr(not(test), allow(dead_code))]
fn longest_match(rest: &str, mappings: &[ResourceMapping]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, mapping) in mappings.iter().enumerate() {
        if mapping.from.is_empty() || !rest.starts_with(&mapping.from) {
            continue;
        }
        let better = best.is_none_or(|current| mapping.from.len() > mappings[current].from.len());
        if better {
            best = Some(index);
        }
    }
    best
}

#[cfg_attr(not(test), allow(dead_code))]
fn ensure_asset_directory(path: &Path) -> Result<(), MigrationError> {
    // 目录创建同样是可观察的文件系统副作用；非 macOS 在入口处直接失败关闭。
    if !cfg!(target_os = "macos") {
        return Err(MigrationError::UnsupportedPlatform);
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(MigrationError::InvalidTarget);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|_| MigrationError::InvalidTarget)
        }
        Err(_) => Err(MigrationError::InvalidTarget),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn read_temp_resource(directory: &Path, file_name: &str) -> Result<Vec<u8>, MigrationError> {
    let path = directory.join(file_name);
    platform_fs::verify_ordinary_file(&path).map_err(|_| MigrationError::Io)?;
    read_ordinary_file(&path)
}

#[cfg_attr(not(test), allow(dead_code))]
fn read_ordinary_file(path: &Path) -> Result<Vec<u8>, MigrationError> {
    platform_fs::verify_ordinary_file(path).map_err(|_| MigrationError::Io)?;
    fs::read(path).map_err(|_| MigrationError::Io)
}

#[cfg_attr(not(test), allow(dead_code))]
fn charge_bytes(current: u64, length: usize) -> Result<u64, MigrationError> {
    if length > MAX_RESOURCE_BYTES {
        return Err(MigrationError::BudgetExceeded);
    }
    let length = u64::try_from(length).map_err(|_| MigrationError::BudgetExceeded)?;
    let total = current
        .checked_add(length)
        .ok_or(MigrationError::BudgetExceeded)?;
    if total > MAX_GENERATION_RESOURCE_BYTES as u64 {
        return Err(MigrationError::BudgetExceeded);
    }
    Ok(total)
}

#[cfg_attr(not(test), allow(dead_code))]
fn check_cancelled(cancellation: &Cancellation) -> Result<(), MigrationError> {
    if cancellation.is_cancelled() {
        Err(MigrationError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn asset_stem(display_name: &str, resource: u64) -> String {
    let stem = display_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(display_name);
    let sanitized: String = stem
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    if sanitized.is_empty() {
        format!("image-{resource:016x}")
    } else {
        sanitized
    }
}

/// 选择不冲突的文件名并复制；已存在且字节相同则直接复用。
#[cfg_attr(not(test), allow(dead_code))]
fn place_asset(
    directory: &Path,
    stem: &str,
    extension: &str,
    bytes: &[u8],
    created: &mut Vec<(PathBuf, FileIdentity)>,
) -> Result<(String, bool), MigrationError> {
    for suffix in 0..1000_u32 {
        let file_name = if suffix == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{suffix}.{extension}")
        };
        let target = directory.join(&file_name);
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    continue;
                }
                let existing = fs::read(&target).map_err(|_| MigrationError::Io)?;
                if existing == bytes {
                    return Ok((file_name, false));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match write_asset(directory, &target, bytes, created) {
                    Ok(()) => return Ok((file_name, true)),
                    Err(FsError::AlreadyExists) => {
                        if fs::read(&target).ok().as_deref() == Some(bytes) {
                            return Ok((file_name, false));
                        }
                    }
                    Err(_) => return Err(MigrationError::Io),
                }
            }
            Err(_) => return Err(MigrationError::Io),
        }
    }
    Err(MigrationError::NameExhausted)
}

#[cfg_attr(not(test), allow(dead_code))]
fn write_asset(
    directory: &Path,
    target: &Path,
    bytes: &[u8],
    created: &mut Vec<(PathBuf, FileIdentity)>,
) -> Result<(), FsError> {
    let staged = platform_fs::create_staging(directory, ".aline-asset", bytes)?;
    let published = platform_fs::publish_staged_new(&staged, target);
    let _ = platform_fs::discard_staging(staged);
    let identity = published?;
    created.push((target.to_path_buf(), identity));
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
fn rollback_assets(directory: &Path, created: &[(PathBuf, FileIdentity)]) {
    for (path, identity) in created {
        if platform_fs::file_identity(path).is_ok_and(|current| current == *identity) {
            let _ = fs::remove_file(path);
        }
    }
    let _ = platform_fs::sync_directory(directory);
}

#[cfg_attr(not(test), allow(dead_code))]
fn reference_text(settings: &ImageSettings, target_dir: &Path, file_name: &str) -> String {
    match &settings.directory {
        ImageDirectory::Relative(dir) => {
            let dir = dir.replace('\\', "/");
            let dir = dir.trim_end_matches('/');
            format!("./{dir}/{}", percent_encode(file_name))
        }
        ImageDirectory::Absolute(_) => {
            let path = target_dir.join(file_name);
            format!("<{}>", path.to_string_lossy().replace('\\', "/"))
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn percent_encode(file_name: &str) -> String {
    let mut encoded = String::with_capacity(file_name.len());
    for ch in file_name.chars() {
        match ch {
            ' ' => encoded.push_str("%20"),
            '(' => encoded.push_str("%28"),
            ')' => encoded.push_str("%29"),
            '<' => encoded.push_str("%3C"),
            '>' => encoded.push_str("%3E"),
            '#' => encoded.push_str("%23"),
            '%' => encoded.push_str("%25"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

#[cfg_attr(not(test), allow(dead_code))]
fn acceptable_asset_token(token: &str) -> bool {
    !token.is_empty()
        && !token.contains("..")
        && !token
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '<' || ch == '>' || ch.is_control())
}

#[cfg_attr(not(test), allow(dead_code))]
fn is_ordinary_file(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) => !metadata.file_type().is_symlink() && metadata.is_file(),
        Err(_) => false,
    }
}
