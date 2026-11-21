//! 首次保存/另存为的资源迁移：把临时资源复制到文档资产目录，并生成引用重写
//! 计划。迁移只读写资产目录与临时根，不修改文本；引用重写由会话层通过编辑协议
//! 提交，保证一次历史步且可撤销。
//!
//! 复制阶段在后台执行，只持有快照、条目快照、设置与取消句柄。

use std::fs;
use std::io::Read;
use std::ops::Range;
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use aline_buffer::{HistoryStepHandle, TextBudget, TextSnapshot};
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

/// 源码历史的只读共享句柄；复制任务只重放补丁，不拥有可变 Buffer 或历史。
pub(crate) struct ResourceHistory {
    pub(crate) snapshot: TextSnapshot,
    pub(crate) nodes: Vec<Vec<HistoryStepHandle>>,
    pub(crate) cursor: usize,
    pub(crate) typing_redo: Option<TextSnapshot>,
}

/// 已复制资产的路径级复验凭据：身份（dev/ino 等）＋冻结字节。
///
/// 残余窗口：verify→read→verify 能识别换名/替换（身份变化）与内容差异，但
/// 「同 inode 原地改写后又在读取间隙改回」无法用路径级 stat 根治（需持 fd）。
/// 该误判方向安全：判「未变」时目标终态仍等于冻结字节，不产生坏引用；
/// 反向误判只导致保守拒绝重试。不得把本凭据当作跨进程强一致保证。
#[derive(Clone, Debug)]
struct AssetProof {
    path: PathBuf,
    identity: FileIdentity,
    bytes: Arc<[u8]>,
}

impl AssetProof {
    fn capture(path: PathBuf, bytes: Vec<u8>) -> Result<Self, MigrationError> {
        let identity = platform_fs::verify_ordinary_file(&path).map_err(|_| MigrationError::Io)?;
        if read_ordinary_file(&path)? != bytes
            || platform_fs::verify_ordinary_file(&path).map_err(|_| MigrationError::Io)? != identity
        {
            return Err(MigrationError::StaleResourceSet);
        }
        Ok(Self {
            path,
            identity,
            bytes: bytes.into(),
        })
    }

    fn unchanged(&self) -> bool {
        platform_fs::verify_ordinary_file(&self.path)
            .is_ok_and(|identity| identity == self.identity)
            && read_ordinary_file(&self.path).is_ok_and(|bytes| bytes == self.bytes.as_ref())
            && platform_fs::verify_ordinary_file(&self.path)
                .is_ok_and(|identity| identity == self.identity)
    }
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
    /// 当前文本与全部可撤销/可重做历史逐步骤状态中已复制的相对引用，
    /// 供写盘前/结算时复验资源覆盖；被裁剪节点与已截断 redo 不在可达集合内。
    pub(crate) relative_references: Vec<String>,
    proofs: Vec<AssetProof>,
    /// 本次迁移新建的资产及创建时身份；供重写失败时按身份撤回。
    created: Vec<(PathBuf, FileIdentity)>,
    /// 资产目录；回滚后同步目录项。
    asset_dir: PathBuf,
    /// 资产目录是否由本次迁移创建；仅此类空目录可在回滚时删除。
    created_asset_dir: bool,
}

impl ResourceMigration {
    pub(crate) fn assets_unchanged(&self) -> bool {
        self.proofs.iter().all(AssetProof::unchanged)
    }
    /// 最新源码与逐步骤历史状态不得引入复制清单外的相对资产。
    /// 覆盖按归一化引用字符串比较（方向保守：同文件不同写法会触发重试）。
    pub(crate) fn covers_relative_resources(
        &self,
        source: &str,
        history: &ResourceHistory,
        directory: &Path,
        settings: &ImageSettings,
        budget: TextBudget,
    ) -> bool {
        let covers = |text: &str| {
            discover_relative_assets(text, directory, settings)
                .iter()
                .all(|asset| {
                    self.relative_references.iter().any(|known| {
                        known.trim_start_matches("./") == asset.reference.trim_start_matches("./")
                    })
                })
        };
        let mut covered = covers(source);
        visit_history_sources(history, budget, |historical| covered &= covers(historical)).is_ok()
            && covered
    }

    /// Markdown 已发布后这些资产可能被侧目标引用，不能再按准备失败撤回。
    pub(crate) fn protect_published_assets(&mut self) {
        self.created.clear();
        self.created_asset_dir = false;
    }

