//! 恢复专用内存文档构造器（W4-57）：只由后台恢复任务的结果构造可编辑文档。
//! 不授予原路径代次、不挂接恢复会话、不创建保存协调器；首次关联成功后才恢复
//! 自动保存与恢复。本模块不新增通用“新建未命名文档”入口。

use std::convert::Infallible;
use std::num::NonZeroU64;
use std::path::PathBuf;

use aline_buffer::SelectionSpec;
use aline_markdown::parse_with_cancel;
use aline_source::{DiskVersion, StorageSnapshot, decode};

use super::{
    AutosaveScheduler, DocCore, DocError, ReadonlyDocument, ReadonlyOpenLimits, ReadonlySession,
};
use crate::document::recovery::{RecoveredPath, RecoveryDocumentId, RestoredRecovery};
use crate::document::resources::{ImageSettings, ResourceCatalog, ResourceError, TempRoot};

/// 恢复打开的失败分类；失败不产生部分文档、不改变当前文档。
/// `DocError` 不实现 `PartialEq`，因此按变体比较（与 `ResourceError` 一致）。
#[derive(Debug)]
pub(crate) enum RecoveredOpenError<E = Infallible> {
    /// 已有文档或未结算打开。
    Busy,
    /// 已有当前文档（打开期间不替换）。
    DocumentOpen,
    /// 恢复文本超过注入的文档文本预算。
    BudgetExceeded,
    /// 核心构造或发布复验失败。
    Document(DocError),
    /// 解析失败/取消，不发布部分文档。
    Parse,
    /// 临时资源登记失败。
    Resources(ResourceError),
    /// `open_recovered_with` 的宿主准备回调失败；文档未采纳。
    Prepare(E),
}

impl<E: PartialEq> PartialEq for RecoveredOpenError<E> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Busy, Self::Busy)
            | (Self::DocumentOpen, Self::DocumentOpen)
            | (Self::BudgetExceeded, Self::BudgetExceeded)
            | (Self::Parse, Self::Parse) => true,
            (Self::Document(_), Self::Document(_)) => true,
            (Self::Resources(left), Self::Resources(right)) => left == right,
            (Self::Prepare(left), Self::Prepare(right)) => left == right,
            _ => false,
        }
    }
}

impl<E: Eq> Eq for RecoveredOpenError<E> {}

/// 恢复打开的展示事实；`original_path` 只用于标题与选择器初始目录。
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RecoveredOpen {
    pub(crate) original_path: Option<PathBuf>,
    pub(crate) resources: usize,
}

/// 恢复来源；只作诊断与后续展示，不构成路径代次授权。
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct RecoveredOrigin {
    pub(super) document: RecoveryDocumentId,
    pub(super) generation: NonZeroU64,
    pub(super) original_path: PathBuf,
    pub(super) recorded_path_generation: u64,
}

