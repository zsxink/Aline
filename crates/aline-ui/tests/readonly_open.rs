//! 无窗口只读文件选择、受限读取及 Loading/Ready 原子发布回归。

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use aline_ui::{
    DocLimits, EditorMode, OpenDisposition, OpenError, ReadonlyFileAccess, ReadonlyFileInput,
    ReadonlyOpenLimits, ReadonlySession,
};

fn limits(max_bytes: usize) -> ReadonlyOpenLimits {
    ReadonlyOpenLimits::new(
        StorageBudget::new(max_bytes),
        DocLimits::new(
            TextBudget::new(max_bytes),
            LineIndexBudget::new(4096),
            HistoryBudget::new(128, max_bytes),
        ),
        ParseConfig::m0(),
        ParseBudget::m0().with_max_source_bytes(max_bytes),
    )
}

struct TrackedReader {
    cursor: Cursor<Vec<u8>>,
    consumed: Rc<Cell<usize>>,
    dropped: Rc<Cell<bool>>,
}
impl Read for TrackedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.cursor.read(output)?;
        self.consumed.set(self.consumed.get() + count);
        Ok(count)
    }
}
impl Drop for TrackedReader {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

struct Files {
    bytes: Vec<u8>,
    reported_len: u64,
    regular: bool,
    error: Option<io::ErrorKind>,
    opens: Rc<Cell<usize>>,
    consumed: Rc<Cell<usize>>,
    dropped: Rc<Cell<bool>>,
}
impl Files {
    fn bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            reported_len: bytes.len() as u64,
            regular: true,
            error: None,
            opens: Rc::new(Cell::new(0)),
            consumed: Rc::new(Cell::new(0)),
            dropped: Rc::new(Cell::new(false)),
        }
    }
}
impl ReadonlyFileAccess for Files {
    type Reader = TrackedReader;
    fn open(&self, _path: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        self.opens.set(self.opens.get() + 1);
        if let Some(kind) = self.error {
            return Err(io::Error::from(kind));
        }
        Ok(ReadonlyFileInput::new(
            TrackedReader {
                cursor: Cursor::new(self.bytes.clone()),
                consumed: self.consumed.clone(),
                dropped: self.dropped.clone(),
            },
            self.reported_len,
            self.regular,
        ))
    }
}

fn job(
    session: &mut ReadonlySession,
    name: &str,
) -> Result<aline_ui::ReadonlyOpenJob, Box<dyn Error>> {
    let request = session.request_open()?;
    session
        .accept_selection(&request, Some(vec![PathBuf::from(name)]))?
        .ok_or_else(|| "selection unexpectedly cancelled".into())
}

fn publish(session: &mut ReadonlySession, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let completion = job(session, "document.md")?.read_with(&Files::bytes(bytes));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Applied
    ));
    Ok(())
}

#[test]
fn loading_keeps_old_ready_document_and_replacement_publishes_atomically()
-> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    assert!(session.document().is_none());
    assert!(!session.is_loading());
    publish(&mut session, b"\xef\xbb\xbf# old\r\n\runchanged")?;
    let original = session
        .document()
        .ok_or("missing original")?
        .core()
        .document_id()
        .clone();
    let opening = job(&mut session, "replacement.MD")?;
    assert!(session.is_loading());
    assert_eq!(
        session
            .document()
            .ok_or("missing original")?
            .core()
            .document_id(),
        &original
    );
    let files = Files::bytes(b"# new\n");
    let completion = opening.read_with(&files);
    assert!(
        files.dropped.get(),
        "reader must close before the completion is handed to UI"
    );
    assert_eq!(
        session
            .document()
            .ok_or("missing original")?
            .core()
            .document_id(),
        &original
    );
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Applied
    ));
    let document = session.document().ok_or("missing new document")?;
    assert_ne!(document.core().document_id(), &original);
    assert_eq!(document.baseline().storage().bytes(), b"# new\n");
    assert_eq!(document.core().buffer_text()?, "# new\n");
    assert!(!document.core().has_open_block());
    assert_eq!(document.core().history_node_count(), 0);
    assert!(!session.is_loading());
    Ok(())
}

#[test]
fn out_of_order_open_completions_only_publish_latest_request() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    let a = job(&mut session, "a.md")?.read_with(&Files::bytes(b"A"));
    let b = job(&mut session, "b.md")?.read_with(&Files::bytes(b"B"));
    assert!(matches!(session.accept_open(b), OpenDisposition::Applied));
    assert!(matches!(session.accept_open(a), OpenDisposition::Stale));
    assert_eq!(
        session
            .document()
            .ok_or("missing B")?
            .core()
            .buffer_text()?,
        "B"
    );
    Ok(())
}

