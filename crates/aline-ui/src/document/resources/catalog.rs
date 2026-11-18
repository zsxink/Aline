//! 临时资源目录：文档私有的资源条目表与磁盘文件。`locator` 是文本中可出现的
//! 稳定引用键，磁盘文件名由资源序号与扩展名组成。普通 `stage` 只写临时根；
//! 恢复专用的 `materialize_recovered`/`restore` 只复验既有材料，不推进普通分配。
//!
//! 本模块不登记 `DocCore`、不修改文本；登记与会话状态由调用方完成。

use std::fs;
use std::path::{Path, PathBuf};

use aline_source::fs::{self as platform_fs, FsError};

use super::temp_root::{TempDocumentId, TempRoot, TempRootError};
use crate::document::recovery::{
    MAX_REFERENCE_BYTES, MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT, RecoveredResource,
};
use crate::document::{DocError, DocumentId, ResourceReference};

/// locator 的固定前缀；其余两段为 16 位小写十六进制。
const LOCATOR_PREFIX: &str = "aline-temp:";
/// 单条目磁盘名分配的最大连续尝试次数。
#[cfg_attr(not(test), allow(dead_code))]
const MAX_STAGE_ATTEMPTS: u64 = 8;

/// 临时资源序号；只由目录分配或恢复材料解析得到。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TempResourceId(u64);

#[cfg_attr(not(test), allow(dead_code))]
impl TempResourceId {
    /// 返回原始数值，供 locator 编码或诊断。
    pub(crate) const fn value(self) -> u64 {
        self.0
    }
}

/// 已写入临时根并登记到目录的一条资源；不延长文档生命周期。
#[derive(Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct TempResource {
    pub(crate) reference: ResourceReference,
    pub(crate) id: TempResourceId,
    pub(crate) locator: String,
    pub(crate) file_name: String,
    pub(crate) display_name: String,
    /// 恢复与诊断读取；普通迁移路径只按临时文件实际字节计费。
    #[allow(dead_code)]
    pub(crate) len_bytes: u64,
}

/// 临时资源目录的失败分类；`Doc` 只在会话登记引用失败时出现。
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ResourceError {
    UnsupportedPlatform,
    TooLarge,
    ReferenceTooLong,
    CountExceeded,
    InvalidName,
    Io,
    NotEditable,
    WrongDocument,
    /// 载荷只用于诊断映射；测试按变体比较，不读取内部 `DocError`。
    #[allow(dead_code)]
    Doc(DocError),
}

// `DocError` 的载荷类型不实现 `PartialEq`，因此这里按变体比较；
// 错误只用于分类判断，不参与内容等价或持久化。
impl PartialEq for ResourceError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::UnsupportedPlatform, Self::UnsupportedPlatform)
                | (Self::TooLarge, Self::TooLarge)
                | (Self::ReferenceTooLong, Self::ReferenceTooLong)
                | (Self::CountExceeded, Self::CountExceeded)
                | (Self::InvalidName, Self::InvalidName)
                | (Self::Io, Self::Io)
                | (Self::NotEditable, Self::NotEditable)
                | (Self::WrongDocument, Self::WrongDocument)
                | (Self::Doc(_), Self::Doc(_))
        )
    }
}

impl Eq for ResourceError {}