impl ReadonlyDocument {
    /// 仅由后台恢复任务的结果构造；解析与核心/基线均在调用线程完成。
    pub(crate) fn from_recovered(
        restored: RestoredRecovery,
        limits: ReadonlyOpenLimits,
        temp_root: Option<TempRoot>,
        image_settings: ImageSettings,
    ) -> Result<Self, RecoveredOpenError> {
        let RestoredRecovery {
            recovered,
            temp_resources,
        } = restored;
        if recovered.source.len() > limits.document.text().max_bytes() {
            return Err(RecoveredOpenError::BudgetExceeded);
        }
        // 恢复记录不携带存储编码事实：固定 UTF-8 无 BOM、LF（§16 #5）。
        let storage = StorageSnapshot::new(
            recovered.source.as_bytes().to_vec(),
            DiskVersion::initial(),
            limits.storage,
        )
        .map_err(|_| RecoveredOpenError::BudgetExceeded)?;
        let baseline = decode(&storage, limits.storage)
            .map_err(|_| RecoveredOpenError::BudgetExceeded)?
            .into_baseline();
        let mut core =
            DocCore::new_editable(recovered.source, SelectionSpec::new(0, 0), limits.document)
                .map_err(RecoveredOpenError::Document)?;
        let snapshot = core
            .current_snapshot(limits.document.text())
            .map_err(RecoveredOpenError::Document)?;
        let identity = core
            .begin_document_request()
            .map_err(RecoveredOpenError::Document)?;
        // 与打开流程一致地使用无取消解析；恢复已由用户确认，不提供取消窗口。
        let cancel = aline_markdown::CancelToken::new();
        let parsed = parse_with_cancel(snapshot.text(), limits.config, limits.parse, &cancel)
            .map_err(|_| RecoveredOpenError::Parse)?;
        core.accept_document_result(&identity)
            .map_err(RecoveredOpenError::Document)?;
        let path = display_path(&recovered.path);
        let resources = match (temp_resources, recovered.resources.is_empty()) {
            (Some(materialized), false) => {
                let Some(root) = temp_root.as_ref() else {
                    return Err(RecoveredOpenError::Resources(
                        ResourceError::UnsupportedPlatform,
                    ));
                };
                let locators: Vec<String> = recovered
                    .resources
                    .iter()
                    .map(|resource| resource.reference.clone())
                    .collect();
                let catalog = ResourceCatalog::restore(
                    core.document_id().clone(),
                    root,
                    materialized.temp_document(),
                    &locators,
                )
                .map_err(RecoveredOpenError::Resources)?;
                // 目录归属建立成功后才采纳物化材料；此前任何失败由 Drop 清理。
                materialized.adopt();
                Some(catalog)
            }
            _ => None,
        };
        let origin = RecoveredOrigin {
            document: recovered.document,
            generation: recovered.generation,
            original_path: path.clone(),
            recorded_path_generation: recovered.path_generation,
        };
        Ok(Self {
            path,
            baseline,
            core,
            parsed,
            parsed_identity: identity,
            pending_parse: None,
            editable: true,
            limits: limits.document,
            persistence: None,
            storage_budget: limits.storage,
            external: None,
            external_pause: false,
            reload_pause: false,
            reload_mark: None,
            autosave: AutosaveScheduler::default(),
            recovery: None,
            resources,
            temp_root,
            image_settings,
            association: None,
            pending_migration: None,
            next_association: 0,
            parse_budget: limits.parse,
            recovered_origin: Some(origin),
            deferred_recovery: None,
            recovery_pending_arm: false,
        })
    }

    /// 恢复文档的展示原路径；仅用于标题/选择器初始目录。
    pub(crate) fn recovered_original_path(&self) -> Option<&PathBuf> {
        self.recovered_origin
            .as_ref()
            .map(|origin| &origin.original_path)
            .filter(|path| path.is_absolute())
    }
}

impl ReadonlySession {
    /// 恢复一条记录为当前文档；不排队、不替换已打开文档。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn open_recovered(
        &mut self,
        restored: RestoredRecovery,
    ) -> Result<RecoveredOpen, RecoveredOpenError> {
        match self.open_recovered_with(restored, |_| Ok::<(), Infallible>(())) {
            Ok((open, _)) => Ok(open),
            Err(error) => Err(error),
        }
    }

    /// 构造并准备成功后原子发布恢复文档；准备失败时记录仍保留在存储中。
    pub(crate) fn open_recovered_with<T, E>(
        &mut self,
        restored: RestoredRecovery,
        prepare: impl FnOnce(&mut ReadonlyDocument) -> Result<T, E>,
    ) -> Result<(RecoveredOpen, Option<T>), RecoveredOpenError<E>> {
        if !self.alive || self.pending.is_some() {
            return Err(RecoveredOpenError::Busy);
        }
        if self.document.is_some() {
            return Err(RecoveredOpenError::DocumentOpen);
        }
        let temp_root = self.temp_root.clone();
        let image_settings = self.image_settings.clone();
        let deferred = self.recovery.clone();
        let mut document =
            ReadonlyDocument::from_recovered(restored, self.limits, temp_root, image_settings)
                .map_err(plain_error::<E>)?;
        document.deferred_recovery = deferred;
        let prepared = prepare(&mut document).map_err(RecoveredOpenError::Prepare)?;
        let resources = document
            .resources
            .as_ref()
            .map_or(0, |catalog| catalog.entries().len());
        let open = RecoveredOpen {
            original_path: document.recovered_original_path().cloned(),
            resources,
        };
        self.document = Some(document);
        Ok((open, Some(prepared)))
    }
}

