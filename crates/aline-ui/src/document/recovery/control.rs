//! 恢复谱系控制记录：V1 `next-id` high-water 与容量 reservation 的纯字节
//! 编解码。两条记录都带固定 magic、version/flags 与 CRC-32/ISO-HDLC；所有
//! 整数小端。本模块不读文件、不加锁，身份与同步由存储层负责。

// 4.1a 的控制记录只被存储层使用；生产接线（4.1b/4.2）到来前，编码器与
// 部分访问器仅由 crate 内测试使用。该豁免只覆盖本模块。
#![cfg_attr(not(test), allow(dead_code))]

use std::num::NonZeroU64;

use super::record::crc32;

const NEXT_ID_MAGIC: &[u8; 8] = b"ALINESEQ";
const RESERVATION_MAGIC: &[u8; 8] = b"ALINEQUO";

/// 谱系内 generation 与 resource ordinal 的持久 high-water；两数只增不减。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NextId {
    pub(crate) next_generation: NonZeroU64,
    pub(crate) next_resource_ordinal: NonZeroU64,
}

impl NextId {
    pub(crate) const fn initial() -> Self {
        Self {
            next_generation: NonZeroU64::MIN,
            next_resource_ordinal: NonZeroU64::MIN,
        }
    }

    /// 解码严格的 32-byte 记录；长度、magic、版本、flags、CRC 与零值分别拒绝。
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        if bytes.len() < 32 {
            return Err(ControlError::Truncated);
        }
        if bytes.len() > 32 {
            return Err(ControlError::TrailingBytes);
        }
        if &bytes[0..8] != NEXT_ID_MAGIC {
            return Err(ControlError::InvalidMagic);
        }
        if u16::from_le_bytes([bytes[8], bytes[9]]) != 1 {
            return Err(ControlError::UnsupportedVersion);
        }
        if u16::from_le_bytes([bytes[10], bytes[11]]) != 0 {
            return Err(ControlError::UnsupportedFlags);
        }
        let checksum = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
        if crc32(&bytes[..28]) != checksum {
            return Err(ControlError::ChecksumMismatch);
        }
        let next_generation = NonZeroU64::new(u64::from_le_bytes(array(&bytes[12..20])))
            .ok_or(ControlError::InvalidIdentity)?;
        let next_resource_ordinal = NonZeroU64::new(u64::from_le_bytes(array(&bytes[20..28])))
            .ok_or(ControlError::InvalidIdentity)?;
        Ok(Self {
            next_generation,
            next_resource_ordinal,
        })
    }

    pub(crate) fn encode(self) -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        bytes[0..8].copy_from_slice(NEXT_ID_MAGIC);
        bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&0_u16.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.next_generation.get().to_le_bytes());
        bytes[20..28].copy_from_slice(&self.next_resource_ordinal.get().to_le_bytes());
        let checksum = crc32(&bytes[..28]);
        bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    /// 把 generation high-water 推进到 `max(current, used + 1)`；溢出拒绝。
    pub(crate) fn advance_generation(&mut self, used: NonZeroU64) -> Result<(), ControlError> {
        let next = used.get().checked_add(1).ok_or(ControlError::Overflow)?;
        let next = NonZeroU64::new(next).ok_or(ControlError::Overflow)?;
        if next > self.next_generation {
            self.next_generation = next;
        }
        Ok(())
    }

    /// 把 resource ordinal high-water 推进 `used_count`；溢出拒绝且不回绕。
    pub(crate) fn advance_resource_ordinal(
        &mut self,
        used_count: usize,
    ) -> Result<(), ControlError> {
        let used = u64::try_from(used_count).map_err(|_| ControlError::Overflow)?;
        let next = self
            .next_resource_ordinal
            .get()
            .checked_add(used)
            .ok_or(ControlError::Overflow)?;
        self.next_resource_ordinal = NonZeroU64::new(next).ok_or(ControlError::Overflow)?;
        Ok(())
    }
}