#[test]
fn cancelling_selection_or_read_never_reports_success_or_error() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    publish(&mut session, b"old")?;
    let request = session.request_open()?;
    assert!(session.accept_selection(&request, None)?.is_none());
    assert!(!session.is_loading());
    let request = session.request_open()?;
    let opening = session
        .accept_selection(&request, Some(vec![PathBuf::from("a.md")]))?
        .ok_or("missing open job")?;
    assert!(session.cancel_open(&request));
    let files = Files::bytes(b"new");
    assert!(matches!(
        session.accept_open(opening.read_with(&files)),
        OpenDisposition::Stale
    ));
    assert_eq!(files.opens.get(), 0);
    assert_eq!(
        session
            .document()
            .ok_or("missing old")?
            .core()
            .buffer_text()?,
        "old"
    );
    Ok(())
}

#[test]
fn selection_validation_rejects_multi_path_extension_and_directory() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    for paths in [vec![], vec![PathBuf::from("a.md"), PathBuf::from("b.md")]] {
        let request = session.request_open()?;
        assert!(matches!(
            session.accept_selection(&request, Some(paths)),
            Err(OpenError::SelectionCount)
        ));
        assert!(!session.is_loading());
    }
    let request = session.request_open()?;
    assert!(matches!(
        session.accept_selection(&request, Some(vec![PathBuf::from("a.txt")])),
        Err(OpenError::UnsupportedExtension)
    ));
    let mut files = Files::bytes(b"");
    files.regular = false;
    let completion = job(&mut session, "directory.md")?.read_with(&files);
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::NotRegularFile)
    ));
    assert_eq!(files.consumed.get(), 0);
    assert!(session.document().is_none());
    Ok(())
}

#[test]
fn failure_preserves_existing_document_selection_mode_and_original_bytes()
-> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    let original = b"\xef\xbb\xbfold\r\n\rraw";
    publish(&mut session, original)?;
    let document = session.document_mut().ok_or("missing old")?;
    let identity = document.input_identity()?;
    let snapshot = document.parsed().snapshot();
    let selection =
        aline_buffer::SrcSelection::new(snapshot.src_offset(2)?, snapshot.src_offset(2)?)?;
    document.navigate(&identity, &selection)?;
    document.switch_mode(EditorMode::Source)?;
    let id = document.core().document_id().clone();
    for failure in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound] {
        let mut files = Files::bytes(b"replacement");
        files.error = Some(failure);
        let completion = job(&mut session, "a.md")?.read_with(&files);
        assert!(
            matches!(session.accept_open(completion), OpenDisposition::Failed(OpenError::Io(kind)) if kind == failure)
        );
        let document = session.document().ok_or("missing original")?;
        assert_eq!(document.core().document_id(), &id);
        assert_eq!(
            document.core().current_selection(),
            SelectionSpec::new(2, 2)
        );
        assert_eq!(document.core().mode(), EditorMode::Source);
        assert_eq!(document.baseline().storage().bytes(), original);
    }
    let completion = job(&mut session, "invalid.md")?.read_with(&Files::bytes(&[0xff]));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::Decode(_))
    ));
    assert_eq!(
        session
            .document()
            .ok_or("missing original")?
            .core()
            .document_id(),
        &id
    );
    Ok(())
}

#[test]
fn metadata_and_growing_reads_obey_hard_byte_limit_without_partial_publication()
-> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4));
    let files = Files::bytes(b"0123456789");
    let completion = job(&mut session, "large.md")?.read_with(&files);
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::BudgetExceeded)
    ));
    assert_eq!(files.consumed.get(), 0);
    let mut files = Files::bytes(b"0123456789");
    files.reported_len = 1;
    let completion = job(&mut session, "growing.md")?.read_with(&files);
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::BudgetExceeded)
    ));
    assert_eq!(
        files.consumed.get(),
        5,
        "only a single sentinel byte may exceed the limit"
    );
    assert!(files.dropped.get());
    assert!(session.document().is_none());
    let completion = job(&mut session, "exact.md")?.read_with(&Files::bytes(b"four"));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Applied
    ));
    assert_eq!(
        session
            .document()
            .ok_or("missing exact-budget file")?
            .baseline()
            .storage()
            .bytes(),
        b"four"
    );
    let mut empty = ReadonlySession::new(limits(0));
    let completion = job(&mut empty, "empty.md")?.read_with(&Files::bytes(b""));
    assert!(matches!(
        empty.accept_open(completion),
        OpenDisposition::Applied
    ));
    assert_eq!(
        empty
            .document()
            .ok_or("missing empty file")?
            .core()
            .buffer_text()?,
        ""
    );
    Ok(())
}

