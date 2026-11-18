use super::*;
use crate::document::{DocCore, DocError, DocLimits, PublicationOutcome, WaitStatus};
use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec};
use aline_source::DiskVersion;

// 以下导入只被 macOS 真实文件会话测试使用；条件导入避免其他平台测试构建
// 出现未使用导入并在 `-D warnings` 下失败。
#[cfg(target_os = "macos")]
use crate::document::{CloseDisposition, OpenDisposition, ReadonlyOpenLimits, ReadonlySession};
#[cfg(target_os = "macos")]
use aline_markdown::{ParseBudget, ParseConfig};

const TEXT: TextBudget = TextBudget::new(4096);
const STORAGE: StorageBudget = StorageBudget::new(4096);
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn baseline(bytes: &[u8], version: u64) -> Result<SourceBaseline, BaselineError> {
    let raw = StorageSnapshot::new(bytes.to_vec(), DiskVersion::from_value(version), STORAGE)?;
    Ok(decode(&raw, STORAGE)?.into_baseline())
}

fn fixture(bytes: &[u8]) -> Result<(DocCore, PersistenceCoordinator), Box<dyn std::error::Error>> {
    let baseline = baseline(bytes, 0)?;
    let core = DocCore::new(
        baseline.source(),
        SelectionSpec::new(1, 1),
        DocLimits::new(
            TEXT,
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 4096),
        ),
    )?;
    let state = PersistenceCoordinator::new(
        core.document_id().clone(),
        fixture_path("notes.md"),
        baseline,
    )
    .map_err(|error| format!("{error:?}"))?;
    Ok((core, state))
}

fn queue(
    state: &mut PersistenceCoordinator,
    core: &mut DocCore,
) -> Result<OperationId, Box<dyn std::error::Error>> {
    match state
        .request_save(core, TEXT)
        .map_err(|error| format!("{error:?}"))?
    {
        SaveRequest::Queued(id) => Ok(id),
        SaveRequest::Waiting(_) => Err("无组合输入时应直接排队".into()),
    }
}

fn start(state: &mut PersistenceCoordinator) -> Result<FrozenWrite, String> {
    state.start().map_err(|error| format!("{error:?}"))
}

fn published(
    work: FrozenWrite,
    bytes: &[u8],
    version: u64,
    synchronized: bool,
) -> Result<WriteCompletion, BaselineError> {
    Ok(WriteCompletion {
        job: work,
        outcome: WriteOutcome::Published {
            storage: baseline(bytes, version)?,
            synchronized,
        },
    })
}