/// 一次未结算写入的容量上界；文档与 generation 必须非零。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Reservation {
    pub(crate) document: NonZeroU64,
    pub(crate) generation: NonZeroU64,
    pub(crate) reserved_bytes: u64,
}

impl Reservation {
    /// 解码严格的 40-byte 记录。
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        if bytes.len() < 40 {
            return Err(ControlError::Truncated);
        }
        if bytes.len() > 40 {
            return Err(ControlError::TrailingBytes);
        }
        if &bytes[0..8] != RESERVATION_MAGIC {
            return Err(ControlError::InvalidMagic);
        }
        if u16::from_le_bytes([bytes[8], bytes[9]]) != 1 {
            return Err(ControlError::UnsupportedVersion);
        }
        if u16::from_le_bytes([bytes[10], bytes[11]]) != 0 {
            return Err(ControlError::UnsupportedFlags);
        }
        let checksum = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]);
        if crc32(&bytes[..36]) != checksum {
            return Err(ControlError::ChecksumMismatch);
        }
        let document = NonZeroU64::new(u64::from_le_bytes(array(&bytes[12..20])))
            .ok_or(ControlError::InvalidIdentity)?;
        let generation = NonZeroU64::new(u64::from_le_bytes(array(&bytes[20..28])))
            .ok_or(ControlError::InvalidIdentity)?;
        let reserved_bytes = u64::from_le_bytes(array(&bytes[28..36]));
        Ok(Self {
            document,
            generation,
            reserved_bytes,
        })
    }

    pub(crate) fn encode(self) -> [u8; 40] {
        let mut bytes = [0_u8; 40];
        bytes[0..8].copy_from_slice(RESERVATION_MAGIC);
        bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
        bytes[10..12].copy_from_slice(&0_u16.to_le_bytes());
        bytes[12..20].copy_from_slice(&self.document.get().to_le_bytes());
        bytes[20..28].copy_from_slice(&self.generation.get().to_le_bytes());
        bytes[28..36].copy_from_slice(&self.reserved_bytes.to_le_bytes());
        let checksum = crc32(&bytes[..36]);
        bytes[36..40].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }
}

/// 控制记录解码失败；调用方必须保留原 bytes 并禁写禁清该谱系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlError {
    InvalidMagic,
    UnsupportedVersion,
    UnsupportedFlags,
    Truncated,
    TrailingBytes,
    ChecksumMismatch,
    InvalidIdentity,
    Overflow,
}

