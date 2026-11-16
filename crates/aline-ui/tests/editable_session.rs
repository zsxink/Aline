//! M2 打开会话启用统一文档核心，同时保留 M1 只读构造路径。

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use aline_buffer::{HistoryBudget, LineIndexBudget, SrcSelection, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use aline_ui::{
    DocLimits, EditorMode, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits,
    ReadonlySession,
};

struct Files(&'static [u8]);

impl ReadonlyFileAccess for Files {
    type Reader = Cursor<&'static [u8]>;

    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0),
            self.0.len() as u64,
            true,
        ))
    }
}

fn limits(editable: bool) -> ReadonlyOpenLimits {
    let limits = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(4096),
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    );
    if editable {
        limits.with_editing()
    } else {
        limits
    }
}

fn open(editable: bool, bytes: &'static [u8]) -> Result<ReadonlySession, Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits(editable));
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
        .ok_or("missing open job")?;
    session.accept_open(job.read_with(&Files(bytes)));
    Ok(session)
}

#[test]
fn editable_open_uses_the_shared_document_core() -> Result<(), Box<dyn Error>> {
    let mut session = open(true, b"abc")?;
    let document = session.document_mut().ok_or("missing document")?;
    assert!(document.is_editable());
    assert_eq!(document.core().history_node_count(), 0);
    document.switch_mode(EditorMode::Source)?;
    let current = document.current_snapshot()?;
    let range = current.text().src_range(0..current.text().len_bytes())?;
    assert_eq!(
        current
            .text()
            .slice(&range, TextBudget::new(4096))?
            .as_ref(),
        "abc"
    );
    assert_eq!(document.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn default_open_limits_keep_the_m1_readonly_contract() -> Result<(), Box<dyn Error>> {
    let mut session = open(false, b"abc")?;
    let document = session.document_mut().ok_or("missing document")?;
    assert!(!document.is_editable());
    document.switch_mode(EditorMode::Source)?;
    assert_eq!(document.core().buffer_text()?, "abc");
    assert_eq!(document.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn visual_navigation_opens_and_reopens_only_current_parsed_blocks() -> Result<(), Box<dyn Error>> {
    let mut session = open(true, b"abc\n\nxyz")?;
    let document = session.document_mut().ok_or("missing document")?;
    let first = document.parsed().blocks().first().ok_or("first block")?;
    let second = document.parsed().blocks().get(1).ok_or("second block")?;
    let snapshot = document.parsed().snapshot();
    let first_range = first.range().byte_range();
    let second_range = second.range().byte_range();
    let first_caret = SrcSelection::new(
        snapshot.src_offset(first_range.start)?,
        snapshot.src_offset(first_range.start)?,
    )?;
    let second_caret = SrcSelection::new(
        snapshot.src_offset(second_range.start)?,
        snapshot.src_offset(second_range.start)?,
    )?;

    let identity = document.input_identity()?;
    document.navigate(&identity, &first_caret)?;
    let identity = document.input_identity()?;
    assert!(document.open_visual_at(&identity, &first_caret)?);
    assert!(document.core().has_open_block());
    let current = document.current_snapshot()?;
    assert_eq!(
        document
            .core()
            .visual_block_range(&current)?
            .ok_or("extent")?
            .byte_range(),
        first_range
    );

    let identity = document.input_identity()?;
    document.navigate(&identity, &second_caret)?;
    let identity = document.input_identity()?;
    assert!(document.open_visual_at(&identity, &second_caret)?);
    let current = document.current_snapshot()?;
    assert_eq!(
        document
            .core()
            .visual_block_range(&current)?
            .ok_or("extent")?
            .byte_range(),
        second_range
    );
    Ok(())
}