#[test]
fn persistence_publication_waits_for_latest_confirmed_or_cancelled_input() -> TestResult {
    for (outcome, expected, depth) in [
        (PublicationOutcome::PlatformConfirmed, "x中文\n", 1),
        (PublicationOutcome::PlatformCancelled, "x\n", 0),
    ] {
        let (mut doc, mut state) = fixture(b"x\n")?;
        doc.composition_start("中", 0)?;
        let SaveRequest::Waiting(handle) = state
            .request_save(&mut doc, TEXT)
            .map_err(|error| format!("{error:?}"))?
        else {
            return Err("活动组合应等待平台结果".into());
        };
        assert_eq!(state.status(), SaveStatus::WaitingForComposition);
        assert_eq!(state.start().err(), Some(SaveError::NoQueuedWrite));
        assert_eq!(doc.wait().map(|wait| wait.handle()), Some(handle));
        assert_eq!(state.last_operation, 0);

        doc.composition_update("中文", 1)?;
        let candidate = doc.current_snapshot(TEXT)?;
        assert!(matches!(
            state.finish_save_wait(&mut doc, handle, outcome, 2),
            Err(SaveRequestError::Input(DocError::CompositionActive))
        ));
        assert_eq!(
            doc.current_snapshot(TEXT)?.snapshot_id(),
            candidate.snapshot_id()
        );
        assert_eq!(doc.undo_depth(), 0);
        assert_eq!(
            state.replace_association(
                state.path_generation(),
                fixture_path("unrelated.md"),
                &candidate,
                baseline(b"x\n", 0)?,
            ),
            Err(SaveError::Busy)
        );

        match outcome {
            PublicationOutcome::PlatformConfirmed => {
                doc.composition_commit(3)?;
            }
            PublicationOutcome::PlatformCancelled => doc.composition_cancel(3)?,
        }
        let frozen = doc.current_snapshot(TEXT)?;
        let operation = state
            .finish_save_wait(&mut doc, handle, outcome, 4)
            .map_err(|error| format!("{error:?}"))?;
        assert!(doc.wait().is_none());
        assert_eq!(doc.undo_depth(), depth);
        assert!(matches!(
            state.finish_save_wait(&mut doc, handle, outcome, 5),
            Err(SaveRequestError::Input(DocError::UnknownWait))
        ));

        let work = start(&mut state)?;
        assert_eq!(work.path(), fixture_path("notes.md"));
        assert_eq!(
            work.encode(TEXT, STORAGE)
                .map_err(|error| format!("{error:?}"))?
                .bytes()
                .bytes(),
            expected.as_bytes()
        );
        doc.type_text(frozen.selection().head().byte_offset(), "a", 6)?;
        let latest = doc.current_snapshot(TEXT)?;
        state
            .begin_close(&mut doc)
            .map_err(|error| format!("{error:?}"))?;
        assert!(work.cancellation().is_cancelled());
        assert_eq!(state.close_status(&latest), CloseStatus::Waiting);
        let receipt = state.reconcile(published(work, expected.as_bytes(), 1, true)?);
        assert_eq!(receipt.operation, operation);
        assert_eq!(receipt.snapshot.snapshot_id(), frozen.snapshot_id());
        assert_eq!(receipt.snapshot.selection(), frozen.selection());
        assert!(receipt.adopted);
        assert!(!state.is_dirty(&frozen));
        assert!(state.is_dirty(&latest));
        assert_eq!(state.close_status(&latest), CloseStatus::NeedsDecision);
        assert_eq!(
            doc.current_snapshot(TEXT)?.snapshot_id(),
            latest.snapshot_id()
        );
        assert_eq!(doc.undo_depth(), depth);
    }
    Ok(())
}

#[test]
fn persistence_publication_cancelled_wait_rejects_late_result_and_preserves_composition()
-> TestResult {
    for closing in [false, true] {
        let (mut doc, mut state) = fixture(b"x\n")?;
        doc.composition_start("中", 0)?;
        let SaveRequest::Waiting(old) = state
            .request_save(&mut doc, TEXT)
            .map_err(|error| format!("{error:?}"))?
        else {
            return Err("活动组合应等待平台结果".into());
        };
        let before = doc.current_snapshot(TEXT)?;
        if closing {
            state
                .begin_close(&mut doc)
                .map_err(|error| format!("{error:?}"))?;
            assert!(matches!(
                state.request_save(&mut doc, TEXT),
                Err(SaveRequestError::Queue(SaveError::Closing))
            ));
            state.cancel_close();
        } else {
            state
                .cancel_save_wait(&mut doc, old)
                .map_err(|error| format!("{error:?}"))?;
        }
        assert_eq!(
            doc.wait().map(|wait| wait.status()),
            Some(WaitStatus::Cancelled)
        );
        assert_eq!(
            doc.current_snapshot(TEXT)?.snapshot_id(),
            before.snapshot_id()
        );
        assert_eq!(doc.undo_depth(), 0);
        assert_eq!(state.status(), SaveStatus::Idle);

        doc.composition_update("中文", 1)?;
        let SaveRequest::Waiting(new) = state
            .request_save(&mut doc, TEXT)
            .map_err(|error| format!("{error:?}"))?
        else {
            return Err("第二次请求仍应等待平台结果".into());
        };
        doc.composition_commit(2)?;
        let frozen = doc.current_snapshot(TEXT)?;
        assert!(matches!(
            state.finish_save_wait(&mut doc, old, PublicationOutcome::PlatformConfirmed, 3),
            Err(SaveRequestError::Input(DocError::UnknownWait))
        ));
        assert_eq!(doc.wait().map(|wait| wait.handle()), Some(new));
        assert_eq!(state.last_operation, 0);
        state
            .finish_save_wait(&mut doc, new, PublicationOutcome::PlatformConfirmed, 4)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            doc.current_snapshot(TEXT)?.snapshot_id(),
            frozen.snapshot_id()
        );
        assert_eq!(doc.undo_depth(), 1);
        let work = start(&mut state)?;
        assert_eq!(work.snapshot.snapshot_id(), frozen.snapshot_id());
    }
    Ok(())
}

