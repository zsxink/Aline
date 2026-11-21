use super::*;
use crate::document::{
    AutosaveAction, DocCore, DocError, DocLimits, PublicationOutcome, WaitStatus,
};
use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec};
use aline_source::DiskVersion;

// 以下导入只被 macOS 真实文件会话测试使用；条件导入避免其他平台测试构建
// 出现未使用导入并在 `-D warnings` 下失败。
#[cfg(target_os = "macos")]
use crate::document::readonly_session::{
    AssociationCommand, AssociationOutcome, AssociationStep, ReadonlyFileAccess, ReadonlyFileInput,
};
#[cfg(target_os = "macos")]
use crate::document::resources::{ImageDirectory, ImageSettings, MigrationCompletion, TempRoot};
#[cfg(target_os = "macos")]
use crate::document::{
    CloseDisposition, EditorMode, OpenDisposition, ReadonlyOpenLimits, ReadonlySession,
    SaveTargetCommand,
};
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
fn persistence_waiting_save_rejects_reassociation() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    doc.composition_start("候", 0)?;
    let SaveRequest::Waiting(handle) = state
        .request_save(&mut doc, TEXT)
        .map_err(|error| format!("{error:?}"))?
    else {
        return Err("活动组合应等待平台结果".into());
    };
    let before = doc.current_snapshot(TEXT)?;
    assert_eq!(
        state.replace_association(
            state.path_generation(),
            fixture_path("other.md"),
            &before,
            baseline(b"x\n", 0)?,
        ),
        Err(SaveError::Busy),
        "等待期间目标不得改变"
    );
    state
        .cancel_save_wait(&mut doc, handle)
        .map_err(|error| format!("{error:?}"))?;
    doc.composition_cancel(2)?;
    assert_eq!(state.status(), SaveStatus::Idle);
    assert!(
        matches!(
            state.request_save(&mut doc, TEXT),
            Ok(SaveRequest::Queued(_))
        ),
        "取消等待后协调器应可继续使用"
    );
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
fn persistence_close_blocker_names_the_action_that_unblocks_close() -> TestResult {
    let (_doc, mut state) = fixture(b"x\n")?;
    state.status = SaveStatus::Failed;
    assert_eq!(state.close_blocker(), CloseBlocker::SaveFailed);
    state.status = SaveStatus::Conflict;
    assert_eq!(state.close_blocker(), CloseBlocker::SaveConflict);
    state.status = SaveStatus::PublishedUncertain;
    assert_eq!(state.close_blocker(), CloseBlocker::PublishedUncertain);
    state.status = SaveStatus::Idle;
    assert_eq!(
        state.close_blocker(),
        CloseBlocker::PendingChanges,
        "空闲但仍有未保存编辑时不得误导为保存失败"
    );
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
    editable_absolute_session_with(path, ImageSettings::default())
}

/// 同上，但在打开前注入图片目录设置（打开后不可更改）。
#[cfg(target_os = "macos")]
fn editable_absolute_session_with(
    path: &Path,
    settings: ImageSettings,
) -> Result<ReadonlySession, Box<dyn std::error::Error>> {
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
    session
        .configure_image_settings(settings)
        .map_err(|error| format!("{error:?}"))?;
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
fn persistence_conflict_resave_is_rejected_without_queue_residue() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-conflict-resave")?;
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

    let SaveCommand::Run(job) = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .request_save_command()
    else {
        return Err("首次保存应生成后台工作".into());
    };
    let completion = job.run();
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .settle_save_command(completion),
        SaveResult::Conflict,
        "外部内容变化必须结算为冲突"
    );

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(
        matches!(
            document.request_save_command(),
            SaveCommand::Rejected(SaveRejection::Conflict)
        ),
        "冲突未处理时再次保存必须在入队前按 Conflict 拒绝"
    );
    let snapshot = document.current_snapshot()?;
    assert_eq!(
        document.persistence_close_status(&snapshot),
        Some(CloseStatus::NeedsDecision),
        "拒绝后不得残留 queued，否则关闭决策会退化为 Waiting"
    );
    assert_eq!(
        session.close_document(),
        CloseDisposition::Refused,
        "冲突未处理仍应拒绝关闭"
    );
    assert_eq!(fs::read(&target)?, b"y\n", "再次请求不得覆盖外部内容");
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .current_snapshot()?
            .full_text(TEXT)?,
        "xa\n",
        "拒绝保存不得改变内存文本"
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

/// 以给定字节替换目标，同时保留原 mtime；用于构造外改观察。
#[cfg(target_os = "macos")]
fn rewrite_bytes_same_mtime(target: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::{self, FileTimes, OpenOptions};

    let before = fs::metadata(target)?.modified()?;
    fs::write(target, bytes)?;
    OpenOptions::new()
        .write(true)
        .open(target)?
        .set_times(FileTimes::new().set_modified(before))?;
    assert_eq!(fs::metadata(target)?.modified()?, before);
    Ok(())
}

/// 同步执行一次外改检查并结算；拒绝类别视为测试装配错误。
#[cfg(target_os = "macos")]
fn run_external_check(
    session: &mut ReadonlySession,
    intent: ExternalCheckIntent,
    now_millis: u64,
) -> Result<ExternalApplyResult, Box<dyn std::error::Error>> {
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    let ExternalCheckCommand::Run(job) = document.request_external_check(intent) else {
        return Err("外改检查应生成后台工作".into());
    };
    let completion = job.run();
    let result = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .apply_external_check(completion, now_millis);
    Ok(result)
}

