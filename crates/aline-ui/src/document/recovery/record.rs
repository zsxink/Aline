//! W4 恢复清单 V1 的纯字节编码、解码与文件内容核验，供恢复存储使用。
//! 输入为已限长读取的清单字节及同代文件字节。借用原始路径和资源键，不持有文件、
//! 历史、路径授权或发布事实。目录身份、磁盘同步与恢复采纳由存储层负责。

// 4.1a 只提供记录层与存储层；生产接线（4.1b/4.2）到来前，编码器与访问器
// 仅由 crate 内测试使用。该豁免只覆盖本模块，接线后必须复核删除。
#![cfg_attr(not(test), allow(dead_code))]

use std::num::NonZeroU64;

const MIB: usize = 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: usize = MIB;
pub(crate) const HEADER_BYTES: usize = 24;
pub(crate) const MAX_TEXT_BYTES: usize = 10 * MIB;
pub(crate) const MAX_PATH_BYTES: usize = 32 * 1024;
pub(crate) const MAX_RESOURCE_COUNT: usize = 256;
pub(crate) const MAX_REFERENCE_BYTES: usize = 4096;
pub(crate) const MAX_RESOURCE_BYTES: usize = 64 * MIB;
pub(crate) const MAX_GENERATION_RESOURCE_BYTES: usize = 256 * MIB;
pub(crate) const MAX_COMPLETE_GENERATIONS: usize = 2;
pub(crate) const MAX_INCOMPLETE_RESERVATIONS: usize = 32;
pub(crate) const MAX_ACTIVE_LINEAGES: usize = 256;
pub(crate) const ROOT_PAYLOAD_LIMIT: u64 = 1024 * MIB as u64;
pub(crate) const LINEAGE_RESERVE_BYTES: u64 = 80;
pub(crate) const NEXT_ID_BYTES: u64 = 32;
pub(crate) const RESERVATION_BYTES: u64 = 40;
pub(crate) const RETIRED_MANIFEST_BYTES: u64 = 48;
pub(crate) const RECOVERY_INTERVAL_MILLIS: u64 = 10_000;
const MANIFEST_MAGIC: &[u8; 8] = b"ALINEREC";

/// 保留未知编码与损坏的区别，供协调器决定可否操作谱系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordError {
    UnsupportedVersion,
    UnsupportedKind,
    UnsupportedFlags,
    UnsupportedPath,
    Truncated,
    TrailingBytes,
    InvalidMagic,
    InvalidIdentity,
    InvalidPath,
    InvalidPresence,
    InvalidSelection,
    InvalidResource,
    InvalidText,
    LengthMismatch,
    ChecksumMismatch,
    Budget,
    Allocation,
}

/// 记录错误的处置分类：未知格式保留原 bytes 并禁写禁清，损坏隔离该代，
/// 本地分配失败不判格式并可重试。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordDefect {
    /// 当前 V1 实现不理解的版本、kind、flags、路径或 magic。
    UnknownFormat,
    /// 结构、身份、长度、CRC 或预算损坏。
    Corrupt,
    /// 本地分配失败，尚未判断格式。
    LocalFailure,
}

impl RecordError {
    pub(crate) const fn defect(self) -> RecordDefect {
        use RecordError::*;
        match self {
            UnsupportedVersion | UnsupportedKind | UnsupportedFlags | UnsupportedPath
            | InvalidMagic => RecordDefect::UnknownFormat,
            Allocation => RecordDefect::LocalFailure,
            Truncated | TrailingBytes | InvalidIdentity | InvalidPath | InvalidPresence
            | InvalidSelection | InvalidResource | InvalidText | LengthMismatch
            | ChecksumMismatch | Budget => RecordDefect::Corrupt,
        }
    }
}

pub(crate) enum RecoveryRecord<'a> {
    Snapshot(SnapshotRecord<'a>),
    Retired(RetiredRecord),
}

#[derive(Debug, Eq, PartialEq)]
struct RecordId {
    document: NonZeroU64,
    generation: NonZeroU64,
}

pub(crate) struct SnapshotRecord<'a> {
    id: RecordId,
    parent_generation: Option<NonZeroU64>,
    path_generation: u64,
    // 仅记录捕获时的组合态，恢复时应呈现普通文本。
    captured_composition: bool,
    path: RecordedPath<'a>,
    selection: RecordedSelection,
    source: FileDescriptor,
    logical_saved: Option<FileDescriptor>,
    baseline: Option<FileDescriptor>,
    resources: Vec<ResourceRecord<'a>>,
}

