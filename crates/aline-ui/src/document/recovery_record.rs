//! W4 恢复清单 V1 的纯字节解码与文件内容核验，供恢复协调器使用。
//! 输入为已限长读取的清单字节及同代文件字节。借用原始路径和资源键，不持有文件、
//! 历史、路径授权或发布事实。目录身份、磁盘同步与恢复采纳由协调器负责。

use std::num::NonZeroU64;

const MIB: usize = 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = MIB;
const HEADER_BYTES: usize = 24;
const MAX_TEXT_BYTES: usize = 10 * MIB;
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_RESOURCE_COUNT: usize = 256;
const MAX_REFERENCE_BYTES: usize = 4096;
const MAX_RESOURCE_BYTES: usize = 64 * MIB;
const MAX_GENERATION_RESOURCE_BYTES: usize = 256 * MIB;
const MANIFEST_MAGIC: &[u8; 8] = b"ALINEREC";

/// 保留未知编码与损坏的区别，供协调器决定可否操作谱系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecordError {
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

pub(super) enum RecoveryRecord<'a> {
    Snapshot(SnapshotRecord<'a>),
    Retired(RetiredRecord),
}

#[derive(Debug, Eq, PartialEq)]
struct RecordId {
    document: NonZeroU64,
    generation: NonZeroU64,
}

pub(super) struct SnapshotRecord<'a> {
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

pub(super) struct RetiredRecord {
    id: RecordId,
    retired_through_generation: u64,
}

#[derive(Debug, Eq, PartialEq)]
enum RecordedPath<'a> {
    Unlinked,
    Unix(&'a [u8]),
    WindowsUtf16Le(&'a [u8]),
}

#[derive(Debug, Eq, PartialEq)]
struct RecordedSelection {
    anchor_byte: usize,
    head_byte: usize,
}

#[derive(Debug, Eq, PartialEq)]
struct FileDescriptor {
    len_bytes: usize,
    crc32: u32,
}

struct ResourceRecord<'a> {
    ordinal: NonZeroU64,
    reference: &'a str,
    bytes: FileDescriptor,
}

impl<'a> RecoveryRecord<'a> {
    pub(super) fn decode(bytes: &'a [u8]) -> Result<Self, RecordError> {
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
    fn verify_source<'b>(&self, bytes: &'b [u8]) -> Result<&'b str, RecordError> {
        let text = self.source.verify_utf8(bytes)?;
        if !text.is_char_boundary(self.selection.anchor_byte)
            || !text.is_char_boundary(self.selection.head_byte)
        {
            return Err(RecordError::InvalidSelection);
        }
        Ok(text)
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

    fn verify(&self, bytes: &[u8]) -> Result<(), RecordError> {
        if bytes.len() != self.len_bytes {
            return Err(RecordError::LengthMismatch);
        }
        if crc32(bytes) != self.crc32 {
            return Err(RecordError::ChecksumMismatch);
        }
        Ok(())
    }

    fn verify_utf8<'a>(&self, bytes: &'a [u8]) -> Result<&'a str, RecordError> {
        self.verify(bytes)?;
        std::str::from_utf8(bytes).map_err(|_| RecordError::InvalidText)
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

fn crc32(bytes: &[u8]) -> u32 {
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

    const NAMED: &str = include_str!("../../tests/fixtures/recovery/snapshot-v1.manifest.hex");
    const UNLINKED: &str =
        include_str!("../../tests/fixtures/recovery/unlinked-snapshot-v1.manifest.hex");
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
                "../../tests/fixtures/recovery/snapshot-v1.source.utf8"
            ))?,
            "abc"
        );
        let logical_saved = snapshot.logical_saved.ok_or(RecordError::InvalidPresence)?;
        assert_eq!(logical_saved.len_bytes, 3);
        assert_eq!(
            logical_saved.verify_utf8(include_bytes!(
                "../../tests/fixtures/recovery/snapshot-v1.logical-saved.utf8"
            ))?,
            "a\nb"
        );
        let baseline = snapshot.baseline.ok_or(RecordError::InvalidPresence)?;
        assert_eq!(baseline.len_bytes, 4);
        baseline.verify(include_bytes!(
            "../../tests/fixtures/recovery/snapshot-v1.baseline.bytes"
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
                "../../tests/fixtures/recovery/unlinked-snapshot-v1.source.utf8"
            ))?,
            "abc"
        );
        snapshot
            .logical_saved
            .ok_or(RecordError::InvalidPresence)?
            .verify(include_bytes!(
                "../../tests/fixtures/recovery/unlinked-snapshot-v1.logical-saved.utf8"
            ))?;
        snapshot
            .baseline
            .ok_or(RecordError::InvalidPresence)?
            .verify(include_bytes!(
                "../../tests/fixtures/recovery/unlinked-snapshot-v1.baseline.bytes"
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
                include_str!("../../tests/fixtures/recovery/truncated.manifest.hex"),
                RecordError::Truncated,
            ),
            (
                include_str!("../../tests/fixtures/recovery/bad-crc.manifest.hex"),
                RecordError::ChecksumMismatch,
            ),
            (
                include_str!("../../tests/fixtures/recovery/oversized-source.manifest.hex"),
                RecordError::Budget,
            ),
            (
                include_str!("../../tests/fixtures/recovery/unknown-version.manifest.hex"),
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