#[cfg(target_os = "macos")]
#[test]
fn external_check_detects_same_size_same_mtime_change_and_reloads_clean_document() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-reload")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    rewrite_same_size_and_mtime(&target)?;

    let completion = {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let ExternalCheckCommand::Run(job) =
            document.request_external_check(ExternalCheckIntent::Focus)
        else {
            return Err("外改检查应生成后台工作".into());
        };
        job.run()
    };
    assert!(
        matches!(
            completion.observation(),
            ExternalObservation::Changed { .. }
        ),
        "同尺寸同时间戳的不同内容必须观察为 Changed"
    );
    let result = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .apply_external_check(completion, 10);
    assert_eq!(result, ExternalApplyResult::Reloaded);

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(
        document.core().buffer_text()?,
        "y\n",
        "干净文档应载入磁盘文本"
    );
    assert!(!document.core().has_open_block(), "重载应关闭打开块");
    assert_eq!(document.baseline().source(), "y\n", "重载应采纳磁盘基线");
    assert!(!document.autosave_paused(), "干净重载不应暂停自动保存");
    assert!(document.external_view().is_none(), "重载后不应残留冲突视图");

    document.edit_core()?.undo()?;
    assert!(document.note_history_move(), "撤销越过重载边界应新进入暂停");
    assert_eq!(
        document.core().buffer_text()?,
        "x\n",
        "撤销应恢复重载前文本"
    );
    assert!(
        document.autosave_paused(),
        "撤销到重载前内容后自动保存必须暂停"
    );

    document.edit_core()?.redo()?;
    assert!(
        !document.note_history_move(),
        "重做回磁盘基线不应再次报告新暂停"
    );
    assert_eq!(document.core().buffer_text()?, "y\n", "重做应回到磁盘文本");
    assert!(!document.autosave_paused(), "干净重做后应恢复自动保存");
    assert!(
        matches!(document.request_save_command(), SaveCommand::Run(_)),
        "冲突清除后显式保存应被允许"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_conflict_preserves_draft_and_rejects_save_as_conflict() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-conflict")?;
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

    let result = run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?;
    assert_eq!(
        result,
        ExternalApplyResult::Presented(ExternalDiskState::Changed)
    );

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    {
        let view = document.external_view().ok_or("应呈现冲突视图")?;
        assert_eq!(view.state, ExternalDiskState::Changed);
        assert_eq!(&*view.baseline, "x\n", "基线预览应为保存基线");
        assert_eq!(&*view.local, "xa\n", "本地预览应含未提交草稿");
        assert_eq!(view.disk.as_deref(), Some("y\n"), "磁盘预览应为新文本");
    }
    assert_eq!(document.core().buffer_text()?, "x\n");
    assert_eq!(document.core().open_block_draft(), Some("a"));
    assert!(document.autosave_paused());
    assert_eq!(document.persistence_status(), Some(SaveStatus::Conflict));
    assert!(
        matches!(
            document.request_save_command(),
            SaveCommand::Rejected(SaveRejection::Conflict)
        ),
        "脏冲突必须按 Conflict 拒绝保存"
    );
    assert_eq!(fs::read(&target)?, b"y\n", "呈现冲突不得改写磁盘");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_reload_with_dirty_document_loads_disk_and_keeps_undo_boundary() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-dirty-reload")?;
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

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed),
        "Focus 在脏文档上只呈现冲突"
    );
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(
            document.core().open_block_draft(),
            Some("a"),
            "呈现冲突必须保留草稿"
        );
        assert_eq!(document.core().buffer_text()?, "x\n");
        assert!(document.autosave_paused(), "冲突呈现应暂停自动保存");
    }

    // 先把草稿提交为本地内容，再点击载入：这样旧文本进入 Buffer 历史边界，
    // 撤销可以取回；未提交草稿本身不在 Buffer 历史中。
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .commit_open_block(15)?;

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Reload, 20)?,
        ExternalApplyResult::Reloaded,
        "脏文档的显式载入必须实际加载磁盘内容"
    );
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(
            document.core().buffer_text()?,
            "y\n",
            "显式载入应替换本地文本"
        );
        assert_eq!(
            document.baseline().source(),
            "y\n",
            "显式载入应采纳新磁盘基线"
        );
        assert!(!document.core().has_open_block(), "重载应关闭打开块");
        assert!(document.external_view().is_none(), "载入后不应残留冲突视图");
        assert!(!document.autosave_paused(), "载入完成后不应处于暂停");
        assert_eq!(
            document.persistence_status(),
            Some(SaveStatus::Idle),
            "载入完成后保存状态应回到 Idle"
        );
        let snapshot = document.current_snapshot()?;
        assert_eq!(
            document.persistence_is_dirty(&snapshot),
            Some(false),
            "载入后应相对新磁盘基线为干净"
        );

        document.edit_core()?.undo()?;
        assert!(document.note_history_move(), "撤销越过载入边界应新进入暂停");
        assert_eq!(
            document.core().buffer_text()?,
            "xa\n",
            "撤销应取回载入前的本地内容"
        );
        assert!(
            document.autosave_paused(),
            "撤销到载入前内容后必须暂停自动保存"
        );

        document.edit_core()?.redo()?;
        assert!(
            !document.note_history_move(),
            "重做回新基线不应再次报告暂停"
        );
        assert_eq!(document.core().buffer_text()?, "y\n");
        assert!(
            !document.autosave_paused(),
            "重做回磁盘文本后应恢复自动保存"
        );
        let snapshot = document.current_snapshot()?;
        assert_eq!(
            document.persistence_is_dirty(&snapshot),
            Some(false),
            "重做后应相对新磁盘基线为干净"
        );
    }
    assert_eq!(fs::read(&target)?, b"y\n", "载入磁盘不得改写磁盘");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_overwrite_adopts_disk_baseline_then_saves_local_text() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-overwrite")?;
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
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed)
    );

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Overwrite, 11)?,
        ExternalApplyResult::OverwriteReady
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(
        document.baseline().source(),
        "y\n",
        "覆盖确认应采纳磁盘基线"
    );
    let SaveCommand::Run(job) = document.request_save_command() else {
        return Err("覆盖确认后显式保存应生成后台工作".into());
    };
    let completion = job.run();
    let result = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save_command(completion);
    assert_eq!(result, SaveResult::Saved, "覆盖确认后应成功发布本地文本");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(fs::read(&target)?, b"xa\n", "发布后磁盘应等于本地全文");
    assert_eq!(document.baseline().source(), "xa\n");
    assert!(!document.autosave_paused());
    assert!(document.external_view().is_none());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_overwrite_conflicts_again_when_disk_changes_before_publish() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-overwrite-race")?;
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
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed)
    );
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Overwrite, 11)?,
        ExternalApplyResult::OverwriteReady
    );
    rewrite_bytes_same_mtime(&target, b"z\n")?;

    let SaveCommand::Run(job) = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .request_save_command()
    else {
        return Err("覆盖确认后显式保存应生成后台工作".into());
    };
    let completion = job.run();
    let result = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save_command(completion);
    assert_eq!(result, SaveResult::Conflict, "发布前磁盘再变化必须再次冲突");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(fs::read(&target)?, b"z\n", "冲突时不得覆盖磁盘");
    assert_eq!(
        document.current_snapshot()?.full_text(TEXT)?,
        "xa\n",
        "冲突时内存全文必须保留"
    );
    assert_eq!(document.persistence_status(), Some(SaveStatus::Conflict));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_missing_target_keeps_memory_and_offers_retry_without_overwrite() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-missing")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    fs::remove_file(&target)?;

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Missing)
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    {
        let view = document.external_view().ok_or("应呈现缺失视图")?;
        assert_eq!(view.state, ExternalDiskState::Missing);
        assert!(view.disk.is_none(), "缺失目标不得提供磁盘预览");
        assert_eq!(&*view.baseline, "x\n", "基线仍是保存基线");
    }
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, "xa\n");
    assert!(document.autosave_paused());

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Overwrite, 11)?,
        ExternalApplyResult::Presented(ExternalDiskState::Missing),
        "缺失目标不能作为可采纳基线"
    );
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .baseline()
            .source(),
        "x\n"
    );
    assert!(
        matches!(
            session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .request_save_command(),
            SaveCommand::Rejected(SaveRejection::Conflict)
        ),
        "缺失目标下的脏文档必须按 Conflict 拒绝保存"
    );

    fs::write(&target, b"x\n")?;
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Retry, 12)?,
        ExternalApplyResult::Unchanged
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(document.external_view().is_none());
    assert!(!document.autosave_paused());
    assert_eq!(document.persistence_status(), Some(SaveStatus::Idle));
    assert!(
        matches!(document.request_save_command(), SaveCommand::Run(_)),
        "磁盘恢复基线后应允许显式保存"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_unsupported_encoding_is_presented_without_disk_text() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-unsupported")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    rewrite_bytes_same_mtime(&target, &[0xFF, b'\n'])?;

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Unsupported)
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    {
        let view = document.external_view().ok_or("应呈现不支持视图")?;
        assert_eq!(view.state, ExternalDiskState::Unsupported);
        assert!(view.disk.is_none(), "不可解码时不得提供磁盘预览");
    }
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, "xa\n");
    assert_eq!(document.baseline().source(), "x\n");
    assert!(
        matches!(
            document.request_save_command(),
            SaveCommand::Rejected(SaveRejection::Conflict)
        ),
        "不可解码冲突必须按 Conflict 拒绝保存"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_directory_target_is_presented_without_disk_text() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-directory")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    fs::remove_file(&target)?;
    fs::create_dir(&target)?;

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Unsupported)
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, "xa\n");
    {
        let view = document.external_view().ok_or("应呈现目录视图")?;
        assert_eq!(view.state, ExternalDiskState::Unsupported);
        assert!(view.disk.is_none(), "目录目标不得提供磁盘预览");
    }

    fs::remove_dir(&target)?;
    fs::write(&target, b"x\n")?;
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Retry, 11)?,
        ExternalApplyResult::Unchanged
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(document.external_view().is_none());
    assert!(!document.autosave_paused());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_unreadable_target_is_presented_without_disk_text() -> TestResult {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let directory = TempSaveDir::create("aline-external-unreadable")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o000))?;

    let result = run_external_check(&mut session, ExternalCheckIntent::Focus, 10);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
    assert_eq!(
        result?,
        ExternalApplyResult::Presented(ExternalDiskState::Unreadable)
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    {
        let view = document.external_view().ok_or("应呈现不可读视图")?;
        assert_eq!(view.state, ExternalDiskState::Unreadable);
        assert!(view.disk.is_none(), "不可读目标不得提供磁盘预览");
    }
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, "xa\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_check_is_deferred_during_composition_without_side_effects() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-composition")?;
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

    let job = {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let ExternalCheckCommand::Run(job) =
            document.request_external_check(ExternalCheckIntent::Reload)
        else {
            return Err("组合前外改检查应生成后台工作".into());
        };
        job
    };
    let completion = job.run();
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.edit_core()?.composition_start("候", 2)?;
    }
    let (version, selection, depth, draft, buffer) = {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        (
            document.core().version(),
            document.core().current_selection(),
            document.core().undo_depth(),
            document.core().open_block_draft().map(str::to_owned),
            document.core().buffer_text()?,
        )
    };
    for intent in [ExternalCheckIntent::Focus, ExternalCheckIntent::Reload] {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert!(
            matches!(
                document.request_external_check(intent),
                ExternalCheckCommand::Rejected(ExternalCheckRejection::Deferred)
            ),
            "组合活动时请求必须 Deferred"
        );
    }
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .apply_external_check(completion, 20),
        ExternalApplyResult::Deferred
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.core().version(), version, "Deferred 不得改变版本");
    assert_eq!(
        document.core().current_selection(),
        selection,
        "Deferred 不得改变选区"
    );
    assert_eq!(document.core().undo_depth(), depth, "Deferred 不得改变历史");
    assert_eq!(
        document.core().open_block_draft().map(str::to_owned),
        draft,
        "Deferred 不得改变草稿"
    );
    assert_eq!(document.core().buffer_text()?, buffer);
    assert!(document.external_view().is_none(), "Deferred 不得呈现冲突");
    assert!(!document.autosave_paused(), "Deferred 不得设置暂停");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_dismiss_keeps_conflict_and_pause_until_disk_matches_baseline() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-external-dismiss")?;
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
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed)
    );

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.dismiss_external();
    assert!(document.external_view().is_none(), "Dismiss 只清视图");
    assert!(document.autosave_paused(), "Dismiss 不清暂停");
    assert!(
        matches!(
            document.request_save_command(),
            SaveCommand::Rejected(SaveRejection::Conflict)
        ),
        "Dismiss 不清冲突状态"
    );

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 11)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed),
        "再次检查应重新呈现仍存在的冲突"
    );
    fs::write(&target, b"x\n")?;
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 12)?,
        ExternalApplyResult::Unchanged
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(document.external_view().is_none());
    assert!(!document.autosave_paused());
    assert_eq!(document.persistence_status(), Some(SaveStatus::Idle));
    assert!(matches!(
        document.request_save_command(),
        SaveCommand::Run(_)
    ));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn external_check_completion_is_send() {
    fn assert_send<T: Send>() {}

    assert_send::<ExternalCheckJob>();
    assert_send::<ExternalCheckCompletion>();
}