    /// 撤回本次迁移新建且身份未变化的资产；已存在或非本次创建的不动。
    ///
    /// 清理失败只保留残留，不向调用方伪造成功。
    pub(crate) fn rollback_assets(&self) {
        rollback_assets(&self.asset_dir, &self.created, self.created_asset_dir);
    }
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
    history: Option<ResourceHistory>,
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
            history: None,
        }
    }

    pub(crate) fn with_history(mut self, history: ResourceHistory) -> Self {
        self.history = Some(history);
        self
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
            let document_dir = match self.current_dir.as_deref() {
                Some(directory) => directory,
                None => return Err(MigrationError::InvalidTarget),
            };
            let mut assets = discover_relative_assets(&source, document_dir, &self.settings);
            if let Some(history) = &self.history {
                let visited = visit_history_sources(history, self.text_budget, |historical| {
                    for asset in discover_relative_assets(historical, document_dir, &self.settings)
                    {
                        if !assets.iter().any(|known| known.absolute == asset.absolute) {
                            assets.push(asset);
                        }
                    }
                });
                visited?;
            }
            assets
        } else {
            Vec::new()
        };
        if referenced.len().saturating_add(discovered.len()) > MAX_RESOURCE_COUNT {
            return Err(MigrationError::TooManyResources);
        }
        // M2 只迁移配置资产目录内的平面文件；不能把子目录文件扁平复制后保留坏引用。
        if let (Some(current_dir), ImageDirectory::Relative(dir)) =
            (&self.current_dir, &self.settings.directory)
        {
            let asset_dir = current_dir.join(dir);
            if discovered
                .iter()
                .any(|asset| asset.absolute.parent() != Some(asset_dir.as_path()))
            {
                return Err(MigrationError::InvalidTarget);
            }
        }
        let created_asset_dir = ensure_asset_directory(&target_dir)?;
        let mut created: Vec<(PathBuf, FileIdentity)> = Vec::new();
        let copied_assets = match self.copy_all(&target_dir, &referenced, &discovered, &mut created)
        {
            Ok(result) => result,
            Err(error) => {
                rollback_assets(&target_dir, &created, created_asset_dir);
                return Err(error);
            }
        };
        if platform_fs::sync_directory(&target_dir).is_err() {
            rollback_assets(&target_dir, &created, created_asset_dir);
            return Err(MigrationError::Io);
        }
        Ok(ResourceMigration {
            target: self.target.clone(),
            mappings: copied_assets.mappings,
            copied: copied_assets.copied,
            relative_references: discovered
                .iter()
                .map(|asset| asset.reference.clone())
                .collect(),
            proofs: copied_assets.proofs,
            created,
            asset_dir: target_dir,
            created_asset_dir,
        })
    }

    /// 逐条复制并建立映射；失败时由调用方统一按身份撤回本次新建文件。
    fn copy_all(
        &self,
        target_dir: &Path,
        referenced: &[&TempResource],
        discovered: &[DiscoveredAsset],
        created: &mut Vec<(PathBuf, FileIdentity)>,
    ) -> Result<CopiedAssets, MigrationError> {
        let mut copied = Vec::new();
        let mut mappings = Vec::new();
        let mut proofs = Vec::new();
        let mut total_bytes = 0_u64;
        for entry in referenced {
            check_cancelled(&self.cancellation)?;
            let bytes = read_temp_resource(&self.temp_directory, &entry.file_name)?;
            total_bytes = charge_bytes(total_bytes, bytes.len())?;
            let stem = asset_stem(&entry.display_name, entry.id.value());
            let extension = resource_extension(&entry.display_name);
            let (file_name, _created) =
                place_asset(target_dir, &stem, &extension, &bytes, created)?;
            let path = target_dir.join(&file_name);
            copied.push(path.clone());
            proofs.push(AssetProof::capture(path, bytes)?);
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
                    let existing = read_ordinary_file(&destination)?;
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
            copied.push(destination.clone());
            proofs.push(AssetProof::capture(destination, bytes)?);
            check_cancelled(&self.cancellation)?;
        }
        Ok(CopiedAssets {
            copied,
            mappings,
            proofs,
        })
    }
}

/// 一次复制阶段的三份产物：就位路径、引用映射与逐字节复验凭据。
struct CopiedAssets {
    copied: Vec<PathBuf>,
    mappings: Vec<ResourceMapping>,
    proofs: Vec<AssetProof>,
}