/// 把无准备回调的错误提升为任意 `E` 的错误；用于共享同一构造路径。
fn plain_error<E>(never: RecoveredOpenError<Infallible>) -> RecoveredOpenError<E> {
    match never {
        RecoveredOpenError::Busy => RecoveredOpenError::Busy,
        RecoveredOpenError::DocumentOpen => RecoveredOpenError::DocumentOpen,
        RecoveredOpenError::BudgetExceeded => RecoveredOpenError::BudgetExceeded,
        RecoveredOpenError::Document(error) => RecoveredOpenError::Document(error),
        RecoveredOpenError::Parse => RecoveredOpenError::Parse,
        RecoveredOpenError::Resources(error) => RecoveredOpenError::Resources(error),
        RecoveredOpenError::Prepare(never) => match never {},
    }
}

/// 解码展示路径：仅 macOS 的绝对 `Unix` 路径获得原路径展示；其余用占位名。
fn display_path(path: &RecoveredPath) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let RecoveredPath::Unix(bytes) = path {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;
            let decoded = PathBuf::from(OsStr::from_bytes(bytes));
            if decoded.is_absolute() {
                return decoded;
            }
        }
        PathBuf::from("recovered.md")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        PathBuf::from("recovered.md")
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    #[cfg(target_os = "macos")]
    use std::fs;
    use std::num::NonZeroU64;
    use std::path::{Path, PathBuf};
    #[cfg(target_os = "macos")]
    use std::sync::Arc;

    use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
    use aline_markdown::{ParseBudget, ParseConfig};
    #[cfg(target_os = "macos")]
    use aline_source::Cancellation;
    use aline_source::StorageBudget;

    use super::*;
    use crate::document::{
        AssociationCommand, AssociationStep, AutosaveAction, CloseDisposition, DocLimits,
        ImageSettings, ReadonlyDocument, ReadonlyOpenLimits, ReadonlySession, RecoveredDocument,
        RecoveredPath, RecoveryConfig, RecoveryDocumentId, RecoveryResourceSource,
        RecoveryRestoreError, RecoveryRestoreJob, RecoveryStore, ResourceError, RestoredRecovery,
        SaveRejection, SaveTargetCommand, TempResourceSource, TempRoot, UnavailableResources,
        WriteKind,
    };
    #[cfg(target_os = "macos")]
    use crate::document::{
        RecoveryGenerationInput, RecoveryPath, RecoveryPublishOutcome, RecoveryResourceInput,
        RecoveryRoot,
    };

    const TEXT: usize = 64 * 1024;

    type TestResult = Result<(), Box<dyn Error>>;

    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, Box<dyn Error>> {
        result.map_err(|error| format!("{error:?}").into())
    }

    fn limits() -> ReadonlyOpenLimits {
        ReadonlyOpenLimits::new(
            StorageBudget::new(TEXT),
            DocLimits::new(
                TextBudget::new(TEXT),
                LineIndexBudget::new(1024),
                HistoryBudget::new(16, TEXT),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing()
    }

    fn non_zero(value: u64) -> Result<NonZeroU64, Box<dyn Error>> {
        NonZeroU64::new(value).ok_or_else(|| "代次/身份必须非零".into())
    }

    fn recovered_identity(value: u64) -> Result<RecoveryDocumentId, Box<dyn Error>> {
        Ok(RecoveryDocumentId::from_raw_for_test(non_zero(value)?))
    }

    fn recovered_document(
        source: &str,
        generation: u64,
        path: RecoveredPath,
        resources: Vec<crate::document::recovery::RecoveredResource>,
    ) -> Result<RecoveredDocument, Box<dyn Error>> {
        Ok(RecoveredDocument {
            document: recovered_identity(1)?,
            generation: non_zero(generation)?,
            path_generation: 7,
            captured_composition: false,
            path,
            anchor_byte: 0,
            head_byte: 0,
            source: source.to_owned(),
            logical_saved: None,
            baseline: None,
            resources,
        })
    }

    fn restored(source: &str, path: RecoveredPath) -> Result<RestoredRecovery, Box<dyn Error>> {
        Ok(RestoredRecovery {
            recovered: recovered_document(source, 1, path, Vec::new())?,
            temp_resources: None,
        })
    }

    fn open_plain(source: &str) -> Result<ReadonlyDocument, Box<dyn Error>> {
        ok(ReadonlyDocument::from_recovered(
            restored(source, RecoveredPath::Unlinked)?,
            limits(),
            None,
            ImageSettings::default(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn associate_session(
        session: &mut ReadonlySession,
        target: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let completion = {
            let document = session.document_mut().ok_or("应存在可编辑文档")?;
            match document.begin_association_save(WriteKind::Create, target.to_path_buf(), 0) {
                AssociationCommand::Run(job) => job.run(),
                AssociationCommand::Wait(_) => return Err("无组合时不应等待".into()),
                AssociationCommand::Save(_) => return Err("空资源应进入迁移".into()),
                AssociationCommand::Cancelled => return Err("关联不应在开始前取消".into()),
                AssociationCommand::Rejected(_) => return Err("关联不应被拒绝".into()),
            }
        };
        let job = {
            let document = session.document_mut().ok_or("应存在可编辑文档")?;
            match document.association_prepared(completion, 1) {
                AssociationStep::Write(job) => job,
                AssociationStep::Run(_) => return Err("复制完成项不应再次要求复制".into()),
                AssociationStep::Settled(_) => return Err("关联未进入写盘".into()),
            }
        };
        let completion = job.run();
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        let settlement = ok(document.settle_save(completion))?;
        assert!(settlement.adopted, "首次保存应采纳目标");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    struct TempDir(PathBuf);

    #[cfg(target_os = "macos")]
    impl TempDir {
        fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
            use std::time::{SystemTime, UNIX_EPOCH};

            let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            for attempt in 0..100u32 {
                let path = std::env::temp_dir().join(format!(
                    "aline-recovered-{prefix}-{}-{nanos:x}-{attempt}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err("无法创建恢复测试目录".into())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(target_os = "macos")]
    struct Fixture {
        dir: TempDir,
        store: RecoveryStore,
    }

    #[cfg(target_os = "macos")]
    impl Fixture {
        fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
            let dir = TempDir::new(prefix)?;
            let root_path = dir.path().join("recovery");
            fs::create_dir(&root_path)?;
            let root = ok(RecoveryRoot::open(&root_path))?;
            let store = ok(RecoveryStore::open(root))?;
            Ok(Self { dir, store })
        }

        fn lineage(&self) -> Result<RecoveryDocumentId, Box<dyn Error>> {
            ok(self.store.open_lineage(100))
        }

        fn publish(
            &self,
            document: RecoveryDocumentId,
            source: &str,
            resources: Vec<RecoveryResourceInput>,
        ) -> Result<NonZeroU64, Box<dyn Error>> {
            let input = RecoveryGenerationInput {
                path_generation: 7,
                captured_composition: false,
                path: RecoveryPath::Unlinked,
                anchor_byte: 0,
                head_byte: 0,
                source: source.to_owned(),
                logical_saved: None,
                baseline: None,
                resources,
            };
            let cancellation = Cancellation::new();
            match ok(self.store.publish(document, input, &cancellation))? {
                RecoveryPublishOutcome::Published { generation, .. } => Ok(generation),
                other => Err(format!("期望 Published，实际 {other:?}").into()),
            }
        }

        fn temp_root(&self) -> Result<TempRoot, Box<dyn Error>> {
            let home = self.dir.path().join("home");
            fs::create_dir_all(home.join("Library/Application Support"))?;
            ok(TempRoot::open_or_create(&home))
        }
    }

    fn restored_with_path(
        source: &str,
        path: RecoveredPath,
    ) -> Result<RestoredRecovery, Box<dyn Error>> {
        restored(source, path)
    }

    #[test]
    fn recovered_document_uses_source_and_starts_plain() -> TestResult {
        let source = "# 恢复\n\n正文\n";
        let document = open_plain(source)?;
        assert_eq!(document.core().buffer_text()?, source);
        assert!(document.core().composition().is_none());
        assert!(!document.core().has_open_block());
        assert_eq!(
            document.core().current_selection(),
            SelectionSpec::new(0, 0)
        );
        assert_eq!(document.core().undo_depth(), 0);
        Ok(())
    }

    #[test]
    fn recovered_document_has_no_path_generation_or_autosave() -> TestResult {
        let mut document = open_plain("x\n")?;
        assert!(document.persistence.is_none());
        assert_eq!(document.autosave_poll(0), AutosaveAction::NoTarget);
        assert!(document.recovery_deadline().is_none());
        assert!(matches!(
            document.request_save_or_target(),
            SaveTargetCommand::ChooseTarget {
                kind: WriteKind::Create
            }
        ));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_original_path_is_display_only() -> TestResult {
        use std::os::unix::ffi::OsStrExt;

        let fixture = Fixture::new("display")?;
        let original = fixture.dir.path().join("original.md");
        let raw = original.as_os_str().as_bytes().to_vec();
        let mut document = ok(ReadonlyDocument::from_recovered(
            restored_with_path("x\n", RecoveredPath::Unix(raw))?,
            limits(),
            None,
            ImageSettings::default(),
        ))?;
        assert_eq!(document.path(), original);
        assert_eq!(
            document.recovered_original_path().map(PathBuf::as_path),
            Some(original.as_path())
        );
        let baseline = document.baseline().source().to_owned();

        // 即使目标等于展示原路径，无协调器仍按 Create 进入关联，不直接覆盖。
        let command = document.begin_association_save(WriteKind::SaveAs, original.clone(), 0);
        assert!(matches!(command, AssociationCommand::Run(_)));
        assert_eq!(document.path(), original, "写盘前展示路径不得变化");
        assert_eq!(document.baseline().source(), baseline, "写盘前基线不得变化");
        assert!(!original.exists(), "关联开始不得写盘");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_grants_path_only_after_association() -> TestResult {
        use crate::document::persistence::CloseStatus;

        let fixture = Fixture::new("associate")?;
        let target = fixture.dir.path().join("saved.md");
        let mut session = ReadonlySession::new(limits());
        session
            .open_recovered(restored("x\n", RecoveredPath::Unlinked)?)
            .map_err(|error| format!("{error:?}"))?;
        associate_session(&mut session, &target)?;

        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert_eq!(document.path(), target);
        assert_eq!(document.persistence_is_creating(), Some(false));
        let snapshot = document.current_snapshot()?;
        assert_eq!(document.persistence_is_dirty(&snapshot), Some(false));
        assert_eq!(document.close_status(), CloseStatus::Ready);

        let mut fresh = open_plain("x\n")?;
        assert!(matches!(
            fresh.begin_association_save(WriteKind::Create, PathBuf::from("relative.md"), 0),
            AssociationCommand::Rejected(SaveRejection::NoTarget)
        ));
        assert!(fresh.persistence.is_none(), "失败不得创建协调器");
        assert_eq!(fresh.path(), Path::new("recovered.md"));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_attaches_recovery_only_after_association() -> TestResult {
        let fixture = Fixture::new("attach")?;
        let target = fixture.dir.path().join("saved.md");
        let mut session = ReadonlySession::new(limits());
        session
            .configure_recovery(Some(RecoveryConfig {
                store: fixture.store.clone(),
                resources: Arc::new(UnavailableResources),
            }))
            .map_err(|error| format!("{error:?}"))?;
        session
            .open_recovered(restored("x\n", RecoveredPath::Unlinked)?)
            .map_err(|error| format!("{error:?}"))?;
        {
            let document = session.document_mut().ok_or("应存在可编辑文档")?;
            assert!(document.recovery.is_none(), "关联前不得挂接恢复");
            assert!(!document.take_recovery_pending_arm());
        }

        associate_session(&mut session, &target)?;
        let document = session.document_mut().ok_or("应存在可编辑文档")?;
        assert!(document.recovery.is_some(), "成功后应挂接恢复");
        assert!(document.take_recovery_pending_arm(), "首次布防只请求一次");
        assert!(!document.take_recovery_pending_arm());
        document.recovery_arm(20_000);
        assert_eq!(document.recovery_deadline(), Some(30_000));
        Ok(())
    }

    #[test]
    fn recovered_unnamed_record_uses_home_for_target_dialog() -> TestResult {
        let document = open_plain("x\n")?;
        assert_eq!(document.path(), Path::new("recovered.md"));
        assert!(document.recovered_original_path().is_none());
        let home = Path::new("/tmp/aline-recovered-home");
        assert_eq!(
            crate::native_window::target_initial_directory(Some(document.path()), Some(home)),
            Some(home.to_path_buf())
        );
        assert_eq!(
            crate::native_window::target_suggested_name(Some(document.path())),
            "untitled.md"
        );
        assert!(
            crate::native_window::target_initial_directory(Some(document.path()), None).is_none()
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_restores_embedded_resources_into_temp_root() -> TestResult {
        let fixture = Fixture::new("resources")?;
        let document_id = fixture.lineage()?;
        let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
        let generation = fixture.publish(
            document_id,
            "# 恢复\n",
            vec![RecoveryResourceInput {
                reference: locator.clone(),
                bytes: b"png-bytes".to_vec(),
            }],
        )?;
        let root = fixture.temp_root()?;
        // 模拟原临时文件已不存在：恢复任务只依赖记录内嵌 bytes 写回。
        let directory = root.path().join("0000000000000005");
        fs::create_dir_all(&directory)?;
        let destination = directory.join("0000000000000001.png");
        fs::write(&destination, b"stale")?;
        fs::remove_file(&destination)?;

        let completion = RecoveryRestoreJob::new(
            fixture.store.clone(),
            document_id,
            generation,
            11,
            Some(root.clone()),
        )
        .run();
        let restored = match completion.result {
            Ok(restored) => restored,
            Err(error) => return Err(format!("恢复应成功: {error:?}").into()),
        };
        assert!(restored.temp_resources.is_some());
        let document = ok(ReadonlyDocument::from_recovered(
            restored,
            limits(),
            Some(root.clone()),
            ImageSettings::default(),
        ))?;
        assert_eq!(
            document
                .resources
                .as_ref()
                .map_or(0, |catalog| catalog.entries().len()),
            1
        );
        let source = TempResourceSource::new(root);
        let bytes = source
            .read(&locator, 1024)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(bytes, b"png-bytes");
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_resource_materialization_conflict_fails_closed() -> TestResult {
        let fixture = Fixture::new("conflict")?;
        let document_id = fixture.lineage()?;
        let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
        let generation = fixture.publish(
            document_id,
            "body\n",
            vec![RecoveryResourceInput {
                reference: locator.clone(),
                bytes: b"record-bytes".to_vec(),
            }],
        )?;
        let root = fixture.temp_root()?;
        let FirstRun { mut restored } = run_restore(
            &fixture.store,
            document_id,
            generation,
            Some(root.clone()),
            21,
        )?;
        // 采纳首次恢复的材料，模拟已建立目录归属，避免 Drop 清理干扰冲突用例。
        let temp_document = restored
            .temp_resources
            .take()
            .ok_or("首次恢复应有临时身份")?
            .adopt();
        let destination = root
            .path()
            .join(temp_document.directory_name())
            .join("0000000000000001.png");
        fs::write(&destination, b"different-bytes")?;

        let second = run_restore(
            &fixture.store,
            document_id,
            generation,
            Some(root.clone()),
            22,
        );
        match second {
            Err(RestoreFailure::Resources(ResourceError::Io)) => {}
            Err(other) => return Err(format!("期望资源冲突失败，实际 {other:?}").into()),
            Ok(_) => return Err("冲突必须失败关闭".into()),
        }
        assert_eq!(fs::read(&destination)?, b"different-bytes");
        let recovered = ok(fixture.store.recover(document_id, generation))?;
        assert_eq!(recovered.source, "body\n");
        assert_eq!(
            ok(fixture.store.scan())?
                .recoverable_lineages()
                .iter()
                .filter(|lineage| lineage.document == document_id)
                .count(),
            1
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    struct FirstRun {
        restored: RestoredRecovery,
    }

    #[cfg(target_os = "macos")]
    #[derive(Debug)]
    enum RestoreFailure {
        Resources(ResourceError),
        Debug(String),
    }

    #[cfg(target_os = "macos")]
    impl std::fmt::Display for RestoreFailure {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Resources(error) => write!(formatter, "资源恢复失败: {error:?}"),
                Self::Debug(message) => formatter.write_str(message),
            }
        }
    }

    #[cfg(target_os = "macos")]
    impl std::error::Error for RestoreFailure {}

    #[cfg(target_os = "macos")]
    fn run_restore(
        store: &RecoveryStore,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
        temp_root: Option<TempRoot>,
        operation: u64,
    ) -> Result<FirstRun, RestoreFailure> {
        let completion =
            RecoveryRestoreJob::new(store.clone(), document, generation, operation, temp_root)
                .run();
        match completion.result {
            Ok(restored) => Ok(FirstRun { restored }),
            Err(RecoveryRestoreError::Resources(error)) => Err(RestoreFailure::Resources(error)),
            Err(error) => Err(RestoreFailure::Debug(format!("{error:?}"))),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_rejects_when_document_or_open_is_pending() -> TestResult {
        let fixture = Fixture::new("busy")?;
        let document_id = fixture.lineage()?;
        let generation = fixture.publish(document_id, "keep\n", Vec::new())?;
        let before = ok(fixture.store.scan())?;
        let restored = RestoredRecovery {
            recovered: ok(fixture.store.recover(document_id, generation))?,
            temp_resources: None,
        };
        let mut session = ReadonlySession::new(limits());
        session
            .open_recovered(restored)
            .map_err(|error| format!("{error:?}"))?;
        let second = session.open_recovered(RestoredRecovery {
            recovered: ok(fixture.store.recover(document_id, generation))?,
            temp_resources: None,
        });
        assert!(matches!(second, Err(RecoveredOpenError::DocumentOpen)));
        assert_eq!(
            session
                .document()
                .ok_or("应存在当前文档")?
                .core()
                .buffer_text()?,
            "keep\n"
        );
        assert_eq!(ok(fixture.store.scan())?, before);

        let mut pending = ReadonlySession::new(limits());
        let _request = pending.request_open()?;
        let third = pending.open_recovered(RestoredRecovery {
            recovered: ok(fixture.store.recover(document_id, generation))?,
            temp_resources: None,
        });
        assert!(matches!(third, Err(RecoveredOpenError::Busy)));
        assert!(pending.document().is_none());
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_close_without_edits_is_ready() -> TestResult {
        let fixture = Fixture::new("close")?;
        let document_id = fixture.lineage()?;
        let generation = fixture.publish(document_id, "keep\n", Vec::new())?;
        let mut session = ReadonlySession::new(limits());
        session
            .configure_recovery(Some(RecoveryConfig {
                store: fixture.store.clone(),
                resources: Arc::new(UnavailableResources),
            }))
            .map_err(|error| format!("{error:?}"))?;
        session
            .open_recovered(RestoredRecovery {
                recovered: ok(fixture.store.recover(document_id, generation))?,
                temp_resources: None,
            })
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            session.document_mut().ok_or("应存在文档")?.close_status(),
            crate::document::persistence::CloseStatus::Ready
        );
        assert_eq!(session.close_document(), CloseDisposition::Closed);
        assert!(session.take_retired_recoveries().is_empty());
        assert!(session.take_retired_cleanups().is_empty());
        assert_eq!(
            ok(fixture.store.scan())?
                .recoverable_lineages()
                .iter()
                .filter(|lineage| lineage.document == document_id)
                .count(),
            1,
            "关闭未编辑恢复文档不得删除记录"
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_without_temp_root_requires_resources() -> TestResult {
        let fixture = Fixture::new("no-root")?;
        let document_id = fixture.lineage()?;
        let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
        let generation = fixture.publish(
            document_id,
            "body\n",
            vec![RecoveryResourceInput {
                reference: locator,
                bytes: b"img".to_vec(),
            }],
        )?;
        match run_restore(&fixture.store, document_id, generation, None, 31) {
            Err(RestoreFailure::Resources(ResourceError::UnsupportedPlatform)) => {}
            Err(other) => return Err(format!("期望平台失败，实际 {other:?}").into()),
            Ok(_) => return Err("无临时根时含资源记录必须失败".into()),
        }
        assert_eq!(
            ok(fixture.store.scan())?
                .recoverable_lineages()
                .iter()
                .filter(|lineage| lineage.document == document_id)
                .count(),
            1,
            "失败必须保留记录"
        );

        let root = fixture.temp_root()?;
        let FirstRun { restored } = run_restore(
            &fixture.store,
            document_id,
            generation,
            Some(root.clone()),
            32,
        )?;
        ok(ReadonlyDocument::from_recovered(
            restored,
            limits(),
            Some(root),
            ImageSettings::default(),
        ))?;

        let plain = fixture.lineage()?;
        let plain_generation = fixture.publish(plain, "plain\n", Vec::new())?;
        let FirstRun { restored } = run_restore(&fixture.store, plain, plain_generation, None, 33)?;
        ok(ReadonlyDocument::from_recovered(
            restored,
            limits(),
            None,
            ImageSettings::default(),
        ))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovered_document_encoding_baseline_is_lf_without_bom() -> TestResult {
        let fixture = Fixture::new("encoding")?;
        let target = fixture.dir.path().join("saved.md");
        let source = "# 标题\n\n正文\n";
        let mut session = ReadonlySession::new(limits());
        session
            .open_recovered(restored(source, RecoveredPath::Unlinked)?)
            .map_err(|error| format!("{error:?}"))?;
        associate_session(&mut session, &target)?;
        let bytes = fs::read(&target)?;
        assert_eq!(bytes, source.as_bytes());
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "不得写入 BOM");
        assert!(!bytes.contains(&b'\r'), "不得写入 CR");
        Ok(())
    }
}