/// 打开真实可编辑会话并制造一次本地修改；供自动保存计时用例复用。
#[cfg(target_os = "macos")]
fn dirty_editable_session(
    prefix: &str,
) -> Result<(TempSaveDir, PathBuf, ReadonlySession), Box<dyn std::error::Error>> {
    use std::fs;

    let directory = TempSaveDir::create(prefix)?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 0)?;
    Ok((directory, target, session))
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_fires_exactly_at_one_second() -> TestResult {
    let (_directory, _target, mut session) = dirty_editable_session("aline-autosave-boundary")?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(0);
    assert_eq!(
        document.autosave_poll(999),
        AutosaveAction::NotDue {
            remaining_millis: 1
        },
        "999ms 时不应到期"
    );
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::Fire,
        "1000ms 必须到期"
    );
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::Idle,
        "Fire 必须一次性消费到期时刻"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_repeated_events_reset_the_deadline() -> TestResult {
    let (_directory, _target, mut session) = dirty_editable_session("aline-autosave-reset")?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(0);
    document.note_document_activity(500);
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::NotDue {
            remaining_millis: 500
        },
        "重复活动应把到期时刻重置到 1500ms"
    );
    assert_eq!(
        document.autosave_poll(1499),
        AutosaveAction::NotDue {
            remaining_millis: 1
        }
    );
    assert_eq!(document.autosave_poll(1500), AutosaveAction::Fire);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_defers_through_composition_and_rearms_after_it_ends() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-autosave-composition")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_start("候", 0)?;

    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.note_document_activity(0);
        assert_eq!(
            document.autosave_poll(1000),
            AutosaveAction::DeferredComposition,
            "组合活动期间不得自动保存"
        );
    }
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_commit(1500)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(1500);
    assert_eq!(
        document.autosave_poll(2499),
        AutosaveAction::NotDue {
            remaining_millis: 1
        },
        "组合结束后的结算应从 1500ms 重新布防"
    );
    assert_eq!(document.autosave_poll(2500), AutosaveAction::Fire);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_skips_clean_document() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-autosave-clean")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(0);
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::NotDirty,
        "干净文档不得触发自动保存"
    );
    assert_eq!(
        document.autosave_deadline(),
        None,
        "NotDirty 后不应保留到期时刻"
    );
    Ok(())
}

#[test]
fn autosave_never_selects_a_path_without_target() -> TestResult {
    use std::io::{self, Cursor};

    use crate::{
        OpenDisposition, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits, ReadonlySession,
    };
    use aline_markdown::{ParseBudget, ParseConfig};

    struct RelativeFiles;
    impl ReadonlyFileAccess for RelativeFiles {
        type Reader = Cursor<&'static [u8]>;
        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(Cursor::new(b"x\n"), 2, true))
        }
    }

    let limits = ReadonlyOpenLimits::new(
        STORAGE,
        DocLimits::new(
            TEXT,
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
    .with_editing();
    let relative = PathBuf::from(format!(
        "aline-autosave-no-target-{}.md",
        std::process::id()
    ));
    assert!(!relative.exists(), "测试前置：相对目标不应存在");
    let mut session = ReadonlySession::new(limits);
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![relative.clone()]))?
        .ok_or("应生成打开工作")?;
    assert!(matches!(
        session.accept_open(job.read_with(&RelativeFiles)),
        OpenDisposition::Applied
    ));

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(document.is_editable(), "相对路径仍应以可编辑文档打开");
    assert_eq!(
        document.persistence_status(),
        None,
        "相对路径不得建立写目标"
    );
    document.note_document_activity(0);
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::NoTarget,
        "无持久化目标必须拒绝自动保存"
    );
    assert!(
        matches!(
            document.request_save_command(),
            SaveCommand::Rejected(SaveRejection::NoTarget)
        ),
        "未命名文档不得创建保存请求"
    );
    assert_eq!(document.path(), relative.as_path());
    assert!(!relative.exists(), "自动保存绝不选择路径");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_pauses_for_external_conflict_and_resumes_after_load_disk() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-autosave-conflict")?;
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
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Presented(ExternalDiskState::Changed)
    );
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.note_document_activity(0);
        assert_eq!(
            document.autosave_poll(1000),
            AutosaveAction::Paused,
            "外改冲突必须暂停自动保存"
        );
    }

    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Reload, 20)?,
        ExternalApplyResult::Reloaded
    );
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(
            document.autosave_poll(1000),
            AutosaveAction::NotDirty,
            "载入磁盘后已干净，到期结算应为 NotDirty"
        );
        assert_eq!(document.autosave_deadline(), None);
    }
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 2000)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(2000);
    assert_eq!(
        document.autosave_poll(3000),
        AutosaveAction::Fire,
        "暂停解除后的新输入应重新布防"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_pauses_after_reload_undo_until_redo_to_baseline() -> TestResult {
    use std::fs;

    let directory = TempSaveDir::create("aline-autosave-reload-undo")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut session = editable_absolute_session(&target)?;
    rewrite_same_size_and_mtime(&target)?;
    assert_eq!(
        run_external_check(&mut session, ExternalCheckIntent::Focus, 10)?,
        ExternalApplyResult::Reloaded,
        "干净文档的外改应自动重载"
    );

    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.edit_core()?.undo()?;
        assert!(document.note_history_move(), "撤销越过重载边界应进入暂停");
        document.note_document_activity(0);
        assert_eq!(
            document.autosave_poll(1000),
            AutosaveAction::Paused,
            "重载撤销暂停期间不得自动保存"
        );
    }

    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.edit_core()?.redo()?;
        assert!(!document.note_history_move(), "重做回磁盘基线应解除暂停");
        document.note_document_activity(2000);
        assert_eq!(
            document.autosave_poll(3000),
            AutosaveAction::NotDirty,
            "重做回基线后到期结算应为 NotDirty"
        );
    }
    open_editing_block(&mut session)?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(1, "a", 4000)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(4000);
    assert_eq!(document.autosave_poll(5000), AutosaveAction::Fire);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_reports_busy_while_a_save_is_active() -> TestResult {
    let (_directory, _target, mut session) = dirty_editable_session("aline-autosave-busy")?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    let SaveCommand::Run(job) = document.request_save_command() else {
        return Err("脏文档的自动保存应生成后台工作".into());
    };
    document.note_document_activity(0);
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::Busy,
        "在途保存期间不得再次自动保存"
    );
    let completion = job.run();
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .settle_save_command(completion),
        SaveResult::Saved,
        "结算后应回到成功状态"
    );
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.persistence_status(), Some(SaveStatus::Idle));
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::NotDirty,
        "保存成功后保留的到期时刻应按干净文档消费"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_end_to_end_saves_once_after_debounce() -> TestResult {
    use std::fs;

    let (_directory, target, mut session) = dirty_editable_session("aline-autosave-e2e")?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(0);
    assert_eq!(document.autosave_poll(1000), AutosaveAction::Fire);
    let SaveCommand::Run(job) = document.request_save_command() else {
        return Err("到期后应生成后台工作".into());
    };
    let completion = job.run();
    assert_eq!(
        document.settle_save_command(completion),
        SaveResult::Saved,
        "自动保存应走同一协调器并成功发布"
    );
    assert_eq!(fs::read(&target)?, b"xa\n", "磁盘字节应更新");
    assert_eq!(
        document.autosave_poll(1000),
        AutosaveAction::Idle,
        "保存完成后没有新的待办计时"
    );
    assert!(!document.autosave_paused());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn autosave_cancel_clears_pending_deadline() -> TestResult {
    let (_directory, _target, mut session) = dirty_editable_session("aline-autosave-cancel")?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.note_document_activity(0);
    document.cancel_autosave();
    assert_eq!(document.autosave_deadline(), None);
    assert_eq!(document.autosave_poll(1000), AutosaveAction::Idle);
    Ok(())
}

// ---- 4.2a 首次保存/另存为关联（临时资源迁移 + 写盘） ----

/// 固定字节读取器：只用于无窗口的未命名文档打开，不访问真实文件。
#[cfg(target_os = "macos")]
struct BytesFiles(Vec<u8>);

#[cfg(target_os = "macos")]
impl ReadonlyFileAccess for BytesFiles {
    type Reader = std::io::Cursor<Vec<u8>>;

    fn open(&self, _path: &Path) -> std::io::Result<ReadonlyFileInput<Self::Reader>> {
        let len = u64::try_from(self.0.len()).unwrap_or(u64::MAX);
        Ok(ReadonlyFileInput::new(
            std::io::Cursor::new(self.0.clone()),
            len,
            true,
        ))
    }
}

/// 打开未命名（相对路径、无保存协调器）的可编辑会话；可选注入受检临时根。
#[cfg(target_os = "macos")]
fn untitled_editable_session(
    root: Option<TempRoot>,
) -> Result<ReadonlySession, Box<dyn std::error::Error>> {
    untitled_editable_session_with(b"x\n", root)
}

/// 与 `untitled_editable_session` 相同，但使用调用方给定的初始 bytes。
#[cfg(target_os = "macos")]
fn untitled_editable_session_with(
    source: &[u8],
    root: Option<TempRoot>,
) -> Result<ReadonlySession, Box<dyn std::error::Error>> {
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
    session.configure_temp_root(root);
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("untitled.md")]))?
        .ok_or("应生成打开工作")?;
    assert!(
        matches!(
            session.accept_open(job.read_with(&BytesFiles(source.to_vec()))),
            OpenDisposition::Applied
        ),
        "未命名可编辑文档应发布"
    );
    Ok(session)
}