/// 每次只读重放一个历史节点；输出用于后台发现资源，不形成可提交全文或新历史。
fn visit_history_sources(
    history: &ResourceHistory,
    budget: TextBudget,
    mut visit: impl FnMut(&str),
) -> Result<(), MigrationError> {
    let base_range = history
        .snapshot
        .src_range(0..history.snapshot.len_bytes())
        .map_err(|_| MigrationError::StaleResourceSet)?;
    let mut current = history
        .snapshot
        .slice(&base_range, budget)
        .map_err(|_| MigrationError::BudgetExceeded)?
        .into_owned();
    for node in history
        .nodes
        .get(..history.cursor)
        .ok_or(MigrationError::StaleResourceSet)?
        .iter()
        .rev()
    {
        for step in node.iter().rev() {
            replay_history(
                &mut current,
                step.get().inverse_patches(),
                step.get().after_len(),
                budget,
            )?;
            // 多步骤节点的每个中间态都可经逐步撤销/重做到达，逐步骤访问。
            visit(&current);
        }
    }
    current = history
        .snapshot
        .slice(&base_range, budget)
        .map_err(|_| MigrationError::BudgetExceeded)?
        .into_owned();
    for node in history
        .nodes
        .get(history.cursor..)
        .ok_or(MigrationError::StaleResourceSet)?
    {
        for step in node {
            replay_history(
                &mut current,
                step.get().forward_patches(),
                step.get().before_len(),
                budget,
            )?;
            // redo 分支同样逐步骤访问，覆盖多步骤节点的中间态。
            visit(&current);
        }
    }
    if let Some(redo) = &history.typing_redo {
        let range = redo
            .src_range(0..redo.len_bytes())
            .map_err(|_| MigrationError::StaleResourceSet)?;
        current = redo
            .slice(&range, budget)
            .map_err(|_| MigrationError::BudgetExceeded)?
            .into_owned();
        visit(&current);
    }
    Ok(())
}

fn replay_history(
    source: &mut String,
    patches: &[(Range<usize>, String)],
    expected_len: usize,
    budget: TextBudget,
) -> Result<(), MigrationError> {
    if source.len() != expected_len {
        return Err(MigrationError::StaleResourceSet);
    }
    for (range, replacement) in patches.iter().rev() {
        if range.start > range.end
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return Err(MigrationError::StaleResourceSet);
        }
        let len = source
            .len()
            .checked_sub(range.len())
            .and_then(|len| len.checked_add(replacement.len()))
            .ok_or(MigrationError::BudgetExceeded)?;
        if len > budget.max_bytes() {
            return Err(MigrationError::BudgetExceeded);
        }
        source.replace_range(range.clone(), replacement);
    }
    Ok(())
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

/// 确保资产目录可用；返回本次是否新建该目录，供失败回滚只删本次创建的空目录。
fn ensure_asset_directory(path: &Path) -> Result<bool, MigrationError> {
    // 目录创建同样是可观察的文件系统副作用；非 macOS 在入口处直接失败关闭。
    if !cfg!(target_os = "macos") {
        return Err(MigrationError::UnsupportedPlatform);
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(MigrationError::InvalidTarget);
            }
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|_| MigrationError::InvalidTarget)?;
            Ok(true)
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
    let identity = platform_fs::verify_ordinary_file(path).map_err(|_| MigrationError::Io)?;
    let file = fs::File::open(path).map_err(|_| MigrationError::Io)?;
    let metadata = file.metadata().map_err(|_| MigrationError::Io)?;
    if !metadata.is_file() || metadata.len() > MAX_RESOURCE_BYTES as u64 {
        return Err(MigrationError::BudgetExceeded);
    }
    #[cfg(target_os = "macos")]
    if metadata.dev() != identity.dev || metadata.ino() != identity.ino {
        return Err(MigrationError::StaleResourceSet);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(metadata.len()).map_err(|_| MigrationError::BudgetExceeded)?,
        )
        .map_err(|_| MigrationError::BudgetExceeded)?;
    let mut reader = file.take(MAX_RESOURCE_BYTES as u64 + 1);
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|_| MigrationError::Io)?;
        if read == 0 {
            break;
        }
        if bytes
            .len()
            .checked_add(read)
            .is_none_or(|len| len > MAX_RESOURCE_BYTES)
        {
            return Err(MigrationError::BudgetExceeded);
        }
        bytes
            .try_reserve_exact(read)
            .map_err(|_| MigrationError::BudgetExceeded)?;
        bytes.extend_from_slice(&chunk[..read]);
    }
    if platform_fs::verify_ordinary_file(path).map_err(|_| MigrationError::Io)? != identity {
        return Err(MigrationError::StaleResourceSet);
    }
    Ok(bytes)
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
                if metadata.len() != bytes.len() as u64 {
                    continue;
                }
                let existing = read_ordinary_file(&target)?;
                if existing == bytes {
                    return Ok((file_name, false));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match write_asset(directory, &target, bytes, created) {
                    Ok(()) => return Ok((file_name, true)),
                    Err(FsError::AlreadyExists) => {
                        if read_ordinary_file(&target).ok().as_deref() == Some(bytes) {
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

fn rollback_assets(directory: &Path, created: &[(PathBuf, FileIdentity)], remove_empty_dir: bool) {
    for (path, identity) in created {
        if platform_fs::file_identity(path).is_ok_and(|current| current == *identity) {
            let _ = fs::remove_file(path);
        }
    }
    // 仅本次新建且已空的目录可删除；既有或非空目录保持原状，失败只留残留。
    if remove_empty_dir && fs::remove_dir(directory).is_ok() {
        if let Some(parent) = directory.parent() {
            let _ = platform_fs::sync_directory(parent);
        }
        return;
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