#[test]
fn close_invalidates_pending_jobs_and_drops_current_document() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    publish(&mut session, b"current")?;
    let late = job(&mut session, "late.md")?.read_with(&Files::bytes(b"late"));
    session.close_document();
    assert!(!session.is_loading());
    assert!(session.document().is_none());
    assert!(matches!(session.accept_open(late), OpenDisposition::Stale));
    let late = job(&mut session, "late.md")?;
    session.close_window();
    assert!(matches!(
        session.accept_open(late.read_with(&Files::bytes(b"late"))),
        OpenDisposition::Stale
    ));
    assert!(matches!(
        session.request_open(),
        Err(OpenError::ClosedWindow)
    ));
    Ok(())
}

#[test]
fn actual_local_file_read_leaves_disk_bytes_identical() -> Result<(), Box<dyn Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "aline-readonly-open-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory)?;
    let path = directory.join("sample.MD");
    let bytes = b"\xef\xbb\xbf# real\r\n\risolated\nlast";
    std::fs::write(&path, bytes)?;
    let mut session = ReadonlySession::new(limits(4096));
    let request = session.request_open()?;
    let opening = session
        .accept_selection(&request, Some(vec![path.clone()]))?
        .ok_or("missing local job")?;
    assert!(matches!(
        session.accept_open(opening.run()),
        OpenDisposition::Applied
    ));
    assert_eq!(
        session
            .document()
            .ok_or("missing local doc")?
            .baseline()
            .storage()
            .bytes(),
        bytes
    );
    session.close_document();
    assert_eq!(std::fs::read(&path)?, bytes);
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn first_open_failure_and_late_selection_keep_the_current_target() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(4096));
    let mut files = Files::bytes(b"x");
    files.error = Some(io::ErrorKind::PermissionDenied);
    let completion = job(&mut session, "denied.md")?.read_with(&files);
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::Io(io::ErrorKind::PermissionDenied))
    ));
    assert!(session.document().is_none());
    assert!(!session.is_loading());
    let completion = job(&mut session, "invalid.md")?.read_with(&Files::bytes(&[0xff]));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::Decode(_))
    ));
    assert!(session.document().is_none());

    let a = session.request_open()?;
    let b = session.request_open()?;
    assert!(
        !session.cancel_open(&a),
        "a late cancellation must not cancel B"
    );
    assert!(matches!(
        session.accept_selection(&a, None),
        Err(OpenError::StaleRequest)
    ));
    assert!(session.is_loading());
    let opening = session
        .accept_selection(&b, Some(vec![PathBuf::from("b.md")]))?
        .ok_or("missing B")?;
    assert!(matches!(
        session.accept_open(opening.read_with(&Files::bytes(b"B"))),
        OpenDisposition::Applied
    ));

    let mut other = ReadonlySession::new(limits(4096));
    let foreign = job(&mut other, "other.md")?.read_with(&Files::bytes(b"foreign"));
    assert!(matches!(
        session.accept_open(foreign),
        OpenDisposition::Stale
    ));
    assert_eq!(
        session
            .document()
            .ok_or("missing B")?
            .core()
            .buffer_text()?,
        "B"
    );
    Ok(())
}

#[test]
fn cancelled_job_after_session_drop_has_no_io_or_live_target() -> Result<(), Box<dyn Error>> {
    let opening = {
        let mut session = ReadonlySession::new(limits(4096));
        job(&mut session, "late.md")?
    };
    assert!(opening.is_cancelled());
    let files = Files::bytes(b"late");
    let completion = opening.read_with(&files);
    drop(completion);
    assert_eq!(files.opens.get(), 0);
    Ok(())
}

#[test]
fn open_job_and_completion_support_the_background_send_boundary() {
    fn send<T: Send>() {}
    send::<aline_ui::ReadonlyOpenJob>();
    send::<aline_ui::ReadonlyOpenCompletion>();
}

