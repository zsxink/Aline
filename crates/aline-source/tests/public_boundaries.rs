//! 存储字节身份的外部消费；不把原始字节视图当作已解码文件。

use aline_source::{DiskVersion, StorageBudget, StorageError, StorageSnapshot};

#[test]
fn storage_snapshot_can_be_carried_by_future_host_tasks() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<StorageSnapshot>();
}

#[test]
fn storage_ranges_are_bound_to_their_own_baseline() -> Result<(), Box<dyn std::error::Error>> {
    let first = StorageSnapshot::new(
        vec![0xef, 0xbb, 0xbf, b'x'],
        DiskVersion::initial(),
        StorageBudget::new(8),
    )?;
    let cloned = first.clone();
    let second = StorageSnapshot::new(
        vec![0xef, 0xbb, 0xbf, b'x'],
        DiskVersion::initial(),
        StorageBudget::new(8),
    )?;
    let range = first.range(0..3)?;
    assert_eq!(cloned.slice(&range)?, &[0xef, 0xbb, 0xbf]);
    assert!(matches!(
        second.slice(&range),
        Err(StorageError::BaselineMismatch)
    ));
    assert!(matches!(
        first.range(std::ops::Range { start: 3, end: 2 }),
        Err(StorageError::ReversedRange)
    ));
    assert!(matches!(
        first.range(0..5),
        Err(StorageError::OutOfBounds { .. })
    ));
    assert!(matches!(
        DiskVersion::from_value(u64::MAX).next(),
        Err(StorageError::VersionExhausted)
    ));
    Ok(())
}