fn array(bytes: &[u8]) -> [u8; 8] {
    // 调用点都传入固定长度的切片；`try_into` 失败时返回全零，随后由非零
    // 身份检查拒绝，不产生未检查的索引。
    let mut value = [0_u8; 8];
    if let Ok(source) = <[u8; 8]>::try_from(bytes) {
        value = source;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_next_id_round_trip_and_monotonic_advance() -> Result<(), ControlError> {
        let initial = NextId::initial();
        assert_eq!(initial.next_generation.get(), 1);
        assert_eq!(initial.next_resource_ordinal.get(), 1);
        let bytes = initial.encode();
        assert_eq!(NextId::decode(&bytes)?, initial);

        let mut next = initial;
        next.advance_generation(NonZeroU64::new(4).ok_or(ControlError::InvalidIdentity)?)?;
        assert_eq!(next.next_generation.get(), 5);
        next.advance_generation(NonZeroU64::new(2).ok_or(ControlError::InvalidIdentity)?)?;
        assert_eq!(next.next_generation.get(), 5);
        next.advance_resource_ordinal(3)?;
        assert_eq!(next.next_resource_ordinal.get(), 4);
        next.advance_resource_ordinal(0)?;
        assert_eq!(next.next_resource_ordinal.get(), 4);
        assert_eq!(NextId::decode(&next.encode())?, next);
        Ok(())
    }

    #[test]
    fn control_next_id_rejects_bad_magic_version_crc_and_zero() -> Result<(), ControlError> {
        let mut bytes = NextId::initial().encode();
        bytes[0] = b'B';
        assert_eq!(
            NextId::decode(&bytes).err(),
            Some(ControlError::InvalidMagic)
        );

        let mut bytes = NextId::initial().encode();
        bytes[8] = 9;
        assert_eq!(
            NextId::decode(&bytes).err(),
            Some(ControlError::UnsupportedVersion)
        );

        let mut bytes = NextId::initial().encode();
        bytes[10] = 1;
        assert_eq!(
            NextId::decode(&bytes).err(),
            Some(ControlError::UnsupportedFlags)
        );

        let mut bytes = NextId::initial().encode();
        bytes[31] ^= 0xff;
        assert_eq!(
            NextId::decode(&bytes).err(),
            Some(ControlError::ChecksumMismatch)
        );

        let mut bytes = NextId::initial().encode();
        bytes[12..20].copy_from_slice(&0_u64.to_le_bytes());
        let checksum = crc32(&bytes[..28]);
        bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            NextId::decode(&bytes).err(),
            Some(ControlError::InvalidIdentity)
        );
        assert_eq!(
            NextId::decode(&bytes[..31]).err(),
            Some(ControlError::Truncated)
        );
        let mut trailing = NextId::initial().encode().to_vec();
        trailing.push(0);
        assert_eq!(
            NextId::decode(&trailing).err(),
            Some(ControlError::TrailingBytes)
        );
        Ok(())
    }

    #[test]
    fn control_reservation_round_trip_and_exact_length() -> Result<(), ControlError> {
        let reservation = Reservation {
            document: NonZeroU64::new(7).ok_or(ControlError::InvalidIdentity)?,
            generation: NonZeroU64::new(2).ok_or(ControlError::InvalidIdentity)?,
            reserved_bytes: 4096,
        };
        let bytes = reservation.encode();
        assert_eq!(bytes.len(), 40);
        assert_eq!(Reservation::decode(&bytes)?, reservation);
        assert_eq!(
            Reservation::decode(&bytes[..39]).err(),
            Some(ControlError::Truncated)
        );
        Ok(())
    }

    #[test]
    fn control_reservation_rejects_trailing_bytes_and_zero_identity() -> Result<(), ControlError> {
        let reservation = Reservation {
            document: NonZeroU64::MIN,
            generation: NonZeroU64::MIN,
            reserved_bytes: 0,
        };
        let mut trailing = reservation.encode().to_vec();
        trailing.push(0);
        assert_eq!(
            Reservation::decode(&trailing).err(),
            Some(ControlError::TrailingBytes)
        );
        let mut zero_document = reservation.encode();
        zero_document[12..20].copy_from_slice(&0_u64.to_le_bytes());
        let checksum = crc32(&zero_document[..36]);
        zero_document[36..40].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reservation::decode(&zero_document).err(),
            Some(ControlError::InvalidIdentity)
        );
        let mut zero_generation = reservation.encode();
        zero_generation[20..28].copy_from_slice(&0_u64.to_le_bytes());
        let checksum = crc32(&zero_generation[..36]);
        zero_generation[36..40].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reservation::decode(&zero_generation).err(),
            Some(ControlError::InvalidIdentity)
        );
        Ok(())
    }

    #[test]
    fn control_overflow_is_rejected_without_wrap() -> Result<(), ControlError> {
        let mut next = NextId::initial();
        assert_eq!(
            next.advance_generation(NonZeroU64::MAX),
            Err(ControlError::Overflow)
        );
        assert_eq!(
            next.advance_resource_ordinal(usize::MAX),
            Err(ControlError::Overflow)
        );
        assert_eq!(next, NextId::initial());
        assert_eq!(
            NextId::decode(&[0_u8; 31]).err(),
            Some(ControlError::Truncated)
        );
        Ok(())
    }
}