pub(crate) struct RetiredRecord {
    id: RecordId,
    retired_through_generation: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RecordedPath<'a> {
    Unlinked,
    Unix(&'a [u8]),
    WindowsUtf16Le(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordedSelection {
    anchor_byte: usize,
    head_byte: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileDescriptor {
    len_bytes: usize,
    crc32: u32,
}

pub(crate) struct ResourceRecord<'a> {
    ordinal: NonZeroU64,
    reference: &'a str,
    bytes: FileDescriptor,
}

impl<'a> RecoveryRecord<'a> {
    pub(crate) fn decode(bytes: &'a [u8]) -> Result<Self, RecordError> {
        let mut header = Wire::new(bytes);
        if &header.array::<8>()? != MANIFEST_MAGIC {
            return Err(RecordError::InvalidMagic);
        }
        if header.u16()? != 1 {
            return Err(RecordError::UnsupportedVersion);
        }
        let kind = header.byte()?;
        if kind > 1 {
            return Err(RecordError::UnsupportedKind);
        }
        if header.byte()? != 0 {
            return Err(RecordError::UnsupportedFlags);
        }
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(RecordError::Budget);
        }
        let length = header.bounded_len((MAX_MANIFEST_BYTES - HEADER_BYTES) as u64)?;
        let checksum = header.u32()?;
        let payload = header.take(length)?;
        header.finish()?;
        if crc32(payload) != checksum {
            return Err(RecordError::ChecksumMismatch);
        }
        let mut body = Wire::new(payload);
        let id = RecordId {
            document: body.nonzero()?,
            generation: body.nonzero()?,
        };
        let record = if kind == 0 {
            Self::Snapshot(SnapshotRecord::decode(id, &mut body)?)
        } else {
            let retired_through_generation = body.u64()?;
            if retired_through_generation.checked_add(1) != Some(id.generation.get()) {
                return Err(RecordError::InvalidIdentity);
            }
            Self::Retired(RetiredRecord {
                id,
                retired_through_generation,
            })
        };
        body.finish()?;
        Ok(record)
    }
}

impl<'a> SnapshotRecord<'a> {
    fn decode(id: RecordId, body: &mut Wire<'a>) -> Result<Self, RecordError> {
        let parent_generation = NonZeroU64::new(body.u64()?);
        if parent_generation.is_some_and(|parent| parent >= id.generation) {
            return Err(RecordError::InvalidIdentity);
        }
        let path_generation = body.u64()?;
        let captured_composition = match body.byte()? {
            0 => false,
            1 => true,
            _ => return Err(RecordError::UnsupportedFlags),
        };
        let path = RecordedPath::decode(body)?;
        let anchor = body.u64()?;
        let head = body.u64()?;
        let source = FileDescriptor::decode(body, MAX_TEXT_BYTES)?;
        let selection = RecordedSelection {
            anchor_byte: checked_offset(anchor, source.len_bytes)?,
            head_byte: checked_offset(head, source.len_bytes)?,
        };
        let logical_saved = FileDescriptor::decode_optional(body)?;
        let baseline = FileDescriptor::decode_optional(body)?;
        let resources = decode_resources(body)?;
        Ok(Self {
            id,
            parent_generation,
            path_generation,
            captured_composition,
            path,
            selection,
            source,
            logical_saved,
            baseline,
            resources,
        })
    }

    /// 源码完整并且选区端点均在 UTF-8 边界上时，才可用于创建内存文档。
    pub(crate) fn verify_source<'b>(&self, bytes: &'b [u8]) -> Result<&'b str, RecordError> {
        let text = self.source.verify_utf8(bytes)?;
        if !text.is_char_boundary(self.selection.anchor_byte)
            || !text.is_char_boundary(self.selection.head_byte)
        {
            return Err(RecordError::InvalidSelection);
        }
        Ok(text)
    }

    pub(crate) const fn document_id(&self) -> NonZeroU64 {
        self.id.document
    }

    pub(crate) const fn generation(&self) -> NonZeroU64 {
        self.id.generation
    }

    pub(crate) const fn parent_generation(&self) -> Option<NonZeroU64> {
        self.parent_generation
    }

    pub(crate) const fn path_generation(&self) -> u64 {
        self.path_generation
    }

    pub(crate) const fn captured_composition(&self) -> bool {
        self.captured_composition
    }

    pub(crate) const fn path(&self) -> &RecordedPath<'a> {
        &self.path
    }

    pub(crate) const fn selection(&self) -> RecordedSelection {
        self.selection
    }

    pub(crate) const fn source(&self) -> FileDescriptor {
        self.source
    }

    pub(crate) const fn logical_saved(&self) -> Option<FileDescriptor> {
        self.logical_saved
    }

    pub(crate) const fn baseline(&self) -> Option<FileDescriptor> {
        self.baseline
    }

    pub(crate) fn resources(&self) -> &[ResourceRecord<'a>] {
        &self.resources
    }
}

impl RetiredRecord {
    pub(crate) const fn document_id(&self) -> NonZeroU64 {
        self.id.document
    }

    pub(crate) const fn generation(&self) -> NonZeroU64 {
        self.id.generation
    }

    pub(crate) const fn retired_through_generation(&self) -> u64 {
        self.retired_through_generation
    }
}

impl<'a> RecordedPath<'a> {
    /// 路径种类：0 无路径、1 原始 Unix bytes、2 Windows UTF-16LE。
    pub(crate) const fn kind(&self) -> u8 {
        match self {
            Self::Unlinked => 0,
            Self::Unix(_) => 1,
            Self::WindowsUtf16Le(_) => 2,
        }
    }

    pub(crate) const fn bytes(&self) -> &'a [u8] {
        match self {
            Self::Unlinked => &[],
            Self::Unix(bytes) | Self::WindowsUtf16Le(bytes) => bytes,
        }
    }
}

impl RecordedSelection {
    pub(crate) const fn anchor_byte(self) -> usize {
        self.anchor_byte
    }

    pub(crate) const fn head_byte(self) -> usize {
        self.head_byte
    }
}

impl<'a> ResourceRecord<'a> {
    pub(crate) const fn ordinal(&self) -> NonZeroU64 {
        self.ordinal
    }

    pub(crate) const fn reference(&self) -> &'a str {
        self.reference
    }

    pub(crate) const fn bytes(&self) -> FileDescriptor {
        self.bytes
    }
}

impl<'a> RecordedPath<'a> {
    fn decode(body: &mut Wire<'a>) -> Result<Self, RecordError> {
        let kind = body.byte()?;
        if kind > 2 {
            return Err(RecordError::UnsupportedPath);
        }
        let length = bounded_len(u64::from(body.u32()?), MAX_PATH_BYTES as u64)?;
        let bytes = body.take(length)?;
        match kind {
            0 if bytes.is_empty() => Ok(Self::Unlinked),
            1 => Ok(Self::Unix(bytes)),
            2 if length.is_multiple_of(2) => Ok(Self::WindowsUtf16Le(bytes)),
            _ => Err(RecordError::InvalidPath),
        }
    }
}