#[test]
fn persistence_state_frozen_write_and_newer_input_keep_history() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\r\ny\r\n")?;
    doc.type_text(1, "\na", 0)?;
    let frozen = doc.current_snapshot(TEXT)?;
    assert!(state.is_dirty(&frozen));
    let id = queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let history = (doc.undo_depth(), doc.redo_depth());
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        frozen.snapshot_id()
    );
    assert_eq!(
        work.encode(TEXT, STORAGE)
            .map_err(|error| format!("{error:?}"))?
            .bytes()
            .bytes(),
        b"x\r\na\r\ny\r\n"
    );

    doc.type_text(3, "b", 100)?;
    let latest = doc.current_snapshot(TEXT)?;
    let latest_id = queue(&mut state, &mut doc)?;
    assert_eq!(state.start().err(), Some(SaveError::Busy));
    assert!(matches!(
        state.request_save(&mut doc, TEXT),
        Err(SaveRequestError::Queue(SaveError::Busy))
    ));
    let completed = state.reconcile(published(work, b"x\r\na\r\ny\r\n", 1, true)?);
    assert!(completed.adopted);
    assert_eq!(completed.operation, id);
    assert_eq!(&completed.path, &fixture_path("notes.md"));
    assert_eq!(completed.published_target(), Some(completed.path.as_path()));
    assert!(completed.failure().is_none());
    assert_eq!(completed.snapshot.version(), frozen.version());
    assert_eq!(completed.snapshot.selection(), frozen.selection());
    assert_eq!((doc.undo_depth(), doc.redo_depth()), history);
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        latest.snapshot_id()
    );
    assert!(!state.is_dirty(&frozen));
    assert!(state.is_dirty(&latest));

    let work = start(&mut state)?;
    assert_eq!(work.baseline().storage().version().value(), 1);
    let completed = state.reconcile(published(work, b"x\r\nab\r\ny\r\n", 2, true)?);
    assert_eq!(completed.operation, latest_id);
    assert!(!state.is_dirty(&latest));
    assert_eq!(state.close_status(&latest), CloseStatus::Ready);
    Ok(())
}

#[test]
fn persistence_state_equivalent_content_after_undo_redo_and_save_encoding() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\r\ny\r\n")?;
    doc.type_text(1, "\na", 0)?;
    let saved = doc.current_snapshot(TEXT)?;
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    state.reconcile(published(work, b"x\r\na\r\ny\r\n", 1, true)?);
    doc.commit_open_block(100)?;
    doc.undo()?;
    assert!(state.is_dirty(&doc.current_snapshot(TEXT)?));
    doc.redo()?;
    let redone = doc.current_snapshot(TEXT)?;
    assert_ne!(redone.snapshot_id(), saved.snapshot_id());
    assert!(!state.is_dirty(&redone));
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let encoded = work
        .encode(TEXT, STORAGE)
        .map_err(|error| format!("{error:?}"))?;
    assert!(encoded.bytes().reused_original());
    assert_eq!(encoded.bytes().bytes(), b"x\r\na\r\ny\r\n");
    assert_eq!(redone.full_text(TEXT)?, "x\na\r\ny\r\n");
    Ok(())
}

#[test]
fn persistence_state_cancel_close_during_write_still_reconciles_publication() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    doc.type_text(1, "a", 0)?;
    let first = doc.current_snapshot(TEXT)?;
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    doc.type_text(2, "b", 100)?;
    let latest = doc.current_snapshot(TEXT)?;
    let queued = queue(&mut state, &mut doc)?;

    state
        .begin_close(&mut doc)
        .map_err(|error| format!("{error:?}"))?;
    assert!(work.cancellation().is_cancelled());
    assert_eq!(state.close_status(&latest), CloseStatus::Waiting);
    assert!(matches!(
        state.request_save(&mut doc, TEXT),
        Err(SaveRequestError::Queue(SaveError::Closing))
    ));
    assert!(!state.cancel(queued));
    let settlement = state.reconcile(published(work, b"xa\n", 1, true)?);
    assert!(settlement.adopted);
    assert!(!state.is_dirty(&first));
    assert_eq!(state.close_status(&latest), CloseStatus::NeedsDecision);
    state.cancel_close();
    assert_eq!(state.start().err(), Some(SaveError::NoQueuedWrite));
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        latest.snapshot_id()
    );
    assert_eq!(doc.open_block_draft(), Some("ab"));
    Ok(())
}