#[test]
fn cancellation_between_read_chunks_releases_reader_without_publication()
-> Result<(), Box<dyn Error>> {
    struct CancelReader {
        bytes: Cursor<Vec<u8>>,
        target: Rc<RefCell<ReadonlySession>>,
        request: aline_ui::OpenRequestId,
        dropped: Rc<Cell<bool>>,
    }
    impl Read for CancelReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = self.bytes.read(output)?;
            self.target.borrow_mut().cancel_open(&self.request);
            Ok(count)
        }
    }
    impl Drop for CancelReader {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    struct CancelFiles {
        target: Rc<RefCell<ReadonlySession>>,
        request: aline_ui::OpenRequestId,
        dropped: Rc<Cell<bool>>,
    }
    impl ReadonlyFileAccess for CancelFiles {
        type Reader = CancelReader;
        fn open(&self, _path: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                CancelReader {
                    bytes: Cursor::new(b"replacement".to_vec()),
                    target: self.target.clone(),
                    request: self.request.clone(),
                    dropped: self.dropped.clone(),
                },
                11,
                true,
            ))
        }
    }
    let target = Rc::new(RefCell::new(ReadonlySession::new(limits(4096))));
    publish(&mut target.borrow_mut(), b"old")?;
    let request = target.borrow_mut().request_open()?;
    let opening = target
        .borrow_mut()
        .accept_selection(&request, Some(vec![PathBuf::from("cancel.md")]))?
        .ok_or("missing cancellation job")?;
    let dropped = Rc::new(Cell::new(false));
    let files = CancelFiles {
        target: target.clone(),
        request,
        dropped: dropped.clone(),
    };
    let completion = opening.read_with(&files);
    assert!(dropped.get());
    assert!(matches!(
        target.borrow_mut().accept_open(completion),
        OpenDisposition::Stale
    ));
    assert_eq!(
        target
            .borrow()
            .document()
            .ok_or("missing old")?
            .core()
            .buffer_text()?,
        "old"
    );
    Ok(())
}

#[test]
fn document_and_parse_hard_budget_failures_do_not_create_ready_documents()
-> Result<(), Box<dyn Error>> {
    let config = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(1),
            LineIndexBudget::new(4096),
            HistoryBudget::new(128, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    );
    let mut session = ReadonlySession::new(config);
    let completion = job(&mut session, "doc-budget.md")?.read_with(&Files::bytes(b"too large"));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::BudgetExceeded)
    ));
    assert!(session.document().is_none());

    let config = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(4096),
            LineIndexBudget::new(4096),
            HistoryBudget::new(128, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0().with_max_source_bytes(1),
    );
    let mut session = ReadonlySession::new(config);
    let completion = job(&mut session, "parse-budget.md")?.read_with(&Files::bytes(b"too large"));
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::Parse(_))
    ));
    assert!(session.document().is_none());
    Ok(())
}

#[test]
fn oversized_document_text_is_rejected_before_core_line_index() -> Result<(), Box<dyn Error>> {
    let config = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(1),
            LineIndexBudget::new(0),
            HistoryBudget::new(128, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    );
    let mut session = ReadonlySession::new(config);
    let completion =
        job(&mut session, "preflight.md")?.read_with(&Files::bytes(b"large\nsource\n"));
    // 行索引预算故意为零；超大文本必须先被文件会话拒绝，不能到达核心构造。
    assert!(matches!(
        session.accept_open(completion),
        OpenDisposition::Failed(OpenError::BudgetExceeded)
    ));
    assert!(session.document().is_none());
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn fifo_path_is_rejected_with_process_deadline() -> Result<(), Box<dyn Error>> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "aline-readonly-fifo-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory)?;
    let fifo = directory.join("blocked.md");
    let status = Command::new("mkfifo").arg(&fifo).status()?;
    assert!(status.success(), "mkfifo must create the real FIFO fixture");
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "fifo_open_child", "--ignored", "--nocapture"])
        .env("ALINE_READONLY_FIFO_PATH", &fifo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let started = Instant::now();
    let timed_out = loop {
        if child.try_wait()?.is_some() {
            break false;
        }
        if started.elapsed() > Duration::from_secs(5) {
            child.kill()?;
            break true;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output()?;
    std::fs::remove_dir_all(&directory)?;
    assert!(!timed_out, "FIFO open blocked beyond the process deadline");
    assert!(
        output.status.success(),
        "FIFO child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
#[ignore = "由有进程时限的父回归执行"]
fn fifo_open_child() -> Result<(), Box<dyn Error>> {
    let path =
        PathBuf::from(std::env::var_os("ALINE_READONLY_FIFO_PATH").ok_or("missing FIFO path")?);
    let mut session = ReadonlySession::new(limits(4096));
    let request = session.request_open()?;
    let opening = session
        .accept_selection(&request, Some(vec![path]))?
        .ok_or("missing FIFO job")?;
    assert!(matches!(
        session.accept_open(opening.run()),
        OpenDisposition::Failed(_)
    ));
    assert!(session.document().is_none());
    Ok(())
}