impl FileDescriptor {
    /// 描述一份文本正文（source/logical/baseline），上限 10 MiB。
    pub(crate) fn of_text(bytes: &[u8]) -> Result<Self, RecordError> {
        Self::of_limited(bytes, MAX_TEXT_BYTES)
    }

    /// 描述一份资源正文，上限 64 MiB。
    pub(crate) fn of_resource(bytes: &[u8]) -> Result<Self, RecordError> {
        Self::of_limited(bytes, MAX_RESOURCE_BYTES)
    }

    fn of_limited(bytes: &[u8], limit: usize) -> Result<Self, RecordError> {
        if bytes.len() > limit {
            return Err(RecordError::Budget);
        }
        Ok(Self {
            len_bytes: bytes.len(),
            crc32: crc32(bytes),
        })
    }

    #[cfg(test)]
    pub(crate) const fn from_parts(len_bytes: usize, crc32: u32) -> Self {
        Self { len_bytes, crc32 }
    }

    pub(crate) const fn len_bytes(self) -> usize {
        self.len_bytes
    }

    pub(crate) const fn crc32(self) -> u32 {
        self.crc32
    }

    fn decode(body: &mut Wire<'_>, limit: usize) -> Result<Self, RecordError> {
        Ok(Self {
            len_bytes: body.bounded_len(limit as u64)?,
            crc32: body.u32()?,
        })
    }

    fn decode_optional(body: &mut Wire<'_>) -> Result<Option<Self>, RecordError> {
        match body.byte()? {
            0 => Ok(None),
            1 => Self::decode(body, MAX_TEXT_BYTES).map(Some),
            _ => Err(RecordError::InvalidPresence),
        }
    }

    pub(crate) fn verify(&self, bytes: &[u8]) -> Result<(), RecordError> {
        if bytes.len() != self.len_bytes {
            return Err(RecordError::LengthMismatch);
        }
        if crc32(bytes) != self.crc32 {
            return Err(RecordError::ChecksumMismatch);
        }
        Ok(())
    }

    pub(crate) fn verify_utf8<'a>(&self, bytes: &'a [u8]) -> Result<&'a str, RecordError> {
        self.verify(bytes)?;
        std::str::from_utf8(bytes).map_err(|_| RecordError::InvalidText)
    }
}

/// 编码输入中的路径表示；与解码 [`RecordedPath`] 对称。
pub(crate) enum EncodedPath<'a> {
    Unlinked,
    Unix(&'a [u8]),
    WindowsUtf16Le(&'a [u8]),
}

/// 编码输入的一条资源记录；字节正文由同代资源文件承载。
pub(crate) struct ResourceEncoding<'a> {
    pub(crate) ordinal: NonZeroU64,
    pub(crate) reference: &'a str,
    pub(crate) bytes: FileDescriptor,
}

/// 编码一个 Snapshot 代所需的全部字段；不读取文件、不校验磁盘。
pub(crate) struct SnapshotEncoding<'a> {
    pub(crate) document_id: NonZeroU64,
    pub(crate) generation: NonZeroU64,
    pub(crate) parent_generation: Option<NonZeroU64>,
    pub(crate) path_generation: u64,
    pub(crate) captured_composition: bool,
    pub(crate) path: EncodedPath<'a>,
    pub(crate) anchor_byte: u64,
    pub(crate) head_byte: u64,
    pub(crate) source: FileDescriptor,
    pub(crate) logical_saved: Option<FileDescriptor>,
    pub(crate) baseline: Option<FileDescriptor>,
    pub(crate) resources: &'a [ResourceEncoding<'a>],
}

/// 编码一个退役 marker 所需的全部字段。
pub(crate) struct RetiredEncoding {
    pub(crate) document_id: NonZeroU64,
    pub(crate) generation: NonZeroU64,
    pub(crate) retired_through_generation: u64,
}