#[test]
fn persistence_state_uncertain_publication_retains_both_baselines_and_blocks_close() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    doc.type_text(1, "a", 0)?;
    let current = doc.current_snapshot(TEXT)?;
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let settlement = state.reconcile(published(work, b"xa\n", 1, false)?);
    assert!(settlement.adopted);
    assert!(matches!(settlement.outcome, WriteOutcome::Published {
        storage, synchronized: false,
    } if storage.storage().bytes() == b"xa\n"));
    assert_eq!(state.status(), SaveStatus::PublishedUncertain);
    assert!(!state.is_dirty(&current));
    assert_eq!(state.close_status(&current), CloseStatus::NeedsDecision);
    queue(&mut state, &mut doc)?;
    assert_eq!(state.start().err(), Some(SaveError::Busy));
    Ok(())
}

#[test]
fn persistence_state_rejects_path_change_in_flight_and_foreign_late_failure() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    let initial = doc.current_snapshot(TEXT)?;
    let mut obsolete = PersistenceCoordinator::new(
        doc.document_id().clone(),
        fixture_path("notes.md"),
        baseline(b"x\n", 0)?,
    )
    .map_err(|error| format!("{error:?}"))?;
    queue(&mut state, &mut doc)?;
    queue(&mut obsolete, &mut doc)?;
    let active = start(&mut state)?;
    let stale = start(&mut obsolete)?;
    assert_eq!(active.path(), stale.path());
    let target = fixture_path("other.md");
    let generation = state.path_generation();
    assert_eq!(
        state.replace_association(generation, target.clone(), &initial, baseline(b"x\n", 0)?),
        Err(SaveError::Busy)
    );
    let success = state.reconcile(published(active, b"x\n", 1, true)?);
    assert!(success.adopted);
    assert_eq!(
        state.replace_association(generation, target.clone(), &initial, baseline(b"y\n", 0)?),
        Err(SaveError::InconsistentBaseline)
    );
    state
        .replace_association(generation, target.clone(), &initial, baseline(b"x\n", 0)?)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        state.replace_association(
            generation,
            fixture_path("late-open.md"),
            &initial,
            baseline(b"x\n", 0)?
        ),
        Err(SaveError::StaleAssociation)
    );
    let rejected = state.reconcile(stale.stop_before_write(PrepareError::Conflict));
    assert!(!rejected.adopted);
    assert_eq!(state.status(), SaveStatus::Idle);
    assert_eq!(state.close_status(&initial), CloseStatus::Ready);
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    assert_eq!(work.path(), target);
    Ok(())
}

#[test]
fn persistence_state_failed_or_cancelled_before_publication_retains_input() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    doc.type_text(1, "a", 0)?;
    let initial = doc.current_snapshot(TEXT)?;
    let operation = queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    assert!(state.cancel(operation));
    assert!(matches!(
        work.encode(TEXT, STORAGE),
        Err(PrepareError::Cancelled)
    ));
    let cancelled = state.reconcile(work.stop_before_write(PrepareError::Cancelled));
    assert!(!cancelled.adopted);
    assert_eq!(state.status(), SaveStatus::Idle);
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let error = work
        .encode(TextBudget::new(0), STORAGE)
        .err()
        .ok_or("源码预算必须拒绝")?;
    let failed = state.reconcile(work.stop_before_write(error));
    let expected = BufferError::BudgetExceeded {
        needed: 3,
        limit: 0,
    };
    assert_eq!(
        failed.failure().map(ToString::to_string),
        Some(expected.to_string())
    );
    assert!(matches!(
        failed.outcome,
        WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Text(
            BufferError::BudgetExceeded {
                needed: 3,
                limit: 0
            }
        )))
    ));
    assert_eq!(state.status(), SaveStatus::Failed);
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let error = work
        .encode(TEXT, StorageBudget::new(0))
        .err()
        .ok_or("预算应拒绝")?;
    assert!(matches!(
        error,
        PrepareError::Encoding(EncodeError::BudgetExceeded {
            needed: 3,
            limit: 0
        })
    ));
    let failed = state.reconcile(work.stop_before_write(error));
    assert!(failed.adopted);
    assert!(matches!(
        failed.outcome,
        WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Encoding(
            EncodeError::BudgetExceeded {
                needed: 3,
                limit: 0
            }
        )))
    ));
    assert_eq!(state.status(), SaveStatus::Failed);
    assert!(state.is_dirty(&initial));
    assert_eq!(state.close_status(&initial), CloseStatus::NeedsDecision);
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        initial.snapshot_id()
    );
    assert_eq!(doc.buffer_text()?, "x\n");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}