/// 文档私有的临时资源条目表；`directory` 由 `TempRoot` 独占分配。
#[derive(Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResourceCatalog {
    document: DocumentId,
    temp_document: TempDocumentId,
    directory: PathBuf,
    entries: Vec<TempResource>,
    next_id: u64,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ResourceCatalog {
    /// 为文档独占分配临时目录；失败时目录表为空。
    pub(crate) fn new(
        document: DocumentId,
        root: &TempRoot,
        candidate_nanos: u64,
    ) -> Result<Self, ResourceError> {
        let (temp_document, directory) =
            root.allocate_document(candidate_nanos)
                .map_err(|error| match error {
                    TempRootError::UnsupportedPlatform => ResourceError::UnsupportedPlatform,
                    _ => ResourceError::Io,
                })?;
        Ok(Self {
            document,
            temp_document,
            directory,
            entries: Vec::new(),
            next_id: 0,
        })
    }

    /// 写入一条资源并建立条目。本方法不登记 `DocCore`、不修改文本；
    /// 失败时不留条目，已写入的临时文件按身份撤回。
    pub(crate) fn stage(
        &mut self,
        display_name: &str,
        bytes: &[u8],
        max_bytes: u64,
    ) -> Result<TempResource, ResourceError> {
        if bytes.len() > MAX_RESOURCE_BYTES
            || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes
        {
            return Err(ResourceError::TooLarge);
        }
        if self.entries.len() >= MAX_RESOURCE_COUNT {
            return Err(ResourceError::CountExceeded);
        }
        let display_name = sanitize_display_name(display_name)?;
        let extension = resource_extension(display_name);
        let mut next = self.next_id;
        for _ in 0..MAX_STAGE_ATTEMPTS {
            let id = TempResourceId(next);
            next = next.checked_add(1).ok_or(ResourceError::Io)?;
            let disk_name = format!("{:016x}.{extension}", id.0);
            let locator = format!(
                "{LOCATOR_PREFIX}{}/{disk_name}",
                self.temp_document.directory_name()
            );
            if locator.len() > MAX_REFERENCE_BYTES {
                return Err(ResourceError::ReferenceTooLong);
            }
            let staged = match platform_fs::create_staging(&self.directory, "img", bytes) {
                Ok(staged) => staged,
                Err(FsError::AlreadyExists) => continue,
                Err(error) => return Err(map_fs(error)),
            };
            let target = self.directory.join(&disk_name);
            match platform_fs::publish_staged_new(&staged, &target) {
                Ok(_identity) => {
                    let _ = platform_fs::discard_staging(staged);
                    self.next_id = next;
                    let resource = TempResource {
                        reference: ResourceReference::new(self.document.clone(), locator.clone()),
                        id,
                        locator,
                        file_name: disk_name,
                        display_name: display_name.to_owned(),
                        len_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    };
                    self.entries.push(resource.clone());
                    return Ok(resource);
                }
                Err(FsError::AlreadyExists) => {
                    let _ = platform_fs::discard_staging(staged);
                    continue;
                }
                Err(error) => {
                    let _ = platform_fs::discard_staging(staged);
                    return Err(map_fs(error));
                }
            }
        }
        Err(ResourceError::Io)
    }

    pub(crate) fn entries(&self) -> &[TempResource] {
        &self.entries
    }

    #[allow(dead_code)]
    pub(crate) fn get(&self, locator: &str) -> Option<&TempResource> {
        self.entries.iter().find(|entry| entry.locator == locator)
    }

    pub(crate) fn remove(&mut self, id: TempResourceId) -> Option<TempResource> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(index))
    }

    /// 删除一条刚从目录移出的临时文件；只用于登记失败回滚，非普通文件保留不动。
    pub(crate) fn discard_file(&self, resource: &TempResource) -> Result<(), ResourceError> {
        let path = self.directory.join(&resource.file_name);
        platform_fs::verify_ordinary_file(&path).map_err(map_fs)?;
        fs::remove_file(&path).map_err(|_| ResourceError::Io)?;
        platform_fs::sync_directory(&self.directory).map_err(map_fs)
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn temp_document(&self) -> TempDocumentId {
        self.temp_document
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 恢复专用：后台任务已把内嵌资源按原 locator 写回临时根，本方法只复验
    /// 目录与文件并建立条目，不写盘、不改动既有文件内容。
    #[allow(dead_code)]
    pub(crate) fn restore(
        document: DocumentId,
        root: &TempRoot,
        temp_document: TempDocumentId,
        locators: &[String],
    ) -> Result<Self, ResourceError> {
        #[cfg(target_os = "macos")]
        {
            let mut parsed = Vec::new();
            for locator in locators {
                let Some(part) = parse_locator(locator) else {
                    return Err(ResourceError::WrongDocument);
                };
                if part.document != temp_document.value() {
                    return Err(ResourceError::WrongDocument);
                }
                parsed.push((part, locator));
            }
            if parsed.len() > MAX_RESOURCE_COUNT {
                return Err(ResourceError::CountExceeded);
            }
            let directory = root.path().join(temp_document.directory_name());
            verify_document_directory(&directory, root.owner_uid())?;
            let mut entries = Vec::new();
            let mut next_id = 0_u64;
            for (part, locator) in parsed {
                let file_name = part.file_name();
                let path = directory.join(&file_name);
                platform_fs::verify_ordinary_file(&path).map_err(map_fs)?;
                let len_bytes = metadata_len(&path)?;
                if len_bytes > MAX_RESOURCE_BYTES as u64 {
                    return Err(ResourceError::TooLarge);
                }
                next_id = next_id.max(part.resource.saturating_add(1));
                entries.push(TempResource {
                    reference: ResourceReference::new(document.clone(), locator.clone()),
                    id: TempResourceId(part.resource),
                    locator: locator.clone(),
                    file_name: file_name.clone(),
                    display_name: file_name,
                    len_bytes,
                });
            }
            Ok(Self {
                document,
                temp_document,
                directory,
                entries,
                next_id,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (document, root, temp_document, locators);
            Err(ResourceError::UnsupportedPlatform)
        }
    }
}

/// 恢复专用：把恢复记录内嵌的资源按原 locator 写回临时根。所有引用必须指向
/// 同一临时文档；目录与文件都是 create-only，既有同名字节相同才复用。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn materialize_recovered(
    root: &TempRoot,
    resources: &[RecoveredResource],
) -> Result<TempDocumentId, ResourceError> {
    #[cfg(target_os = "macos")]
    {
        if resources.is_empty() || resources.len() > MAX_RESOURCE_COUNT {
            return Err(ResourceError::WrongDocument);
        }
        let mut parsed = Vec::new();
        let mut document = None;
        for resource in resources {
            let Some(part) = parse_locator(&resource.reference) else {
                return Err(ResourceError::WrongDocument);
            };
            if let Some(expected) = document {
                if expected != part.document {
                    return Err(ResourceError::WrongDocument);
                }
            } else {
                document = Some(part.document);
            }
            if resource.bytes.len() > MAX_RESOURCE_BYTES {
                return Err(ResourceError::TooLarge);
            }
            parsed.push((part, resource));
        }
        let document = document.ok_or(ResourceError::WrongDocument)?;
        let temp_document = TempDocumentId::from_raw(document);
        let directory = root.path().join(temp_document.directory_name());
        if directory.try_exists().map_err(|_| ResourceError::Io)? {
            verify_document_directory(&directory, root.owner_uid())?;
        } else {
            match platform_fs::create_directory_exclusive(&directory) {
                Ok(_) => platform_fs::sync_directory(root.path()).map_err(map_fs)?,
                Err(FsError::AlreadyExists) => {}
                Err(error) => return Err(map_fs(error)),
            }
            verify_document_directory(&directory, root.owner_uid())?;
        }
        let mut created: Vec<(PathBuf, platform_fs::FileIdentity)> = Vec::new();
        for (part, resource) in parsed {
            let target = directory.join(part.file_name());
            match fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        let _ = rollback_files(&directory, &created);
                        return Err(ResourceError::Io);
                    }
                    let existing = match fs::read(&target) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            let _ = rollback_files(&directory, &created);
                            return Err(ResourceError::Io);
                        }
                    };
                    if existing != resource.bytes {
                        let _ = rollback_files(&directory, &created);
                        return Err(ResourceError::Io);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let staged =
                        match platform_fs::create_staging(&directory, "img", &resource.bytes) {
                            Ok(staged) => staged,
                            Err(error) => {
                                let _ = rollback_files(&directory, &created);
                                return Err(map_fs(error));
                            }
                        };
                    match platform_fs::publish_staged_new(&staged, &target) {
                        Ok(identity) => {
                            let _ = platform_fs::discard_staging(staged);
                            created.push((target, identity));
                        }
                        Err(FsError::AlreadyExists) => {
                            let _ = platform_fs::discard_staging(staged);
                            let existing = match fs::read(&target) {
                                Ok(bytes) => bytes,
                                Err(_) => {
                                    let _ = rollback_files(&directory, &created);
                                    return Err(ResourceError::Io);
                                }
                            };
                            if existing != resource.bytes {
                                let _ = rollback_files(&directory, &created);
                                return Err(ResourceError::Io);
                            }
                        }
                        Err(error) => {
                            let _ = platform_fs::discard_staging(staged);
                            let _ = rollback_files(&directory, &created);
                            return Err(map_fs(error));
                        }
                    }
                }
                Err(_) => {
                    let _ = rollback_files(&directory, &created);
                    return Err(ResourceError::Io);
                }
            }
        }
        platform_fs::sync_directory(&directory).map_err(map_fs)?;
        Ok(temp_document)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (root, resources);
        Err(ResourceError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn rollback_files(
    directory: &Path,
    created: &[(PathBuf, platform_fs::FileIdentity)],
) -> Result<(), FsError> {
    for (path, identity) in created {
        if platform_fs::file_identity(path).is_ok_and(|current| current == *identity) {
            let _ = fs::remove_file(path);
        }
    }
    platform_fs::sync_directory(directory)
}

/// 严格解析 `aline-temp:{document:016x}/{resource:016x}.{ext}`。
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct ParsedLocator {
    pub(super) document: u64,
    pub(super) resource: u64,
    pub(super) extension: String,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ParsedLocator {
    pub(super) fn document_name(&self) -> String {
        format!("{:016x}", self.document)
    }

    pub(super) fn file_name(&self) -> String {
        format!("{:016x}.{}", self.resource, self.extension)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn parse_locator(locator: &str) -> Option<ParsedLocator> {
    if locator.len() > MAX_REFERENCE_BYTES {
        return None;
    }
    let rest = locator.strip_prefix(LOCATOR_PREFIX)?;
    let (document, rest) = rest.split_once('/')?;
    let (resource, extension) = rest.split_once('.')?;
    let document = parse_hex16(document)?;
    let resource = parse_hex16(resource)?;
    if extension.is_empty()
        || extension.len() > 8
        || !extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return None;
    }
    Some(ParsedLocator {
        document,
        resource,
        extension: extension.to_owned(),
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_hex16(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() != 16 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    if bytes.iter().any(u8::is_ascii_uppercase) {
        return None;
    }
    u64::from_str_radix(value, 16).ok()
}

#[cfg_attr(not(test), allow(dead_code))]
fn sanitize_display_name(display_name: &str) -> Result<&str, ResourceError> {
    let name = display_name
        .rsplit(['/', '\\'])
        .next()
        .ok_or(ResourceError::InvalidName)?;
    if name.is_empty() || name == "." || name == ".." || name.contains('\0') {
        return Err(ResourceError::InvalidName);
    }
    Ok(name)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn resource_extension(display_name: &str) -> String {
    display_name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            (1..=8).contains(&extension.len())
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".to_owned())
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn verify_document_directory(path: &Path, expected_uid: u32) -> Result<(), ResourceError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).map_err(|_| ResourceError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ResourceError::Io);
    }
    if metadata.uid() != expected_uid {
        return Err(ResourceError::Io);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn metadata_len(path: &Path) -> Result<u64, ResourceError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|_| ResourceError::Io)
}

#[cfg_attr(not(test), allow(dead_code))]
fn map_fs(error: FsError) -> ResourceError {
    match error {
        FsError::UnsupportedPlatform => ResourceError::UnsupportedPlatform,
        FsError::InvalidPath
        | FsError::NotFound
        | FsError::NotOrdinary
        | FsError::Symlink
        | FsError::AlreadyExists
        | FsError::WouldBlock
        | FsError::IdentityChanged
        | FsError::Io { .. } => ResourceError::Io,
    }
}