/// 按 Decision 0 固定顺序编码一个 Snapshot manifest；无 padding，全部小端。
pub(crate) fn encode_snapshot(spec: &SnapshotEncoding<'_>) -> Result<Vec<u8>, RecordError> {
    // 1. 身份与选区。
    if spec
        .parent_generation
        .is_some_and(|parent| parent >= spec.generation)
    {
        return Err(RecordError::InvalidIdentity);
    }
    if spec.source.len_bytes > MAX_TEXT_BYTES
        || spec
            .logical_saved
            .is_some_and(|descriptor| descriptor.len_bytes > MAX_TEXT_BYTES)
        || spec
            .baseline
            .is_some_and(|descriptor| descriptor.len_bytes > MAX_TEXT_BYTES)
    {
        return Err(RecordError::Budget);
    }
    let anchor = usize::try_from(spec.anchor_byte).map_err(|_| RecordError::InvalidSelection)?;
    let head = usize::try_from(spec.head_byte).map_err(|_| RecordError::InvalidSelection)?;
    if anchor > spec.source.len_bytes || head > spec.source.len_bytes {
        return Err(RecordError::InvalidSelection);
    }

    // 2. 路径种类与长度。
    let (path_kind, path_bytes) = match spec.path {
        EncodedPath::Unlinked => (0_u8, &[][..]),
        EncodedPath::Unix(bytes) => (1_u8, bytes),
        EncodedPath::WindowsUtf16Le(bytes) => {
            if !bytes.len().is_multiple_of(2) {
                return Err(RecordError::InvalidPath);
            }
            (2_u8, bytes)
        }
    };
    if path_bytes.len() > MAX_PATH_BYTES {
        return Err(RecordError::Budget);
    }

    // 3. 资源表：数量、ordinal 严格升序、引用非空唯一、单资源与合计预算。
    if spec.resources.len() > MAX_RESOURCE_COUNT {
        return Err(RecordError::Budget);
    }
    let mut references = Vec::<&str>::new();
    references
        .try_reserve_exact(spec.resources.len())
        .map_err(|_| RecordError::Allocation)?;
    let mut total_resources = 0_usize;
    let mut previous_ordinal = None;
    for resource in spec.resources {
        if previous_ordinal.is_some_and(|previous| previous >= resource.ordinal) {
            return Err(RecordError::InvalidResource);
        }
        previous_ordinal = Some(resource.ordinal);
        if resource.reference.is_empty() {
            return Err(RecordError::InvalidResource);
        }
        if resource.reference.len() > MAX_REFERENCE_BYTES {
            return Err(RecordError::Budget);
        }
        if references.contains(&resource.reference) {
            return Err(RecordError::InvalidResource);
        }
        if resource.bytes.len_bytes > MAX_RESOURCE_BYTES {
            return Err(RecordError::Budget);
        }
        total_resources = total_resources
            .checked_add(resource.bytes.len_bytes)
            .ok_or(RecordError::Budget)?;
        references.push(resource.reference);
    }
    if total_resources > MAX_GENERATION_RESOURCE_BYTES {
        return Err(RecordError::Budget);
    }

    // 4. 正文长度必须在 manifest 预算内。
    let mut body_len = 8 + 8 + 8 + 8 + 1 + 1 + 4;
    body_len = checked_len(body_len, path_bytes.len())?;
    body_len = checked_len(body_len, 8 + 8 + 12)?;
    body_len = checked_len(body_len, 1)?;
    if spec.logical_saved.is_some() {
        body_len = checked_len(body_len, 12)?;
    }
    body_len = checked_len(body_len, 1)?;
    if spec.baseline.is_some() {
        body_len = checked_len(body_len, 12)?;
    }
    body_len = checked_len(body_len, 4)?;
    for resource in spec.resources {
        body_len = checked_len(body_len, 8 + 4 + 12)?;
        body_len = checked_len(body_len, resource.reference.len())?;
    }
    if body_len > MAX_MANIFEST_BYTES - HEADER_BYTES {
        return Err(RecordError::Budget);
    }

    // 5. 固定顺序写入正文；容量已预留，不依赖隐式分配。
    let mut body = Vec::new();
    body.try_reserve_exact(body_len)
        .map_err(|_| RecordError::Allocation)?;
    push_u64(&mut body, spec.document_id.get());
    push_u64(&mut body, spec.generation.get());
    push_u64(&mut body, spec.parent_generation.map_or(0, NonZeroU64::get));
    push_u64(&mut body, spec.path_generation);
    push_u8(&mut body, u8::from(spec.captured_composition));
    push_u8(&mut body, path_kind);
    push_u32(
        &mut body,
        u32::try_from(path_bytes.len()).map_err(|_| RecordError::Budget)?,
    );
    push_bytes(&mut body, path_bytes);
    push_u64(&mut body, spec.anchor_byte);
    push_u64(&mut body, spec.head_byte);
    push_descriptor(&mut body, spec.source);
    push_optional_descriptor(&mut body, spec.logical_saved);
    push_optional_descriptor(&mut body, spec.baseline);
    push_u32(
        &mut body,
        u32::try_from(spec.resources.len()).map_err(|_| RecordError::Budget)?,
    );
    for resource in spec.resources {
        push_u64(&mut body, resource.ordinal.get());
        push_u32(
            &mut body,
            u32::try_from(resource.reference.len()).map_err(|_| RecordError::Budget)?,
        );
        push_bytes(&mut body, resource.reference.as_bytes());
        push_descriptor(&mut body, resource.bytes);
    }
    encode_manifest(0, &body)
}

/// 编码退役 marker；正文严格 24 bytes，且 `retired_through + 1 == generation`。
pub(crate) fn encode_retired(spec: RetiredEncoding) -> Result<Vec<u8>, RecordError> {
    if spec.retired_through_generation.checked_add(1) != Some(spec.generation.get()) {
        return Err(RecordError::InvalidIdentity);
    }
    let mut body = Vec::new();
    body.try_reserve_exact(24)
        .map_err(|_| RecordError::Allocation)?;
    push_u64(&mut body, spec.document_id.get());
    push_u64(&mut body, spec.generation.get());
    push_u64(&mut body, spec.retired_through_generation);
    encode_manifest(1, &body)
}

fn encode_manifest(kind: u8, body: &[u8]) -> Result<Vec<u8>, RecordError> {
    if body.len() > MAX_MANIFEST_BYTES - HEADER_BYTES {
        return Err(RecordError::Budget);
    }
    let body_len = u64::try_from(body.len()).map_err(|_| RecordError::Budget)?;
    let total = HEADER_BYTES
        .checked_add(body.len())
        .ok_or(RecordError::Budget)?;
    let mut manifest = Vec::new();
    manifest
        .try_reserve_exact(total)
        .map_err(|_| RecordError::Allocation)?;
    push_bytes(&mut manifest, MANIFEST_MAGIC);
    push_u16(&mut manifest, 1);
    push_u8(&mut manifest, kind);
    push_u8(&mut manifest, 0);
    push_u64(&mut manifest, body_len);
    push_u32(&mut manifest, crc32(body));
    push_bytes(&mut manifest, body);
    Ok(manifest)
}

fn checked_len(current: usize, add: usize) -> Result<usize, RecordError> {
    current.checked_add(add).ok_or(RecordError::Budget)
}

fn push_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
}

fn push_descriptor(out: &mut Vec<u8>, descriptor: FileDescriptor) {
    push_u64(out, descriptor.len_bytes as u64);
    push_u32(out, descriptor.crc32);
}

fn push_optional_descriptor(out: &mut Vec<u8>, descriptor: Option<FileDescriptor>) {
    match descriptor {
        None => push_u8(out, 0),
        Some(descriptor) => {
            push_u8(out, 1);
            push_descriptor(out, descriptor);
        }
    }
}