#[test]
fn persistence_state_source_failure_retains_original_file_error() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    let encoded = work
        .encode(TEXT, STORAGE)
        .map_err(|error| format!("{error:?}"))?;
    let result = aline_source::prepare_atomic(
        encoded.bytes(),
        work.path(),
        work.path(),
        aline_source::IoBudget::new(0),
        work.cancellation(),
    );
    let error = result.err().ok_or("零字节预算必须拒绝写入")?;
    let receipt = state.reconcile(work.finish_replacement(&encoded, Err(error)));
    assert_eq!(receipt.published_target(), None);
    assert!(receipt.failure().is_some());
    assert!(matches!(
        receipt.outcome,
        WriteOutcome::NotPublished(WriteStop::File(error))
        if matches!(error.kind(), AtomicFailureKind::Budget { needed: 2, limit: 0 })
            && error.staging_path().is_none()
    ));
    assert_eq!(state.status(), SaveStatus::Failed);
    Ok(())
}

#[cfg(target_os = "macos")]
struct TempSaveDir(PathBuf);

#[cfg(target_os = "macos")]
impl TempSaveDir {
    /// 在系统临时目录下独占创建本测试使用的目录；Drop 时递归清理。
    fn create(prefix: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100u32 {
            let candidate = fixture_path(&format!(
                "{prefix}-{}-{nonce:x}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("无法创建保存测试目录".into())
    }
}

#[cfg(target_os = "macos")]
impl Drop for TempSaveDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 真实磁盘保存夹具：关联 `x\n` 基线并排队一次输入 `a`。
///
/// 五元组各字段都是测试直接使用的独立句柄；拆成结构体不会减少调用方代码。
#[cfg(target_os = "macos")]
#[allow(clippy::type_complexity)]
fn macos_save_fixture() -> Result<
    (
        TempSaveDir,
        PathBuf,
        DocCore,
        PersistenceCoordinator,
        FrozenWrite,
    ),
    Box<dyn std::error::Error>,
> {
    let owned = TempSaveDir::create("aline-save")?;
    let target = owned.0.join("notes.md");
    std::fs::write(&target, b"x\n")?;
    let (mut doc, mut state) = fixture(b"x\n")?;
    let initial = doc.current_snapshot(TEXT)?;
    state
        .replace_association(
            state.path_generation(),
            target.clone(),
            &initial,
            baseline(b"x\n", 1)?,
        )
        .map_err(|error| format!("{error:?}"))?;
    doc.type_text(1, "a", 0)?;
    queue(&mut state, &mut doc)?;
    let work = start(&mut state)?;
    Ok((owned, target, doc, state, work))
}

/// 以不同内容替换目标，同时保持字节长度和 mtime 不变。
#[cfg(target_os = "macos")]
fn rewrite_same_size_and_mtime(target: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, FileTimes, OpenOptions};

    let before = fs::metadata(target)?.modified()?;
    fs::write(target, b"y\n")?;
    OpenOptions::new()
        .write(true)
        .open(target)?
        .set_times(FileTimes::new().set_modified(before))?;
    assert_eq!(fs::metadata(target)?.modified()?, before);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_state_reconciles_publish_conflict_as_conflict() -> TestResult {
    use std::fs;

    let (owned, target, mut doc, mut state, work) = macos_save_fixture()?;
    let observe = || {
        ExistingTarget::observe(
            &target,
            work.baseline().storage().version(),
            aline_source::IoBudget::new(4096),
            work.cancellation(),
        )
    };
    work.check_observation(&observe()?)
        .map_err(|error| format!("{error:?}"))?;
    let staging = owned.0.join("stage.md");
    let encoded = work
        .encode(TEXT, STORAGE)
        .map_err(|error| format!("{error:?}"))?;
    // macOS 平台受检替换已按 §44 准入实现，staging 创建不再失败关闭；
    // 同尺寸、同时间戳的外部改写必须在发布前的重新观测中按内容比对拒绝。
    let prepared = aline_source::prepare_checked_replace(
        encoded.bytes(),
        &observe()?,
        &staging,
        aline_source::IoBudget::new(4096),
        work.cancellation(),
    )
    .map_err(|error| format!("{error:?}"))?;
    rewrite_same_size_and_mtime(&target)?;
    let rejected = prepared
        .publish(work.cancellation())
        .err()
        .ok_or("发布前重新观测必须拒绝同尺寸同时间戳的外改")?;
    assert!(matches!(
        rejected.kind(),
        AtomicFailureKind::Conflict {
            target_exists: true
        }
    ));
    assert_eq!(rejected.staging_path(), Some(staging.as_path()));
    let receipt = state.reconcile(work.finish_replacement(&encoded, Err(rejected)));
    assert!(receipt.adopted);
    assert_eq!(receipt.published_target(), None);
    assert_eq!(state.status(), SaveStatus::Conflict);
    assert!(state.is_dirty(&doc.current_snapshot(TEXT)?));
    assert_eq!(fs::read(&target)?, b"y\n");
    assert!(staging.try_exists()?);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_state_refuses_reobserved_same_length_changed_content() -> TestResult {
    use std::fs;

    let (_owned, target, mut doc, mut state, work) = macos_save_fixture()?;
    rewrite_same_size_and_mtime(&target)?;
    let error = work
        .check_observation(&ExistingTarget::observe(
            &target,
            work.baseline().storage().version(),
            aline_source::IoBudget::new(4096),
            work.cancellation(),
        )?)
        .err()
        .ok_or("同尺寸、同时间戳的不同内容必须构成冲突")?;
    assert!(matches!(error, PrepareError::Conflict));
    assert!(state.reconcile(work.stop_before_write(error)).adopted);
    assert_eq!(state.status(), SaveStatus::Conflict);
    assert_eq!(fs::read(&target)?, b"y\n");
    assert_eq!(doc.current_snapshot(TEXT)?.full_text(TEXT)?, "xa\n");
    Ok(())
}

/// 打开真实绝对路径上的可编辑会话；目标必须已存在。
#[cfg(target_os = "macos")]
fn editable_absolute_session(path: &Path) -> Result<ReadonlySession, Box<dyn std::error::Error>> {
    let limits = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(4096),
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
    .with_editing();
    let mut session = ReadonlySession::new(limits);
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![path.to_path_buf()]))?
        .ok_or("应生成打开工作")?;
    assert!(
        matches!(session.accept_open(job.run()), OpenDisposition::Applied),
        "真实文件打开应发布可编辑文档"
    );
    Ok(session)
}

/// 保存入口错误只用于测试诊断；测试结果统一装箱为动态错误。
#[cfg(target_os = "macos")]
fn save_request_error(error: SaveRequestError) -> Box<dyn std::error::Error> {
    format!("{error:?}").into()
}

/// 在全文范围打开旧式输入块；无窗口可编辑打开不会自动打开块。
/// 测试需要跨末尾换行继续输入，因此不使用限定在段落视觉范围内的块。
#[cfg(target_os = "macos")]
fn open_editing_block(session: &mut ReadonlySession) -> Result<(), Box<dyn std::error::Error>> {
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    let len = document.baseline().source().len();
    document
        .edit_core()?
        .open_block(0..len, SelectionSpec::new(0, 0))?;
    Ok(())
}

/// 请求一次显式保存并返回排队操作号；组合等待不属于这些会话用例。
#[cfg(target_os = "macos")]
fn queue_save(session: &mut ReadonlySession) -> Result<OperationId, Box<dyn std::error::Error>> {
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    match document
        .request_persistence_save()
        .map_err(save_request_error)?
    {
        SaveRequest::Queued(operation) => Ok(operation),
        SaveRequest::Waiting(_) => Err("无组合输入时应直接排队".into()),
    }
}

/// 取走已排队保存，同步执行真实后台工作并交回会话结算。
#[cfg(target_os = "macos")]
fn run_save(
    session: &mut ReadonlySession,
    staging: PathBuf,
) -> Result<WriteSettlement, Box<dyn std::error::Error>> {
    let completion = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .start_save_job(staging)
        .map_err(save_request_error)?
        .run();
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save(completion)
        .map_err(save_request_error)
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_session_save_publishes_receipt_and_adopts_baseline() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-session-publish")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;

    let (text, version, selection, undo_depth, history_nodes, buffer_text) = {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let snapshot = document.current_snapshot()?;
        (
            snapshot.full_text(TEXT)?,
            snapshot.version(),
            snapshot.selection().clone(),
            document.core().undo_depth(),
            document.core().history_node_count(),
            document.core().buffer_text()?,
        )
    };
    assert_eq!(text, "xa\n", "输入后的快照应包含草稿");

    let operation = queue_save(&mut session)?;
    let staging = directory.0.join("stage.md");
    let settlement = run_save(&mut session, staging.clone())?;

    assert!(settlement.adopted, "真实发布回执应被采纳");
    assert!(
        matches!(
            &settlement.outcome,
            WriteOutcome::Published {
                synchronized: true,
                ..
            }
        ),
        "目录同步成功的发布必须标记 synchronized，实际为 {:?}",
        settlement.outcome
    );
    assert_eq!(settlement.operation, operation, "结算项应属于本次排队操作");
    assert_eq!(settlement.path, target, "结算路径应等于实际目标路径");
    assert_eq!(fs::read(&target)?, b"xa\n", "真实替换后磁盘应包含新内容");
    assert!(!staging.exists(), "成功发布后 staging 应被 source 清理");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(
        document.baseline().source(),
        "xa\n",
        "成功回执应替换文档磁盘基线"
    );
    let snapshot = document.current_snapshot()?;
    assert_eq!(snapshot.full_text(TEXT)?, text, "保存不应改变当前全文");
    assert_eq!(snapshot.version(), version, "保存不应推进工作版本");
    assert_eq!(snapshot.selection(), &selection, "保存不应改变选区");
    assert_eq!(
        document.core().undo_depth(),
        undo_depth,
        "保存不应改变撤销深度"
    );
    assert_eq!(
        document.core().history_node_count(),
        history_nodes,
        "保存不应改变历史节点数"
    );
    assert_eq!(
        document.core().buffer_text()?,
        buffer_text,
        "保存不应改变已提交文本"
    );
    assert_eq!(
        document.persistence_status(),
        Some(SaveStatus::Idle),
        "发布成功后协调器应回到空闲"
    );
    assert_eq!(
        session.close_document(),
        CloseDisposition::Closed,
        "保存成功后应允许关闭"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_session_save_in_flight_refuses_close_until_settled() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-session-inflight")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;

    let operation = queue_save(&mut session)?;
    let job = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .start_save_job(directory.0.join("stage.md"))
        .map_err(save_request_error)?;
    assert_eq!(
        session.close_document(),
        CloseDisposition::Refused,
        "在途保存必须拒绝关闭"
    );
    assert!(session.document().is_some(), "拒绝关闭必须保留文档");
    let text = session
        .document_mut()
        .ok_or("拒绝关闭后应保留文档")?
        .current_snapshot()?
        .full_text(TEXT)?;
    assert_eq!(text, "xa\n", "拒绝关闭不得改变内存文本");

    let settlement = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save(job.run())
        .map_err(save_request_error)?;
    assert!(settlement.adopted, "后台真实发布应被采纳");
    assert_eq!(settlement.operation, operation, "结算项应属于在途操作");
    assert_eq!(
        session.close_document(),
        CloseDisposition::Closed,
        "结算成功后应允许关闭"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_session_continues_editing_after_published_save() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-session-continue")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;

    let first_operation = queue_save(&mut session)?;
    let first = run_save(&mut session, directory.0.join("stage.md"))?;
    assert_eq!(first.operation, first_operation, "首次结算项应匹配排队操作");
    assert!(first.adopted, "首次真实发布应被采纳");
    let first_version = match &first.outcome {
        WriteOutcome::Published { storage, .. } => storage.storage().version(),
        other => return Err(format!("首次保存应成功发布，实际为 {other:?}").into()),
    };
    assert_eq!(fs::read(&target)?, b"xa\n", "首次保存应写入新内容");
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(
            document.baseline().source(),
            "xa\n",
            "首次保存应更新文档磁盘基线"
        );
    }

    // 首次保存不改写内存草稿，逻辑文本仍为 "xa\n"；在换行前偏移 2 输入
    // 得到 "xab\n"（brief 的偏移 3 位于换行之后，会得到 "xa\nb"）。
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(2, "b", 1)?;
    assert_eq!(
        session.close_document(),
        CloseDisposition::Refused,
        "相对实际保存基线仍有修改时必须拒绝关闭"
    );
    assert!(session.document().is_some(), "拒绝关闭必须保留文档");

    let second_operation = queue_save(&mut session)?;
    let second = run_save(&mut session, directory.0.join("stage2.md"))?;
    assert_eq!(
        second.operation, second_operation,
        "第二次结算项应匹配排队操作"
    );
    assert!(second.adopted, "第二次真实发布应被采纳");
    let second_version = match &second.outcome {
        WriteOutcome::Published { storage, .. } => storage.storage().version(),
        other => return Err(format!("第二次保存应成功发布，实际为 {other:?}").into()),
    };
    assert!(
        second_version > first_version,
        "第二次发布的磁盘代次必须严格大于第一次"
    );
    assert_eq!(fs::read(&target)?, b"xab\n", "第二次保存应写入新内容");
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .persistence_status(),
        Some(SaveStatus::Idle),
        "第二次发布成功后协调器应回到空闲"
    );
    assert_eq!(
        session.close_document(),
        CloseDisposition::Closed,
        "第二次保存成功后应允许关闭"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_session_external_change_conflict_preserves_memory_and_disk() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-session-conflict")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    rewrite_same_size_and_mtime(&target)?;

    let operation = queue_save(&mut session)?;
    let staging = directory.0.join("stage-conflict.md");
    let settlement = run_save(&mut session, staging.clone())?;
    assert_eq!(settlement.operation, operation, "冲突结算项应匹配排队操作");
    assert!(settlement.adopted, "冲突完成项仍须结算以记录状态");
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .persistence_status(),
        Some(SaveStatus::Conflict),
        "外部内容变化必须结算为冲突"
    );
    assert_eq!(
        settlement.published_target(),
        None,
        "冲突不得报告已发布目标"
    );
    assert_eq!(fs::read(&target)?, b"y\n", "外部内容不得被覆盖");
    assert!(!staging.exists(), "观察/复核冲突前不得创建 staging");
    let text = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .current_snapshot()?
        .full_text(TEXT)?;
    assert_eq!(text, "xa\n", "冲突结算必须保留内存文本");
    assert_eq!(
        session.close_document(),
        CloseDisposition::Refused,
        "未结算冲突必须拒绝关闭"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn persistence_session_missing_target_settles_conflict_without_recreating_file() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-session-missing")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    let buffer_text = session
        .document()
        .ok_or("应存在可编辑文档")?
        .core()
        .buffer_text()?;
    fs::remove_file(&target)?;

    let operation = queue_save(&mut session)?;
    let settlement = run_save(&mut session, directory.0.join("stage-missing.md"))?;
    assert_eq!(settlement.operation, operation, "冲突结算项应匹配排队操作");
    assert!(settlement.adopted, "目标缺失冲突仍须结算并记录状态");
    assert!(
        matches!(
            &settlement.outcome,
            WriteOutcome::NotPublished(WriteStop::File(error))
                if matches!(
                    error.kind(),
                    AtomicFailureKind::Conflict {
                        target_exists: false
                    }
                )
        ),
        "目标删除必须按 source 的缺失冲突事实交回，实际为 {:?}",
        settlement.outcome
    );
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .persistence_status(),
        Some(SaveStatus::Conflict),
        "目标不存在沿用 source 分类结算为冲突，而不是保存失败"
    );
    assert_eq!(
        settlement.published_target(),
        None,
        "目标缺失不得报告已发布目标"
    );
    let text = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .current_snapshot()?
        .full_text(TEXT)?;
    assert_eq!(text, "xa\n", "失败结算必须保留内存全文");
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .core()
            .buffer_text()?,
        buffer_text,
        "失败结算不得改变已提交文本"
    );
    assert_eq!(
        session.close_document(),
        CloseDisposition::Refused,
        "未结算失败必须拒绝关闭"
    );
    assert!(!target.exists(), "失败保存不得重建或截断原路径");
    Ok(())
}