/// 在系统临时目录下建立测试 HOME 并打开 `Aline/temporary` 受检临时根。
#[cfg(target_os = "macos")]
fn temp_resource_root(home: &Path) -> Result<TempRoot, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(home.join("Library/Application Support"))?;
    Ok(TempRoot::open_or_create(home).map_err(|error| format!("{error:?}"))?)
}

#[cfg(target_os = "macos")]
fn stage_image(
    session: &mut ReadonlySession,
    name: &str,
    bytes: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let len = document.current_snapshot()?.full_text(TEXT)?.len();
        document
            .edit_core()?
            .open_block(0..len, SelectionSpec::new(0, 0))?;
    }
    let reference = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .stage_temp_image(name, bytes, 4096)
        .map_err(|error| format!("{error:?}"))?;
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .leave_block(1)?;
    Ok(reference.locator().to_owned())
}

/// 在文档末尾（换行之前）插入文本；块保持打开，后续关联命令在同一草稿上继续。
#[cfg(target_os = "macos")]
fn append_text(
    session: &mut ReadonlySession,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    let source = document.current_snapshot()?.full_text(TEXT)?;
    let len = source.len();
    let at = if source.ends_with('\n') { len - 1 } else { len };
    let core = document.edit_core()?;
    core.open_block(0..len, SelectionSpec::new(0, 0))?;
    core.type_text(at, text, 0)?;
    core.leave_block(1)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn current_text(session: &mut ReadonlySession) -> Result<String, Box<dyn std::error::Error>> {
    Ok(session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .current_snapshot()?
        .full_text(TEXT)?)
}

/// 开始一次关联并同步执行复制阶段，返回迁移完成项。
#[cfg(target_os = "macos")]
fn association_copy(
    session: &mut ReadonlySession,
    kind: WriteKind,
    target: &Path,
) -> Result<MigrationCompletion, Box<dyn std::error::Error>> {
    let command = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .begin_association_save(kind, target.to_path_buf(), 0);
    match command {
        AssociationCommand::Run(job) => Ok(job.run()),
        AssociationCommand::Save(_) => Err("该目标应进入关联迁移而非直接保存".into()),
        AssociationCommand::Wait(_) => Err("无组合输入时应直接复制".into()),
        AssociationCommand::Rejected(rejection) => Err(format!("关联被拒: {rejection:?}").into()),
    }
}

/// 复核迁移完成项并按需重写引用，返回已冻结的后台写盘工作。
#[cfg(target_os = "macos")]
fn association_write_job(
    session: &mut ReadonlySession,
    completion: MigrationCompletion,
) -> Result<SaveJob, Box<dyn std::error::Error>> {
    match session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .association_prepared(completion, 1)
    {
        AssociationStep::Write(job) => Ok(job),
        AssociationStep::Run(_) => Err("复制完成项不应再次要求复制".into()),
        AssociationStep::Settled(outcome) => Err(format!("关联未进入写盘: {outcome:?}").into()),
    }
}

#[cfg(target_os = "macos")]
fn run_association_write(
    session: &mut ReadonlySession,
    job: SaveJob,
) -> Result<WriteSettlement, Box<dyn std::error::Error>> {
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save(job.run())
        .map_err(|error| format!("{error:?}").into())
}

/// 统计临时根下普通文件数；用于确认失败后材料仍在。
#[cfg(target_os = "macos")]
fn resource_file_count(root: &Path) -> Result<usize, Box<dyn std::error::Error>> {
    fn walk(path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        let mut count = 0;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                count += walk(&entry.path())?;
            } else if metadata.is_file() {
                count += 1;
            }
        }
        Ok(count)
    }
    walk(root)
}

#[cfg(target_os = "macos")]
#[test]
fn untitled_first_save_associates_path_only_after_write() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-first")?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(None)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    assert!(!target.exists(), "写盘完成前目标文件不得出现");
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert_eq!(
        settlement.association.as_ref().map(|(path, _)| path),
        Some(&target)
    );
    assert_eq!(std::fs::read(&target)?, b"x\n");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.path(), target, "成功后路径应改为关联目标");
    assert_eq!(document.persistence_is_creating(), Some(false));
    let snapshot = document.current_snapshot()?;
    assert_eq!(document.persistence_is_dirty(&snapshot), Some(false));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_platform_cancelled_composition_continues_original_save_intent() -> TestResult {
    for kind in [WriteKind::Create, WriteKind::SaveAs] {
        let directory = TempSaveDir::create("aline-assoc-ime-cancel")?;
        let original = directory.0.join("original.md");
        let target = directory.0.join("out/notes.md");
        std::fs::create_dir_all(target.parent().ok_or("parent")?)?;
        let mut session = if kind == WriteKind::Create {
            untitled_editable_session(None)?
        } else {
            std::fs::write(&original, b"x\n")?;
            editable_absolute_session(&original)?
        };
        let document = session.document_mut().ok_or("document")?;
        document
            .edit_core()?
            .open_block(0..2, SelectionSpec::new(1, 1))?;
        document.edit_core()?.composition_start("ni", 0)?;
        let AssociationCommand::Wait(handle) =
            document.begin_association_save(kind, target.clone(), 0)
        else {
            return Err("组合期关联应等待".into());
        };
        document.edit_core()?.composition_cancel(1)?;
        let AssociationCommand::Run(copy) =
            document.finish_association_wait(handle, PublicationOutcome::PlatformCancelled, 2)
        else {
            return Err("平台取消组合后必须继续原保存意图".into());
        };
        let job = association_write_job(&mut session, copy.run())?;
        assert!(run_association_write(&mut session, job)?.adopted);
        assert_eq!(std::fs::read(&target)?, b"x\n");
        assert_eq!(session.document().ok_or("document")?.core().undo_depth(), 0);
        assert_eq!(session.document().ok_or("document")?.path(), target);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn create_cancelled_after_staging_sync_never_publishes_target() -> TestResult {
    let directory = TempSaveDir::create("aline-create-staging-cancel")?;
    let target = directory.0.join("notes.md");
    let (mut core, _) = fixture(b"x\n")?;
    let mut state = PersistenceCoordinator::new_untitled(
        core.document_id().clone(),
        target.clone(),
        baseline(b"x\n", 0)?,
    )
    .map_err(|error| format!("{error:?}"))?;
    queue(&mut state, &mut core)?;
    let frozen = start(&mut state)?;
    let cancellation = frozen.cancellation().clone();
    let mut staged_path = None;
    let completion = super::execute::run_create_using(frozen, TEXT, STORAGE, |parent, bytes| {
        let staged = aline_source::fs::create_staging(parent, ".aline-test-cancel", bytes)?;
        staged_path = Some(staged.path().to_path_buf());
        cancellation.cancel();
        Ok(staged)
    });
    assert!(matches!(
        completion.outcome,
        WriteOutcome::NotPublished(WriteStop::Cancelled)
    ));
    assert!(!target.exists(), "同步 staging 后取消不得发布用户目标");
    assert!(
        !staged_path.ok_or("实际staging路径")?.exists(),
        "未发布staging应按身份清理"
    );
    let settlement = state.reconcile(completion);
    assert!(!settlement.adopted);
    assert_eq!(core.undo_depth(), 0);
    assert_eq!(core.buffer_text()?, "x\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn untitled_first_save_failure_keeps_relative_path_and_materials() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempSaveDir::create("aline-assoc-fail")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let mut session = untitled_editable_session(Some(root.clone()))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let text_before = current_text(&mut session)?;

    let locked = directory.0.join("locked");
    std::fs::create_dir(&locked)?;
    let target = locked.join("notes.md");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500))?;
    let completion = association_copy(&mut session, WriteKind::Create, &target);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))?;
    let completion = completion?;
    assert!(completion.result.is_err(), "只读父目录上的迁移必须失败");
    match session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .association_prepared(completion, 1)
    {
        AssociationStep::Settled(AssociationOutcome::Failed) => {}
        _ => return Err("迁移失败应结算为 Failed".into()),
    }

    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.path(), Path::new("untitled.md"));
    assert_eq!(current_text(&mut session)?, text_before);
    assert_eq!(
        resource_file_count(root.path())?,
        1,
        "失败后临时资源必须保留"
    );
    assert!(!target.exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn first_save_continue_typing_writes_latest_text() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-typing")?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(None)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    append_text(&mut session, "B")?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert_eq!(std::fs::read(&target)?, b"xB\n");
    assert_eq!(current_text(&mut session)?, "xB\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn first_save_new_resource_during_preparation_requests_retry() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-new-resource")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let first = stage_image(&mut session, "first.png", b"first")?;
    append_text(&mut session, &first)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    // 复制完成后、复核前新增一张被文本引用的图片，必须要求重试。
    let second = stage_image(&mut session, "second.png", b"second")?;
    append_text(&mut session, &second)?;
    match session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .association_prepared(completion, 1)
    {
        AssociationStep::Settled(AssociationOutcome::NeedsRetry) => {}
        _ => return Err("新增引用应要求重试".into()),
    }
    assert!(!target.exists(), "要求重试时不得写盘");
    Ok(())
}