fn decode_resources<'a>(body: &mut Wire<'a>) -> Result<Vec<ResourceRecord<'a>>, RecordError> {
    let count = bounded_len(u64::from(body.u32()?), MAX_RESOURCE_COUNT as u64)?;
    let mut resources = Vec::<ResourceRecord<'a>>::new();
    resources
        .try_reserve_exact(count)
        .map_err(|_| RecordError::Allocation)?;
    let mut total: usize = 0;
    for _ in 0..count {
        let ordinal = body.nonzero()?;
        if resources.last().is_some_and(|last| last.ordinal >= ordinal) {
            return Err(RecordError::InvalidResource);
        }
        let length = bounded_len(u64::from(body.u32()?), MAX_REFERENCE_BYTES as u64)?;
        let reference =
            std::str::from_utf8(body.take(length)?).map_err(|_| RecordError::InvalidResource)?;
        if reference.is_empty() || resources.iter().any(|entry| entry.reference == reference) {
            return Err(RecordError::InvalidResource);
        }
        let bytes = FileDescriptor::decode(body, MAX_RESOURCE_BYTES)?;
        total = total
            .checked_add(bytes.len_bytes)
            .ok_or(RecordError::Budget)?;
        if total > MAX_GENERATION_RESOURCE_BYTES {
            return Err(RecordError::Budget);
        }
        resources.push(ResourceRecord {
            ordinal,
            reference,
            bytes,
        });
    }
    Ok(resources)
}

struct Wire<'a> {
    remaining: &'a [u8],
}

impl<'a> Wire<'a> {
    fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], RecordError> {
        let (bytes, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(RecordError::Truncated)?;
        self.remaining = remaining;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], RecordError> {
        self.take(N)?.try_into().map_err(|_| RecordError::Truncated)
    }

    fn byte(&mut self) -> Result<u8, RecordError> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    fn u16(&mut self) -> Result<u16, RecordError> {
        self.array().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, RecordError> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, RecordError> {
        self.array().map(u64::from_le_bytes)
    }

    fn nonzero(&mut self) -> Result<NonZeroU64, RecordError> {
        NonZeroU64::new(self.u64()?).ok_or(RecordError::InvalidIdentity)
    }

    fn bounded_len(&mut self, limit: u64) -> Result<usize, RecordError> {
        bounded_len(self.u64()?, limit)
    }

    fn finish(self) -> Result<(), RecordError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(RecordError::TrailingBytes)
        }
    }
}

fn bounded_len(length: u64, limit: u64) -> Result<usize, RecordError> {
    if length > limit {
        return Err(RecordError::Budget);
    }
    usize::try_from(length).map_err(|_| RecordError::Budget)
}

fn checked_offset(offset: u64, source_len: usize) -> Result<usize, RecordError> {
    let byte_offset = usize::try_from(offset).map_err(|_| RecordError::InvalidSelection)?;
    if byte_offset > source_len {
        return Err(RecordError::InvalidSelection);
    }
    Ok(byte_offset)
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMED: &str = include_str!("../../../tests/fixtures/recovery/snapshot-v1.manifest.hex");
    const UNLINKED: &str =
        include_str!("../../../tests/fixtures/recovery/unlinked-snapshot-v1.manifest.hex");
    const RETIRED: &str = "414c494e45524543 0100010018000000 00000000881c0949 \
                           0100000000000000 0300000000000000 0200000000000000";
    // Windows 原始路径单元 0x0043、0xd800；源码有 BOM、多字节字符及混合行尾。
    const WINDOWS: &str = "414c494e45524543 0100000069000000 000000003168aeed \
                          0300000000000000 0700000000000000 0600000000000000 \
                          0400000000000000 0102040000004300 00d8060000000000 \
                          0000000000000000 00000d0000000000 0000965d62f60000 \
                          0100000003000000 0000000005000000 696d672f61040000 \
                          000000000063f3f3 ad";

    #[test]
    fn decode_named_snapshot_preserves_direction_path_and_baselines() -> Result<(), RecordError> {
        let bytes = hex(NAMED)?;
        let snapshot = snapshot(&bytes)?;
        assert_eq!(snapshot.id.document.get(), 1);
        assert_eq!(snapshot.id.generation.get(), 2);
        assert_eq!(snapshot.parent_generation, NonZeroU64::new(1));
        assert_eq!(snapshot.path_generation, 2);
        assert!(!snapshot.captured_composition);
        assert_eq!(snapshot.path, RecordedPath::Unix(b"notes.md"));
        assert_eq!(
            snapshot.selection,
            RecordedSelection {
                anchor_byte: 3,
                head_byte: 0,
            }
        );
        assert_eq!(snapshot.source.crc32, 0x3524_41c2);
        assert_eq!(
            snapshot.verify_source(include_bytes!(
                "../../../tests/fixtures/recovery/snapshot-v1.source.utf8"
            ))?,
            "abc"
        );
        let logical_saved = snapshot.logical_saved.ok_or(RecordError::InvalidPresence)?;
        assert_eq!(logical_saved.len_bytes, 3);
        assert_eq!(
            logical_saved.verify_utf8(include_bytes!(
                "../../../tests/fixtures/recovery/snapshot-v1.logical-saved.utf8"
            ))?,
            "a\nb"
        );
        let baseline = snapshot.baseline.ok_or(RecordError::InvalidPresence)?;
        assert_eq!(baseline.len_bytes, 4);
        baseline.verify(include_bytes!(
            "../../../tests/fixtures/recovery/snapshot-v1.baseline.bytes"
        ))?;
        assert_eq!(baseline.verify_utf8(b"a\r\nb")?, "a\r\nb");
        assert!(snapshot.resources.is_empty());
        Ok(())
    }

    #[test]
    fn decode_unlinked_snapshot_distinguishes_empty_files_from_absence() -> Result<(), RecordError>
    {
        let bytes = hex(UNLINKED)?;
        let snapshot = snapshot(&bytes)?;
        assert_eq!(snapshot.id.document.get(), 2);
        assert_eq!(snapshot.id.generation.get(), 1);
        assert_eq!(snapshot.parent_generation, None);
        assert_eq!(snapshot.path_generation, 0);
        assert!(!snapshot.captured_composition);
        assert_eq!(snapshot.path, RecordedPath::Unlinked);
        assert_eq!(
            snapshot.selection,
            RecordedSelection {
                anchor_byte: 0,
                head_byte: 0,
            }
        );
        assert_eq!(
            snapshot.verify_source(include_bytes!(
                "../../../tests/fixtures/recovery/unlinked-snapshot-v1.source.utf8"
            ))?,
            "abc"
        );
        snapshot
            .logical_saved
            .ok_or(RecordError::InvalidPresence)?
            .verify(include_bytes!(
                "../../../tests/fixtures/recovery/unlinked-snapshot-v1.logical-saved.utf8"
            ))?;
        snapshot
            .baseline
            .ok_or(RecordError::InvalidPresence)?
            .verify(include_bytes!(
                "../../../tests/fixtures/recovery/unlinked-snapshot-v1.baseline.bytes"
            ))?;
        assert!(snapshot.resources.is_empty());
        Ok(())
    }

    #[test]
    fn decode_snapshot_checks_raw_windows_path_and_resource_bytes() -> Result<(), RecordError> {
        let bytes = hex(WINDOWS)?;
        let snapshot = snapshot(&bytes)?;
        assert_eq!(
            snapshot.path,
            RecordedPath::WindowsUtf16Le(&[0x43, 0x00, 0x00, 0xd8])
        );
        assert!(snapshot.captured_composition);
        assert_eq!(
            snapshot.verify_source("\u{feff}中\r\n😀\n".as_bytes())?,
            "\u{feff}中\r\n😀\n"
        );
        assert!(snapshot.logical_saved.is_none());
        assert!(snapshot.baseline.is_none());
        let [resource] = snapshot.resources.as_slice() else {
            return Err(RecordError::InvalidResource);
        };
        assert_eq!(resource.ordinal.get(), 3);
        assert_eq!(resource.reference, "img/a");
        resource.bytes.verify(b"data")?;
        assert_eq!(
            resource.bytes.verify(b"bad"),
            Err(RecordError::LengthMismatch)
        );
        assert_eq!(
            resource.bytes.verify(b"DATA"),
            Err(RecordError::ChecksumMismatch)
        );
        Ok(())
    }

    #[test]
    fn decode_retired_marker_requires_a_newer_generation() -> Result<(), RecordError> {
        let bytes = hex(RETIRED)?;
        let RecoveryRecord::Retired(retired) = RecoveryRecord::decode(&bytes)? else {
            return Err(RecordError::UnsupportedKind);
        };
        assert_eq!(retired.id.document.get(), 1);
        assert_eq!(retired.id.generation.get(), 3);
        assert_eq!(retired.retired_through_generation, 2);
        let mut invalid_gap = hex(RETIRED)?;
        invalid_gap[40..48].copy_from_slice(&1_u64.to_le_bytes());
        let checksum = crc32(&invalid_gap[HEADER_BYTES..]);
        invalid_gap[20..24].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            RecoveryRecord::decode(&invalid_gap).err(),
            Some(RecordError::InvalidIdentity)
        );
        let invalid = hex("414c494e45524543 0100010018000000 00000000161ca385 \
                           0100000000000000 0300000000000000 0300000000000000")?;
        assert_eq!(
            RecoveryRecord::decode(&invalid).err(),
            Some(RecordError::InvalidIdentity)
        );
        Ok(())
    }

    #[test]
    fn decode_rejects_static_corrupt_oversized_and_unsupported_manifests() -> Result<(), RecordError>
    {
        let cases = [
            (
                include_str!("../../../tests/fixtures/recovery/truncated.manifest.hex"),
                RecordError::Truncated,
            ),
            (
                include_str!("../../../tests/fixtures/recovery/bad-crc.manifest.hex"),
                RecordError::ChecksumMismatch,
            ),
            (
                include_str!("../../../tests/fixtures/recovery/oversized-source.manifest.hex"),
                RecordError::Budget,
            ),
            (
                include_str!("../../../tests/fixtures/recovery/unknown-version.manifest.hex"),
                RecordError::UnsupportedVersion,
            ),
        ];
        for (fixture, error) in cases {
            assert_eq!(RecoveryRecord::decode(&hex(fixture)?).err(), Some(error));
        }
        let mut bytes = hex(NAMED)?;
        bytes.push(0);
        assert_eq!(
            RecoveryRecord::decode(&bytes).err(),
            Some(RecordError::TrailingBytes)
        );
        bytes[10] = 2;
        assert_eq!(
            RecoveryRecord::decode(&bytes).err(),
            Some(RecordError::UnsupportedKind)
        );
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        Ok(())
    }

    #[test]
    fn verify_source_rejects_half_scalar_with_matching_checksum() -> Result<(), RecordError> {
        let bytes = hex("414c494e45524543 0100000069000000 0000000055d77b1c \
                         0300000000000000 0700000000000000 0600000000000000 \
                         0400000000000000 0102040000004300 00d8050000000000 \
                         0000000000000000 00000d0000000000 0000965d62f60000 \
                         0100000003000000 0000000005000000 696d672f61040000 \
                         000000000063f3f3 ad")?;
        let snapshot = snapshot(&bytes)?;
        assert_eq!(
            snapshot.verify_source("\u{feff}中\r\n😀\n".as_bytes()),
            Err(RecordError::InvalidSelection)
        );
        Ok(())
    }

    #[test]
    fn decode_rejects_unknown_path_and_duplicate_resource_keys() -> Result<(), RecordError> {
        let unknown = hex("414c494e45524543 0100000069000000 00000000ab15cfb4 \
                           0300000000000000 0700000000000000 0600000000000000 \
                           0400000000000000 0109040000004300 00d8060000000000 \
                           0000000000000000 00000d0000000000 0000965d62f60000 \
                           0100000003000000 0000000005000000 696d672f61040000 \
                           000000000063f3f3 ad")?;
        let duplicate = hex("414c494e45524543 0100000086000000 00000000f36f9ed8 \
                             0300000000000000 0700000000000000 0600000000000000 \
                             0400000000000000 0102040000004300 00d8060000000000 \
                             0000000000000000 00000d0000000000 0000965d62f60000 \
                             0200000003000000 0000000005000000 696d672f61040000 \
                             000000000063f3f3 ad04000000000000 0005000000696d67 \
                             2f61040000000000 000000000000")?;
        assert_eq!(
            RecoveryRecord::decode(&unknown).err(),
            Some(RecordError::UnsupportedPath)
        );
        assert_eq!(
            RecoveryRecord::decode(&duplicate).err(),
            Some(RecordError::InvalidResource)
        );
        Ok(())
    }

    #[test]
    fn decode_requires_complete_framing_and_exact_body() -> Result<(), RecordError> {
        let named = hex(NAMED)?;
        for length in 0..named.len() {
            assert_eq!(
                RecoveryRecord::decode(&named[..length]).err(),
                Some(RecordError::Truncated)
            );
        }
        let body_with_extra_byte = hex("414c494e45524543 010000006a000000 000000007371e8a5 \
                 0300000000000000 0700000000000000 0600000000000000 \
                 0400000000000000 0102040000004300 00d8060000000000 \
                 0000000000000000 00000d0000000000 0000965d62f60000 \
                 0100000003000000 0000000005000000 696d672f61040000 \
                 000000000063f3f3 ad30")?;
        assert_eq!(
            RecoveryRecord::decode(&body_with_extra_byte).err(),
            Some(RecordError::TrailingBytes)
        );
        let oversized_header = hex("414c494e45524543 01000000ffffffff ffffffff00000000")?;
        assert_eq!(
            RecoveryRecord::decode(&oversized_header).err(),
            Some(RecordError::Budget)
        );
        Ok(())
    }

    #[test]
    fn golden_snapshot_manifest_encodes_exactly() -> Result<(), RecordError> {
        let resources: &[ResourceEncoding<'_>] = &[];
        let spec = SnapshotEncoding {
            document_id: nonzero(1)?,
            generation: nonzero(2)?,
            parent_generation: Some(nonzero(1)?),
            path_generation: 2,
            captured_composition: false,
            path: EncodedPath::Unix(b"notes.md"),
            anchor_byte: 3,
            head_byte: 0,
            source: FileDescriptor::of_text(b"abc")?,
            logical_saved: Some(FileDescriptor::of_text(b"a\nb")?),
            baseline: Some(FileDescriptor::of_text(b"a\r\nb")?),
            resources,
        };
        assert_eq!(encode_snapshot(&spec)?, hex(NAMED)?);
        Ok(())
    }

    #[test]
    fn golden_unlinked_manifest_encodes_exactly() -> Result<(), RecordError> {
        let resources: &[ResourceEncoding<'_>] = &[];
        let spec = SnapshotEncoding {
            document_id: nonzero(2)?,
            generation: nonzero(1)?,
            parent_generation: None,
            path_generation: 0,
            captured_composition: false,
            path: EncodedPath::Unlinked,
            anchor_byte: 0,
            head_byte: 0,
            source: FileDescriptor::of_text(b"abc")?,
            logical_saved: Some(FileDescriptor::of_text(b"")?),
            baseline: Some(FileDescriptor::of_text(b"")?),
            resources,
        };
        assert_eq!(encode_snapshot(&spec)?, hex(UNLINKED)?);
        Ok(())
    }

    #[test]
    fn golden_retired_manifest_encodes_exactly() -> Result<(), RecordError> {
        let spec = RetiredEncoding {
            document_id: nonzero(1)?,
            generation: nonzero(3)?,
            retired_through_generation: 2,
        };
        assert_eq!(encode_retired(spec)?, hex(RETIRED)?);
        Ok(())
    }

    #[test]
    fn encoder_rejects_parent_generation_not_older() -> Result<(), RecordError> {
        let resources: &[ResourceEncoding<'_>] = &[];
        let mut spec = minimal_spec(resources, FileDescriptor::of_text(b"abc")?)?;
        spec.parent_generation = Some(nonzero(2)?);
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidIdentity)
        );
        spec.parent_generation = Some(nonzero(3)?);
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidIdentity)
        );
        Ok(())
    }

    #[test]
    fn encoder_rejects_duplicate_and_descending_resource_ordinals() -> Result<(), RecordError> {
        let descending = [
            ResourceEncoding {
                ordinal: nonzero(2)?,
                reference: "a",
                bytes: FileDescriptor::of_resource(b"x")?,
            },
            ResourceEncoding {
                ordinal: nonzero(1)?,
                reference: "b",
                bytes: FileDescriptor::of_resource(b"y")?,
            },
        ];
        let spec = minimal_spec(&descending, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidResource)
        );
        let duplicate = [
            ResourceEncoding {
                ordinal: nonzero(1)?,
                reference: "a",
                bytes: FileDescriptor::of_resource(b"x")?,
            },
            ResourceEncoding {
                ordinal: nonzero(1)?,
                reference: "b",
                bytes: FileDescriptor::of_resource(b"y")?,
            },
        ];
        let spec = minimal_spec(&duplicate, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidResource)
        );
        Ok(())
    }

    #[test]
    fn encoder_rejects_duplicate_empty_and_oversized_resource_references() -> Result<(), RecordError>
    {
        let empty = [ResourceEncoding {
            ordinal: nonzero(1)?,
            reference: "",
            bytes: FileDescriptor::of_resource(b"x")?,
        }];
        let spec = minimal_spec(&empty, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidResource)
        );
        let repeated = [
            ResourceEncoding {
                ordinal: nonzero(1)?,
                reference: "img/a",
                bytes: FileDescriptor::of_resource(b"x")?,
            },
            ResourceEncoding {
                ordinal: nonzero(2)?,
                reference: "img/a",
                bytes: FileDescriptor::of_resource(b"y")?,
            },
        ];
        let spec = minimal_spec(&repeated, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidResource)
        );
        let oversized_reference = "r".repeat(MAX_REFERENCE_BYTES + 1);
        let oversized = [ResourceEncoding {
            ordinal: nonzero(1)?,
            reference: &oversized_reference,
            bytes: FileDescriptor::of_resource(b"x")?,
        }];
        let spec = minimal_spec(&oversized, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));
        Ok(())
    }

    #[test]
    fn encoder_rejects_path_selection_and_text_budgets() -> Result<(), RecordError> {
        let resources: &[ResourceEncoding<'_>] = &[];
        let oversized_path = vec![b'p'; MAX_PATH_BYTES + 1];
        let mut spec = minimal_spec(resources, FileDescriptor::of_text(b"abc")?)?;
        spec.path = EncodedPath::Unix(&oversized_path);
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));

        let odd_windows = [0x41_u8, 0x00, 0x00];
        let mut spec = minimal_spec(resources, FileDescriptor::of_text(b"abc")?)?;
        spec.path = EncodedPath::WindowsUtf16Le(&odd_windows);
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::InvalidPath));

        let mut spec = minimal_spec(resources, FileDescriptor::of_text(b"abc")?)?;
        spec.anchor_byte = 4;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidSelection)
        );

        let mut spec = minimal_spec(resources, FileDescriptor::of_text(b"abc")?)?;
        spec.head_byte = u64::MAX;
        assert_eq!(
            encode_snapshot(&spec).err(),
            Some(RecordError::InvalidSelection)
        );

        let mut spec = minimal_spec(resources, FileDescriptor::from_parts(MAX_TEXT_BYTES + 1, 0))?;
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));
        spec.source = FileDescriptor::of_text(b"abc")?;
        spec.logical_saved = Some(FileDescriptor::from_parts(MAX_TEXT_BYTES + 1, 0));
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));
        spec.logical_saved = None;
        spec.baseline = Some(FileDescriptor::from_parts(MAX_TEXT_BYTES + 1, 0));
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));
        Ok(())
    }

    #[test]
    fn encoder_rejects_generation_resource_total_over_256_mib() -> Result<(), RecordError> {
        let resources = [
            ResourceEncoding {
                ordinal: nonzero(1)?,
                reference: "a",
                bytes: FileDescriptor::from_parts(MAX_RESOURCE_BYTES, 0),
            },
            ResourceEncoding {
                ordinal: nonzero(2)?,
                reference: "b",
                bytes: FileDescriptor::from_parts(MAX_RESOURCE_BYTES, 0),
            },
            ResourceEncoding {
                ordinal: nonzero(3)?,
                reference: "c",
                bytes: FileDescriptor::from_parts(MAX_RESOURCE_BYTES, 0),
            },
            ResourceEncoding {
                ordinal: nonzero(4)?,
                reference: "d",
                bytes: FileDescriptor::from_parts(MAX_RESOURCE_BYTES, 0),
            },
            ResourceEncoding {
                ordinal: nonzero(5)?,
                reference: "e",
                bytes: FileDescriptor::from_parts(1, 0),
            },
        ];
        let spec = minimal_spec(&resources, FileDescriptor::of_text(b"abc")?)?;
        assert_eq!(encode_snapshot(&spec).err(), Some(RecordError::Budget));
        Ok(())
    }

    #[test]
    fn crc32_matches_iso_hdlc_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    fn nonzero(value: u64) -> Result<NonZeroU64, RecordError> {
        NonZeroU64::new(value).ok_or(RecordError::InvalidIdentity)
    }

    fn minimal_spec<'a>(
        resources: &'a [ResourceEncoding<'a>],
        source: FileDescriptor,
    ) -> Result<SnapshotEncoding<'a>, RecordError> {
        Ok(SnapshotEncoding {
            document_id: nonzero(1)?,
            generation: nonzero(2)?,
            parent_generation: Some(nonzero(1)?),
            path_generation: 0,
            captured_composition: false,
            path: EncodedPath::Unix(b"notes.md"),
            anchor_byte: 0,
            head_byte: 0,
            source,
            logical_saved: None,
            baseline: None,
            resources,
        })
    }

    fn snapshot(bytes: &[u8]) -> Result<SnapshotRecord<'_>, RecordError> {
        match RecoveryRecord::decode(bytes)? {
            RecoveryRecord::Snapshot(snapshot) => Ok(snapshot),
            RecoveryRecord::Retired(_) => Err(RecordError::UnsupportedKind),
        }
    }

    fn hex(fixture: &str) -> Result<Vec<u8>, RecordError> {
        let hex: Vec<_> = fixture
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        let (pairs, remainder) = hex.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(RecordError::Truncated);
        }
        pairs
            .iter()
            .map(|&[high, low]| Ok((hex_digit(high)? << 4) | hex_digit(low)?))
            .collect()
    }

    fn hex_digit(byte: u8) -> Result<u8, RecordError> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(RecordError::InvalidText),
        }
    }
}