/// 写盘失败（非冲突）后瞬时 association 清除、pending_migration 保留；
/// `retry_association` 直接重写盘且不再重搬资产（W4-63）。
#[cfg(target_os = "macos")]
#[test]
fn first_save_write_failure_keeps_pending_migration_for_retry() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempSaveDir::create("aline-assoc-retry")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    let out = target.parent().ok_or("target parent")?;
    std::fs::create_dir_all(out)?;
    let mut session = untitled_editable_session(Some(root.clone()))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let before = current_text(&mut session)?;
    let (version, selection, undo, redo) = {
        let core = session.document().ok_or("应存在可编辑文档")?.core();
        (
            core.version(),
            core.current_selection(),
            core.undo_depth(),
            core.redo_depth(),
        )
    };

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    assert_eq!(
        current_text(&mut session)?,
        before,
        "写盘前引用候选不得发布"
    );
    // 迁移已把资产复制到目标目录；把目录设为只读，令真正写盘失败。
    std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o500))?;
    let completion = job.run();
    std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o700))?;
    let settlement = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .settle_save(completion)
        .map_err(|error| format!("{error:?}"))?;
    assert!(settlement.adopted);
    assert!(matches!(settlement.outcome, WriteOutcome::NotPublished(_)));
    assert!(!target.exists(), "写盘失败不得留下目标文件");
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(document.path(), Path::new("untitled.md"));
        assert_eq!(document.close_status(), CloseStatus::NeedsDecision);
        let core = document.core();
        assert_eq!(core.version(), version, "写盘失败不得提交引用或推进版本");
        assert_eq!(core.current_selection(), selection);
        assert_eq!((core.undo_depth(), core.redo_depth()), (undo, redo));
    }
    assert_eq!(
        current_text(&mut session)?,
        before,
        "失败后临时 locator 必须仍可恢复"
    );
    {
        use crate::document::recovery::{
            RecoveryConfig, RecoveryResourceSource, RecoveryRoot, RecoveryStart, RecoveryStore,
        };
        use crate::document::resources::TempResourceSource;
        use std::sync::Arc;

        let recovery_dir = directory.0.join("recovery");
        std::fs::create_dir(&recovery_dir)?;
        std::fs::set_permissions(&recovery_dir, std::fs::Permissions::from_mode(0o700))?;
        let store = RecoveryStore::open(
            RecoveryRoot::open(&recovery_dir).map_err(|error| format!("{error:?}"))?,
        )
        .map_err(|error| format!("{error:?}"))?;
        let source = TempResourceSource::new(root.clone());
        assert_eq!(
            source
                .read(&locator, 4096)
                .map_err(|error| format!("{error:?}"))?,
            b"png"
        );
        let document = session.document_mut().ok_or("document")?;
        document.attach_recovery(RecoveryConfig {
            store: store.clone(),
            resources: Arc::new(source),
        });
        document.recovery_arm(0);
        assert_eq!(document.recovery_deadline(), Some(10_000));
        let Some(RecoveryStart::Prepare(job)) = document.recovery_start(10_000) else {
            return Err("失败后10秒恢复应冻结原临时引用".into());
        };
        let prepared = job.run();
        let Some(RecoveryStart::Publish(job)) = document.recovery_prepared(prepared, 10_000) else {
            return Err("临时图片可读时恢复应完整发布".into());
        };
        let settlement = job.run();
        assert!(settlement.published.is_some());
        document.recovery_settled(settlement, 10_000);
        let scan = store.scan().map_err(|error| format!("{error:?}"))?;
        let lineage = scan
            .recoverable_lineages()
            .first()
            .copied()
            .ok_or("恢复谱系")?;
        let generation = lineage.latest_complete().ok_or("完整恢复代")?.generation;
        let restored = store
            .recover(lineage.document, generation)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(restored.source, before, "恢复不得引用尚未关联的新目录");
        assert_eq!(restored.resources.len(), 1);
        assert_eq!(restored.resources[0].reference, locator);
        assert_eq!(restored.resources[0].bytes, b"png");
    }

    let retry = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .retry_association()
        .map_err(|rejection| format!("重试被拒: {rejection:?}"))?;
    let job = match retry {
        AssociationStep::Write(job) => job,
        _ => return Err("引用已重写后重试应直接写盘".into()),
    };
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert_eq!(std::fs::read(&target)?, b"x./assets/pic.png\n");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.path(), target);
    assert_eq!(document.close_status(), CloseStatus::Ready);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_normalized_bytes_keep_logical_saved_baseline() -> TestResult {
    for (raw, expected) in [
        (b"a\r\nb\nc\r\n".as_slice(), b"aB\r\nb\r\nc\r\n".as_slice()),
        (
            b"\xef\xbb\xbfa\r\nb\nc\r\n".as_slice(),
            b"\xef\xbb\xbfaB\r\nb\r\nc\r\n".as_slice(),
        ),
        (b"a\r\nb\rc\n".as_slice(), b"aB\nb\rc\n".as_slice()),
    ] {
        for kind in [WriteKind::Create, WriteKind::SaveAs] {
            let directory = TempSaveDir::create("aline-assoc-logical")?;
            let original = directory.0.join("original.md");
            let target = directory.0.join("out/notes.md");
            std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
            let mut session = if kind == WriteKind::Create {
                untitled_editable_session_with(raw, None)?
            } else {
                std::fs::write(&original, raw)?;
                editable_absolute_session(&original)?
            };
            {
                let document = session.document_mut().ok_or("应存在可编辑文档")?;
                let len = document.current_snapshot()?.len_bytes();
                let core = document.edit_core()?;
                core.open_block(0..len, SelectionSpec::new(1, 1))?;
                core.type_text(1, "B", 0)?;
                core.leave_block(1)?;
            }
            let logical = current_text(&mut session)?;
            let completion = association_copy(&mut session, kind, &target)?;
            let job = association_write_job(&mut session, completion)?;
            let settlement = run_association_write(&mut session, job)?;
            assert!(settlement.adopted);
            assert_eq!(std::fs::read(&target)?, expected);
            let document = session.document_mut().ok_or("应存在可编辑文档")?;
            let current = document.current_snapshot()?;
            assert_eq!(current.full_text(TEXT)?, logical, "编码不得修改源码行尾");
            assert_eq!(
                document.persistence_is_dirty(&current),
                Some(false),
                "成功另存的逻辑输入应干净"
            );
            assert_eq!(document.close_status(), CloseStatus::Ready);
            document.edit_core()?.undo()?;
            let undone = document.current_snapshot()?;
            assert_eq!(document.persistence_is_dirty(&undone), Some(true));
            document.edit_core()?.redo()?;
            let redone = document.current_snapshot()?;
            assert_eq!(document.persistence_is_dirty(&redone), Some(false));
            assert_eq!(document.close_status(), CloseStatus::Ready);
            assert_eq!(std::fs::read(&target)?, expected, "Undo/Redo 不写盘");
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn first_save_undoes_reference_rewrite_without_deleting_temp() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-undo")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root.clone()))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    let rewritten = current_text(&mut session)?;
    assert!(rewritten.contains("./assets/pic.png"));
    assert!(!rewritten.contains(&locator));

    {
        let core = session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .edit_core()?;
        core.undo()?;
    }
    assert!(
        current_text(&mut session)?.contains(&locator),
        "Undo 应回到临时 locator"
    );
    assert_eq!(resource_file_count(root.path())?, 1, "临时文件不得被删除");
    {
        let core = session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .edit_core()?;
        core.redo()?;
    }
    assert!(
        current_text(&mut session)?.contains("./assets/pic.png"),
        "Redo 应回到新引用"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn first_save_reference_candidate_keeps_pending_typing_until_success() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-pending")?;
    let root = temp_resource_root(&directory.0.join("home"))?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let before_buffer;
    {
        let document = session.document_mut().ok_or("document")?;
        let len = document.current_snapshot()?.len_bytes();
        let core = document.edit_core()?;
        core.open_block(0..len, SelectionSpec::new(1, 1))?;
        before_buffer = core.buffer_text()?;
        core.type_text(1, "B", 20)?;
    }
    let before = current_text(&mut session)?;
    let depth = session.document().ok_or("document")?.core().undo_depth();
    let copy = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, copy)?;
    assert_eq!(
        session.document().ok_or("document")?.core().buffer_text()?,
        before_buffer
    );
    assert_eq!(current_text(&mut session)?, before);
    assert_eq!(
        session.document().ok_or("document")?.core().undo_depth(),
        depth
    );
    assert!(run_association_write(&mut session, job)?.adopted);
    assert_eq!(current_text(&mut session)?, "xB./assets/pic.png\n");
    let core = session.document_mut().ok_or("document")?.edit_core()?;
    core.undo()?;
    assert_eq!(core.buffer_text()?, before, "首步Undo只撤系统引用补丁");
    core.undo()?;
    assert_eq!(core.buffer_text()?, before_buffer, "第二步Undo撤普通输入组");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn visual_first_save_with_two_images_rewrites_in_source_context() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-visual-multi")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session_with(b"body\n", Some(root))?;
    let first = stage_image(&mut session, "first.png", b"first")?;
    append_text(&mut session, &first)?;
    let second = stage_image(&mut session, "second.png", b"second")?;
    append_text(&mut session, &second)?;
    assert_eq!(
        session.document().ok_or("应存在可编辑文档")?.core().mode(),
        EditorMode::Visual,
        "默认视图应为 Visual"
    );

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    let document = session.document().ok_or("应存在可编辑文档")?;
    assert_eq!(
        document.core().mode(),
        EditorMode::Visual,
        "系统迁移不得改变用户视图"
    );
    let written = std::fs::read_to_string(&target)?;
    assert!(written.contains("./assets/first.png"));
    assert!(written.contains("./assets/second.png"));
    assert!(!written.contains(&first));
    assert!(!written.contains(&second));
    let out = target.parent().ok_or("target parent")?;
    assert!(out.join("assets/first.png").exists());
    assert!(out.join("assets/second.png").exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn visual_first_save_rewrites_front_matter_image_reference() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-visual-front")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session_with(b"---\nt: x\n---\n\nbody\n", Some(root))?;
    let locator = stage_image(&mut session, "front.png", b"front")?;
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        document.switch_mode(EditorMode::Source)?;
        let len = document.current_snapshot()?.full_text(TEXT)?.len();
        let core = document.edit_core()?;
        core.open_block(0..len, SelectionSpec::new(7, 7))?;
        core.type_text(7, &locator, 1)?;
        core.leave_block(2)?;
        document.switch_mode(EditorMode::Visual)?;
    }

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    let written = std::fs::read_to_string(&target)?;
    assert!(written.contains("./assets/front.png"));
    assert!(!written.contains(&locator));
    assert!(
        target
            .parent()
            .ok_or("target parent")?
            .join("assets/front.png")
            .exists(),
        "成功迁移必须保留资产目录与文件"
    );
    assert_eq!(
        session.document().ok_or("应存在可编辑文档")?.core().mode(),
        EditorMode::Visual
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_rewrite_apply_failure_rolls_back_new_assets_and_allows_retry() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-rollback")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    let out = target.parent().ok_or("target parent")?;
    std::fs::create_dir_all(out)?;
    let mut session = untitled_editable_session_with(b"body\n", Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let text_before = session
        .document()
        .ok_or("应存在可编辑文档")?
        .core()
        .buffer_text()?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    // 复制已成功；开启组合使应用阶段以 CompositionActive 拒绝。
    {
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let len = document.current_snapshot()?.full_text(TEXT)?.len();
        let core = document.edit_core()?;
        core.open_block(0..len, SelectionSpec::new(0, 0))?;
        core.composition_start("中", 2)?;
    }
    match session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .association_prepared(completion, 3)
    {
        AssociationStep::Settled(AssociationOutcome::NeedsRetry) => {}
        _ => return Err("应用失败应结算为 NeedsRetry".into()),
    }
    assert!(
        !out.join("assets/pic.png").exists(),
        "失败后本次新建资产必须撤回"
    );
    assert!(
        !out.join("assets").exists(),
        "失败后本次新建的空资产目录也必须删除"
    );
    assert_eq!(
        session.document().ok_or("应存在可编辑文档")?.core().mode(),
        EditorMode::Visual,
        "迁移失败不得改变用户视图"
    );
    assert_eq!(
        session
            .document()
            .ok_or("应存在可编辑文档")?
            .core()
            .buffer_text()?,
        text_before,
        "失败后文档 bytes 不得改变"
    );
    assert!(
        matches!(
            session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .request_save_or_target(),
            SaveTargetCommand::RetryAssociation
        ),
        "失败后保存必须直接重试同一目标，不得重新选择"
    );
    {
        let core = session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .edit_core()?;
        core.composition_cancel(4)?;
        core.leave_block(5)?;
    }

    // 同一目标重试：走 `retry_association` 重新迁移并成功写盘，不再重新选路径。
    let retry = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .retry_association()
        .map_err(|rejection| format!("重试被拒: {rejection:?}"))?;
    let job = match retry {
        AssociationStep::Run(job) => job,
        _ => return Err("引用仍含 locator 时应重新迁移".into()),
    };
    let completion = job.run();
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    let written = std::fs::read_to_string(&target)?;
    assert!(written.contains("./assets/pic.png"));
    assert!(!written.contains(&locator));
    assert!(out.join("assets/pic.png").exists());
    assert_eq!(
        session.document().ok_or("应存在可编辑文档")?.core().mode(),
        EditorMode::Visual,
        "重试成功后用户视图仍为 Visual"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_copies_relative_assets_and_keeps_original() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-saveas")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n![x](./assets/pic.png)\n")?;
    std::fs::write(old.join("assets/pic.png"), b"png")?;
    assert!(old.join("assets/pic.png").exists(), "源资产应存在");
    let mut session =
        editable_absolute_session(&original).map_err(|error| format!("打开失败: {error}"))?;

    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let completion = association_copy(&mut session, WriteKind::SaveAs, &target)
        .map_err(|error| format!("复制阶段失败: {error}"))?;
    match completion.result.as_ref() {
        Ok(migration) => assert!(
            !migration.copied.is_empty(),
            "SaveAs 应发现已存在的相对资产，copied={:?} mappings={:?}",
            migration.copied,
            migration.mappings
        ),
        Err(error) => return Err(format!("SaveAs 迁移失败: {error:?}").into()),
    }
    let job = association_write_job(&mut session, completion)
        .map_err(|error| format!("准备写盘失败: {error}"))?;
    let settlement = run_association_write(&mut session, job)
        .map_err(|error| format!("写盘结算失败: {error}"))?;
    assert!(settlement.adopted);
    assert_eq!(
        std::fs::read(&original).map_err(|error| format!("旧文件读取失败: {error}"))?,
        b"x\n![x](./assets/pic.png)\n",
        "旧文件 bytes 不得改变"
    );
    assert_eq!(
        std::fs::read(directory.0.join("new/assets/pic.png"))
            .map_err(|error| format!("新资产读取失败: {error}"))?,
        b"png"
    );
    assert_eq!(std::fs::read(&target)?, b"x\n![x](./assets/pic.png)\n");
    assert_eq!(session.document().ok_or("应存在可编辑文档")?.path(), target);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_new_relative_asset_after_copy_requires_complete_retry() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-relative-race")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n")?;
    std::fs::write(old.join("assets/new.png"), b"new-image")?;
    let mut session = editable_absolute_session(&original)?;
    session
        .document_mut()
        .ok_or("document")?
        .switch_mode(EditorMode::Source)?;
    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let completion = association_copy(&mut session, WriteKind::SaveAs, &target)?;
    append_text(&mut session, "![b](./assets/new.png)")?;
    let before = current_text(&mut session)?;
    assert!(matches!(
        session
            .document_mut()
            .ok_or("document")?
            .association_prepared(completion, 1),
        AssociationStep::Settled(AssociationOutcome::NeedsRetry)
    ));
    assert!(!target.exists());
    assert_eq!(current_text(&mut session)?, before);
    let retry = session
        .document_mut()
        .ok_or("document")?
        .retry_association()
        .map_err(|error| format!("{error:?}"))?;
    let AssociationStep::Run(copy) = retry else {
        return Err("新增相对资产必须重新完整复制".into());
    };
    let job = association_write_job(&mut session, copy.run())?;
    assert!(run_association_write(&mut session, job)?.adopted);
    assert_eq!(
        std::fs::read(target.parent().ok_or("parent")?.join("assets/new.png"))?,
        b"new-image"
    );
    assert_eq!(std::fs::read_to_string(&target)?, before);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_preserves_relative_images_reachable_only_through_undo_or_redo() -> TestResult {
    for redo_only in [false, true] {
        let directory = TempSaveDir::create("aline-assoc-history-assets")?;
        let old = directory.0.join("old");
        std::fs::create_dir_all(old.join("assets"))?;
        let original = old.join("notes.md");
        std::fs::write(&original, b"x\n")?;
        std::fs::write(old.join("assets/pic.png"), b"historical-image")?;
        let mut session = editable_absolute_session(&original)?;
        session
            .document_mut()
            .ok_or("document")?
            .switch_mode(EditorMode::Source)?;
        append_text(&mut session, "![x](./assets/pic.png)")?;
        if redo_only {
            session
                .document_mut()
                .ok_or("document")?
                .edit_core()?
                .undo()?;
        } else {
            let document = session.document_mut().ok_or("document")?;
            let len = document.current_snapshot()?.len_bytes();
            let core = document.edit_core()?;
            core.open_block(0..len, SelectionSpec::new(1, len - 1))?;
            let identity = core.current_input_identity()?;
            core.replace_selection(&identity, "", 3)?;
        }
        assert_eq!(current_text(&mut session)?, "x\n");
        for target in [
            directory.0.join("new/notes.md"),
            directory.0.join("again/notes.md"),
        ] {
            std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
            let completion = association_copy(&mut session, WriteKind::SaveAs, &target)?;
            let job = association_write_job(&mut session, completion)?;
            assert!(run_association_write(&mut session, job)?.adopted);
            assert_eq!(
                std::fs::read(target.parent().ok_or("parent")?.join("assets/pic.png"))?,
                b"historical-image"
            );
        }
        let document = session.document_mut().ok_or("document")?;
        if redo_only {
            document.edit_core()?.redo()?;
        } else {
            document.edit_core()?.undo()?;
        }
        assert!(
            document
                .current_snapshot()?
                .full_text(TEXT)?
                .contains("./assets/pic.png")
        );
        assert_eq!(
            std::fs::read(
                document
                    .path()
                    .parent()
                    .ok_or("parent")?
                    .join("assets/pic.png")
            )?,
            b"historical-image"
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_preserves_relative_image_in_uncommitted_typing_redo() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-typing-redo")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n")?;
    std::fs::write(old.join("assets/pic.png"), b"pending-redo-image")?;
    let mut session = editable_absolute_session(&original)?;
    let document = session.document_mut().ok_or("document")?;
    document.switch_mode(EditorMode::Source)?;
    let core = document.edit_core()?;
    core.open_block(0..2, SelectionSpec::new(1, 1))?;
    core.type_text(1, "![x](./assets/pic.png)", 0)?;
    core.undo()?;
    assert_eq!(
        core.history_node_count(),
        0,
        "待定redo不得提前进入Buffer历史"
    );
    // redo_depth 只计 Buffer 节点（草稿重做不计数）；待定 redo 由目标范围证明。
    assert_eq!(core.redo_depth(), 0);
    assert_eq!(
        core.redo_target_ranges(),
        Some(std::iter::once(1..1).collect()),
        "待定redo应保留原输入组的插入起点"
    );
    for target in [
        directory.0.join("new/notes.md"),
        directory.0.join("again/notes.md"),
    ] {
        std::fs::create_dir_all(target.parent().ok_or("parent")?)?;
        let copy = association_copy(&mut session, WriteKind::SaveAs, &target)?;
        let job = association_write_job(&mut session, copy)?;
        assert!(run_association_write(&mut session, job)?.adopted);
        assert_eq!(
            std::fs::read(target.parent().ok_or("parent")?.join("assets/pic.png"))?,
            b"pending-redo-image"
        );
        assert_eq!(
            session
                .document()
                .ok_or("document")?
                .core()
                .history_node_count(),
            0
        );
    }
    let document = session.document_mut().ok_or("document")?;
    document.edit_core()?.redo()?;
    assert!(
        document
            .current_snapshot()?
            .full_text(TEXT)?
            .contains("./assets/pic.png")
    );
    assert_eq!(
        std::fs::read(
            document
                .path()
                .parent()
                .ok_or("parent")?
                .join("assets/pic.png")
        )?,
        b"pending-redo-image"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_nested_configured_asset_refuses_before_any_copy() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-nested")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets/sub"))?;
    let original = old.join("notes.md");
    let source = b"![a](./assets/pic.png)\n![b](./assets/sub/pic.png)\n";
    std::fs::write(&original, source)?;
    std::fs::write(old.join("assets/pic.png"), b"flat")?;
    std::fs::write(old.join("assets/sub/pic.png"), b"nested")?;
    let mut session = editable_absolute_session(&original)?;
    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("parent")?)?;
    let copy = association_copy(&mut session, WriteKind::SaveAs, &target)?;
    assert_eq!(
        copy.result.err(),
        Some(crate::document::MigrationError::InvalidTarget)
    );
    assert!(!target.exists());
    assert!(
        !target.parent().ok_or("parent")?.join("assets").exists(),
        "不能先复制平面文件再发现坏层级"
    );
    assert_eq!(std::fs::read(&original)?, source);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_asset_deletion_or_identity_replacement_never_publishes_reference() -> TestResult {
    for after_write in [false, true] {
        for replace in [false, true] {
            let directory = TempSaveDir::create("aline-assoc-asset-proof")?;
            let root = temp_resource_root(&directory.0.join("home"))?;
            let target = directory.0.join("out/notes.md");
            std::fs::create_dir_all(target.parent().ok_or("parent")?)?;
            let mut session = untitled_editable_session(Some(root))?;
            let locator = stage_image(&mut session, "pic.png", b"png")?;
            append_text(&mut session, &locator)?;
            let before = current_text(&mut session)?;
            let copy = association_copy(&mut session, WriteKind::Create, &target)?;
            let asset = target.parent().ok_or("parent")?.join("assets/pic.png");
            let mutate = |asset: &Path| -> std::io::Result<()> {
                if replace {
                    // 同尺寸不同内容 + 新身份：同时踩中字节比对与身份核验。
                    let replacement = asset.with_extension("replacement");
                    std::fs::write(&replacement, b"PNG")?;
                    std::fs::rename(replacement, asset)
                } else {
                    std::fs::remove_file(asset)
                }
            };
            if after_write {
                let job = association_write_job(&mut session, copy)?;
                let completion = job.run();
                mutate(&asset)?;
                let settlement = session
                    .document_mut()
                    .ok_or("document")?
                    .settle_save(completion)
                    .map_err(|error| format!("{error:?}"))?;
                assert!(settlement.adopted && settlement.association_deferred);
                assert!(matches!(settlement.outcome, WriteOutcome::Published { .. }));
                assert!(target.exists(), "已发生文件发布不可被抹掉");
            } else {
                mutate(&asset)?;
                assert!(matches!(
                    session
                        .document_mut()
                        .ok_or("document")?
                        .association_prepared(copy, 1),
                    AssociationStep::Settled(AssociationOutcome::NeedsRetry)
                ));
                assert!(!target.exists());
            }
            assert_eq!(current_text(&mut session)?, before);
            assert_eq!(
                session.document().ok_or("document")?.path(),
                Path::new("untitled.md")
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn first_save_changed_source_after_write_keeps_old_association_and_checks_retry_target()
-> TestResult {
    let directory = TempSaveDir::create("aline-assoc-after-write")?;
    let root = temp_resource_root(&directory.0.join("home"))?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let copy = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, copy)?;
    let written = job.run();
    append_text(&mut session, "B")?;
    let before = current_text(&mut session)?;
    let settlement = session
        .document_mut()
        .ok_or("document")?
        .settle_save(written)
        .map_err(|error| format!("{error:?}"))?;
    assert!(settlement.adopted && settlement.association_deferred);
    assert!(
        matches!(settlement.outcome, WriteOutcome::Published { .. }),
        "不能丢弃已发生的写盘事实"
    );
    assert_eq!(
        session.document().ok_or("document")?.path(),
        Path::new("untitled.md")
    );
    assert_eq!(current_text(&mut session)?, before);
    assert!(before.contains(&locator));
    assert_eq!(std::fs::read(&target)?, b"x./assets/pic.png\n");
    std::fs::write(&target, b"external replacement")?;
    let retry = session
        .document_mut()
        .ok_or("document")?
        .retry_association()
        .map_err(|error| format!("{error:?}"))?;
    let AssociationStep::Write(job) = retry else {
        return Err("已复制资产应复用候选重试".into());
    };
    let retry_settlement = run_association_write(&mut session, job)?;
    assert!(matches!(
        retry_settlement.outcome,
        WriteOutcome::NotPublished(WriteStop::Conflict)
    ));
    assert_eq!(
        std::fs::read(&target)?,
        b"external replacement",
        "部分发布重试不得盲目覆盖外改"
    );
    assert_eq!(current_text(&mut session)?, before);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_foreign_completion_cannot_consume_current_reference_candidate() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-foreign")?;
    let root = temp_resource_root(&directory.0.join("home"))?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let copy = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, copy)?;
    let (mut other_core, mut other) = fixture(b"x\n")?;
    queue(&mut other, &mut other_core)?;
    let foreign = published(start(&mut other)?, b"x\n", 1, true)?;
    assert!(
        !session
            .document_mut()
            .ok_or("document")?
            .settle_save(foreign)
            .map_err(|error| format!("{error:?}"))?
            .adopted
    );
    assert!(current_text(&mut session)?.contains(&locator));
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted && !settlement.association_deferred);
    assert_eq!(current_text(&mut session)?, "x./assets/pic.png\n");
    assert_eq!(session.document().ok_or("document")?.path(), target);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_cancel_after_file_publication_keeps_reference_and_records_partial_result()
-> TestResult {
    let directory = TempSaveDir::create("aline-assoc-published-cancel")?;
    let root = temp_resource_root(&directory.0.join("home"))?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let before = current_text(&mut session)?;
    let copy = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, copy)?;
    let completion = job.run();
    session
        .document_mut()
        .ok_or("document")?
        .cancel_association();
    let document = session.document_mut().ok_or("document")?;
    assert_eq!(
        document.settle_save_command(completion),
        SaveResult::SavedUncertain
    );
    assert_eq!(document.path(), Path::new("untitled.md"));
    assert_eq!(document.close_status(), CloseStatus::NeedsDecision);
    assert_eq!(current_text(&mut session)?, before);
    assert_eq!(std::fs::read(&target)?, b"x./assets/pic.png\n");
    assert_eq!(
        std::fs::read(target.parent().ok_or("parent")?.join("assets/pic.png"))?,
        b"png"
    );
    let retry = session
        .document_mut()
        .ok_or("document")?
        .retry_association()
        .map_err(|error| format!("{error:?}"))?;
    let AssociationStep::Write(job) = retry else {
        return Err("保留映射应可受检重试".into());
    };
    assert!(run_association_write(&mut session, job)?.adopted);
    assert_eq!(current_text(&mut session)?, "x./assets/pic.png\n");
    assert_eq!(
        session.document_mut().ok_or("document")?.close_status(),
        CloseStatus::Ready
    );
    Ok(())
}

/// 目标代次耗尽令已发布关联被拒时，仍须登记侧目标写盘事实；
/// 重试覆盖前逐字节复验，外改后的目标不得被盲目覆盖。
#[test]
fn association_rejected_after_publication_records_side_target_for_checked_retry() -> TestResult {
    let (mut doc, mut state) = fixture(b"x\n")?;
    doc.type_text(1, "a", 0)?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let side = fixture_path("assoc-exhausted-side.md");
    state.target.generation = PathGeneration(u64::MAX);
    state
        .queue_association_snapshot(snapshot, WriteKind::SaveAs, side.clone())
        .map_err(|error| format!("{error:?}"))?;
    let work = start(&mut state)?;
    assert!(
        work.association_expected.is_none(),
        "首次发布前没有侧目标事实"
    );
    let settlement = state.reconcile_association(published(work, b"xa\n", 1, true)?, || Ok(()));
    assert!(
        settlement.association_deferred,
        "代次耗尽必须走 deferred 分支"
    );
    assert!(settlement.association.is_none(), "不得采纳新目标关联");
    assert!(matches!(settlement.outcome, WriteOutcome::Published { .. }));
    assert_eq!(state.status(), SaveStatus::PublishedUncertain);
    assert_eq!(
        state.partial_association.as_ref().map(|(path, _)| path),
        Some(&side),
        "已发布的侧目标必须登记，供重试复验"
    );
    // 同一侧目标重试：解冻资格并携带冻结存储基线做写前比对。
    let snapshot = doc.current_snapshot(TEXT)?;
    state
        .queue_association_snapshot(snapshot, WriteKind::SaveAs, side.clone())
        .map_err(|error| format!("{error:?}"))?;
    let retry = start(&mut state)?;
    assert!(
        retry.association_expected.is_some(),
        "重试必须携带侧目标基线，否则 run_associate 会盲目覆盖外改"
    );
    Ok(())
}

/// 历史覆盖按节点边界与待定 redo 判定：已提交历史引用、当前引用与待定 redo
/// 引用都被复制；多步骤节点的逐步骤重放由 buffer `history_node_steps` 契约
/// 测试保证（当前生产提交路径每节点单步，中间态场景不可经公共 API 构造）。
#[cfg(target_os = "macos")]
#[test]
fn save_as_covers_committed_pending_and_typing_redo_references() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-three-states")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n")?;
    std::fs::write(old.join("assets/committed.png"), b"committed-image")?;
    std::fs::write(old.join("assets/pending.png"), b"pending-image")?;
    let mut session = editable_absolute_session(&original)?;
    session
        .document_mut()
        .ok_or("document")?
        .switch_mode(EditorMode::Source)?;
    {
        let document = session.document_mut().ok_or("document")?;
        let core = document.edit_core()?;
        core.open_block(0..2, SelectionSpec::new(1, 1))?;
        // 已提交引用：第一组键入并封口，留在当前文本与历史节点。
        core.type_text(1, "![a](./assets/committed.png)", 0)?;
        core.leave_block(1)?;
        let committed_len = 1 + "![a](./assets/committed.png)".len();
        // 待定 redo：第二组引用键入后草稿撤销，引用只在 typing_redo 可达。
        core.open_block(
            0..committed_len + 1,
            SelectionSpec::new(committed_len, committed_len),
        )?;
        core.type_text(committed_len, "![b](./assets/pending.png)", 2000)?;
        core.undo()?;
        assert_eq!(core.redo_depth(), 0, "草稿撤销不进入 Buffer 历史");
        assert_eq!(
            core.redo_target_ranges(),
            Some(std::iter::once(committed_len..committed_len).collect()),
            "待定 redo 保留第二组插入起点"
        );
    }
    assert_eq!(
        current_text(&mut session)?,
        "x![a](./assets/committed.png)\n"
    );
    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let completion = association_copy(&mut session, WriteKind::SaveAs, &target)?;
    let job = association_write_job(&mut session, completion)?;
    assert!(run_association_write(&mut session, job)?.adopted);
    let assets = target.parent().ok_or("parent")?.join("assets");
    assert_eq!(
        std::fs::read(assets.join("committed.png"))?,
        b"committed-image",
        "当前文本与已提交历史中的引用资产必须被复制"
    );
    assert_eq!(
        std::fs::read(assets.join("pending.png"))?,
        b"pending-image",
        "待定 redo 中的引用资产必须被复制"
    );
    Ok(())
}

/// 未发布（冲突/失败）结算不得消费引用候选；同一目标重试仍按重写后的候选写盘。
#[cfg(target_os = "macos")]
#[test]
fn association_conflict_keeps_prepared_candidate_for_retry() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempSaveDir::create("aline-assoc-conflict-candidate")?;
    let root = temp_resource_root(&directory.0.join("home"))?;
    let target = directory.0.join("out/notes.md");
    let out = target.parent().ok_or("target parent")?;
    std::fs::create_dir_all(out)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;
    let before = current_text(&mut session)?;
    assert!(before.contains(&locator));

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    // 迁移已复制资产；只读目录令首次写盘失败（NotPublished）。
    std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o500))?;
    let completion = job.run();
    std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o700))?;
    let settlement = session
        .document_mut()
        .ok_or("document")?
        .settle_save(completion)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(settlement.outcome, WriteOutcome::NotPublished(_)));
    assert!(!target.exists());
    assert_eq!(current_text(&mut session)?, before, "未发布不得提交引用");
    assert!(
        session
            .document()
            .ok_or("document")?
            .pending_migration_prepared(),
        "未发布结算必须保留引用候选供重试"
    );
    let retry = session
        .document_mut()
        .ok_or("document")?
        .retry_association()
        .map_err(|error| format!("{error:?}"))?;
    let AssociationStep::Write(job) = retry else {
        return Err("保留候选应可直接重试写盘".into());
    };
    assert!(run_association_write(&mut session, job)?.adopted);
    assert_eq!(
        std::fs::read(&target)?,
        b"x./assets/pic.png\n",
        "重试必须写重写后的引用，不得把临时 locator 写进新目标"
    );
    assert_eq!(current_text(&mut session)?, "x./assets/pic.png\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_absolute_setting_does_not_copy() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-absolute")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("assets"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n![x](./assets/pic.png)\n")?;
    std::fs::write(old.join("assets/pic.png"), b"png")?;
    let absolute = directory.0.join("absolute-assets");
    let settings = ImageSettings {
        directory: ImageDirectory::Absolute(absolute.clone()),
    };
    let mut session = editable_absolute_session_with(&original, settings)?;

    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let completion = association_copy(&mut session, WriteKind::SaveAs, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert!(
        !target
            .parent()
            .ok_or("target parent")?
            .join("assets")
            .exists()
    );
    assert!(!absolute.join("pic.png").exists(), "绝对目录不应产生新副本");
    assert_eq!(std::fs::read(&target)?, b"x\n![x](./assets/pic.png)\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_does_not_move_handwritten_external_reference() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-handwritten")?;
    let old = directory.0.join("old");
    std::fs::create_dir_all(old.join("images"))?;
    let original = old.join("notes.md");
    std::fs::write(&original, b"x\n![x](images/hand.png)\n")?;
    std::fs::write(old.join("images/hand.png"), b"hand")?;
    let mut session = editable_absolute_session(&original)?;

    let target = directory.0.join("new/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let completion = association_copy(&mut session, WriteKind::SaveAs, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert!(
        !directory.0.join("new/images/hand.png").exists(),
        "手写外部引用不得被迁移"
    );
    assert_eq!(std::fs::read(&target)?, b"x\n![x](images/hand.png)\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn save_as_typing_during_write_leaves_document_dirty_with_new_path() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-dirty")?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(None)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    // 写盘工作已冻结后继续输入；磁盘写旧快照，内存保持新文本。
    append_text(&mut session, "B")?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert_eq!(std::fs::read(&target)?, b"x\n");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.path(), target, "路径应在成功发布后更新");
    let snapshot = document.current_snapshot()?;
    assert_eq!(document.persistence_is_dirty(&snapshot), Some(true));
    assert_eq!(snapshot.full_text(TEXT)?, "xB\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn association_cancelled_before_copy_keeps_target_unassociated() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-cancel")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;

    let command = session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .begin_association_save(WriteKind::Create, target.clone(), 0);
    let AssociationCommand::Run(job) = command else {
        return Err("应先生成迁移工作".into());
    };
    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .cancel_association();
    let completion = job.run();
    assert!(completion.cancelled, "取消后迁移任务应观察取消标志");
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    assert_eq!(document.path(), Path::new("untitled.md"));
    assert!(!target.exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn close_status_waits_for_association_and_reports_pending_migration() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-close")?;
    let home = directory.0.join("home");
    let root = temp_resource_root(&home)?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(Some(root))?;
    let locator = stage_image(&mut session, "pic.png", b"png")?;
    append_text(&mut session, &locator)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let _job = association_write_job(&mut session, completion)?;
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .close_status(),
        CloseStatus::Waiting,
        "写盘关联在途必须等待"
    );

    session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .cancel_association();
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .close_status(),
        CloseStatus::NeedsDecision,
        "保留迁移映射时应要求用户决定"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn close_status_is_ready_after_saved_association() -> TestResult {
    let directory = TempSaveDir::create("aline-assoc-ready")?;
    let target = directory.0.join("out/notes.md");
    std::fs::create_dir_all(target.parent().ok_or("target parent")?)?;
    let mut session = untitled_editable_session(None)?;

    let completion = association_copy(&mut session, WriteKind::Create, &target)?;
    let job = association_write_job(&mut session, completion)?;
    let settlement = run_association_write(&mut session, job)?;
    assert!(settlement.adopted);
    assert_eq!(
        session
            .document_mut()
            .ok_or("应存在可编辑文档")?
            .close_status(),
        CloseStatus::Ready
    );
    Ok(())
}
