//! 虚拟阅读范围、布局身份和高度缓存的独立无窗口预期。

#![cfg(target_os = "macos")]

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use aline_ui::{
    DocLimits, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits, ReadonlySession,
};
pub use aline_ui::{DocumentId, EditorMode, ReadonlyDocument, WorkingVersion};

#[path = "../src/native_window/readonly_layout.rs"]
mod readonly_layout;
use readonly_layout::{
    ItemLayoutBudget, LayoutCache, LayoutEnvironment, LayoutError, LayoutKey, ReaderIndex,
    RestoreState, initial_view,
};

struct Files<'a>(&'a [u8]);
impl<'a> ReadonlyFileAccess for Files<'a> {
    type Reader = Cursor<&'a [u8]>;
    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0),
            self.0.len() as u64,
            true,
        ))
    }
}
fn session(bytes: &[u8]) -> Result<ReadonlySession, Box<dyn Error>> {
    let max = 1024 * 1024;
    let mut session = ReadonlySession::new(ReadonlyOpenLimits::new(
        StorageBudget::new(max),
        DocLimits::new(
            TextBudget::new(max),
            LineIndexBudget::new(100_000),
            HistoryBudget::new(0, 0),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    ));
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
        .ok_or("job")?;
    session.accept_open(job.read_with(&Files(bytes)));
    Ok(session)
}
fn environment(width: f32) -> Result<LayoutEnvironment, Box<dyn Error>> {
    Ok(LayoutEnvironment::new(width, 2.0, 1, true, 16.0, 24.0)?)
}

#[test]
fn source_rows_keep_crlf_isolated_cr_and_terminal_empty_line() -> Result<(), Box<dyn Error>> {
    let session = session("中\r\n\tlast\rpart\n".as_bytes())?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100)?;
    assert_eq!(index.row_count(EditorMode::Source), 3);
    let expected = [(0..3, "中"), (5..15, "\tlast\rpart"), (16..16, "")];
    for (row, (source, text)) in expected.into_iter().enumerate() {
        assert_eq!(
            index
                .source_range(document.parsed(), EditorMode::Source, row)?
                .byte_range(),
            source
        );
        assert_eq!(
            index
                .projection(document.parsed(), EditorMode::Source, row, 4096)?
                .drawn_text(),
            text
        );
    }
    Ok(())
}

#[test]
fn visual_rows_partition_source_and_reveal_source_anchor_after_reflow() -> Result<(), Box<dyn Error>>
{
    let session = session(b"# Head\n\nfirst\n\nlast")?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100)?;
    let count = index.row_count(EditorMode::Visual);
    assert!(count >= 3);
    let mut source_end = 0;
    for row in 0..count {
        let range = index
            .source_range(document.parsed(), EditorMode::Visual, row)?
            .byte_range();
        assert_eq!(range.start, source_end);
        source_end = range.end;
    }
    assert_eq!(source_end, 19);
    let anchor = index.anchor(document.parsed(), EditorMode::Source, 4, 7.0)?;
    let visual_row = index.row_for_anchor(document.parsed(), EditorMode::Visual, &anchor)?;
    assert_eq!(
        index
            .projection(document.parsed(), EditorMode::Visual, visual_row, 4096)?
            .drawn_text(),
        "last"
    );
    assert_eq!(anchor.viewport_delta(), 7.0);
    Ok(())
}

#[test]
fn width_scale_font_wrap_and_projection_identity_invalidate_heights() -> Result<(), Box<dyn Error>>
{
    let a = session(b"same")?;
    let b = session(b"same")?;
    let a = a.document().ok_or("a")?;
    let b = b.document().ok_or("b")?;
    let mut cache = LayoutCache::new(2);
    let key = LayoutKey::new(a, EditorMode::Visual, environment(600.0)?);
    assert!(cache.prepare(key.clone())?);
    let token = cache.token().ok_or("token")?;
    assert!(cache.record(token, 0, 48.0));
    assert!(cache.matches_key(&key));
    assert!(!cache.prepare(key)?);
    assert_eq!(cache.height(0), Some(48.0));
    for key in [
        LayoutKey::new(a, EditorMode::Visual, environment(400.0)?),
        LayoutKey::new(
            a,
            EditorMode::Visual,
            LayoutEnvironment::new(400.0, 1.0, 1, true, 16.0, 24.0)?,
        ),
        LayoutKey::new(
            a,
            EditorMode::Visual,
            LayoutEnvironment::new(400.0, 1.0, 2, true, 16.0, 24.0)?,
        ),
        LayoutKey::new(
            a,
            EditorMode::Visual,
            LayoutEnvironment::new(400.0, 1.0, 2, false, 16.0, 24.0)?,
        ),
        LayoutKey::new(a, EditorMode::Source, environment(400.0)?),
        LayoutKey::new(b, EditorMode::Visual, environment(400.0)?),
    ] {
        assert!(cache.prepare(key)?);
        assert_eq!(cache.height(0), None);
        assert!(!cache.record(token, 0, 999.0));
    }
    Ok(())
}

#[test]
fn cache_is_bounded_and_stable_measurement_does_not_request_new_layout()
-> Result<(), Box<dyn Error>> {
    let session = session(b"line")?;
    let document = session.document().ok_or("document")?;
    let mut cache = LayoutCache::new(2);
    cache.prepare(LayoutKey::new(
        document,
        EditorMode::Source,
        environment(600.0)?,
    ))?;
    let token = cache.token().ok_or("token")?;
    assert!(cache.record(token, 0, 24.0));
    assert!(!cache.record(token, 0, 24.0));
    assert!(cache.record(token, 1, 96.0));
    assert_eq!(cache.height(0), Some(24.0));
    assert_eq!(cache.height(1), Some(96.0));
    for row in 2..1000 {
        cache.record(token, row, 24.0);
        assert!(cache.len() <= 2);
    }
    assert_eq!(cache.height(0), None);
    assert!(!cache.record(token, 1000, f32::NAN));
    assert!(!cache.record(token, 1000, -1.0));
    assert_eq!(cache.len(), 2);
    Ok(())
}

#[test]
fn stale_anchors_and_layout_budget_are_rejected() -> Result<(), Box<dyn Error>> {
    let a = session(b"a\nb")?;
    let b = session(b"a\nb")?;
    let a = a.document().ok_or("a")?;
    let b = b.document().ok_or("b")?;
    assert!(ReaderIndex::new(a, 1).is_err());
    let index = ReaderIndex::new(a, 100)?;
    let anchor = index.anchor(a.parsed(), EditorMode::Source, 1, 0.0)?;
    assert!(
        index
            .row_for_anchor(b.parsed(), EditorMode::Source, &anchor)
            .is_err()
    );
    assert!(
        index
            .projection(a.parsed(), EditorMode::Source, 1, 0)
            .is_err()
    );
    assert!(LayoutEnvironment::new(0.0, 2.0, 1, true, 16.0, 24.0).is_err());
    Ok(())
}

#[test]
fn initial_source_policy_uses_strict_four_mib_source_byte_threshold() {
    let threshold = 4 * 1024 * 1024;
    for bytes in [0, threshold - 1, threshold] {
        assert_eq!(initial_view(bytes), (EditorMode::Visual, None));
    }
    assert_eq!(
        initial_view(threshold + 1),
        (EditorMode::Source, Some("reading.large_source"))
    );
}

#[test]
fn long_source_line_is_readable_through_checked_unicode_safe_fragments()
-> Result<(), Box<dyn Error>> {
    let text = format!("{}中{}", "a".repeat(4095), "tail".repeat(20_000));
    let session = session(text.as_bytes())?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100_000)?;
    assert!(index.row_count(EditorMode::Source) > 2);
    let mut restored = String::new();
    let mut source_end = 0;
    for row in 0..index.row_count(EditorMode::Source) {
        let source = index.source_range(document.parsed(), EditorMode::Source, row)?;
        assert_eq!(source.byte_range().start, source_end);
        source_end = source.byte_range().end;
        assert!(source.len_bytes() <= 4096);
        let projection = index.projection(document.parsed(), EditorMode::Source, row, 4096)?;
        restored.push_str(projection.drawn_text());
    }
    assert_eq!(source_end, text.len());
    assert_eq!(restored, text);
    assert_eq!(
        index
            .source_range(document.parsed(), EditorMode::Source, 0)?
            .byte_range(),
        0..4095
    );
    assert_eq!(
        index
            .source_range(document.parsed(), EditorMode::Source, 1)?
            .byte_range()
            .start,
        4095
    );
    Ok(())
}

#[test]
fn giant_visual_item_is_rejected_before_projection_and_source_remains_accessible()
-> Result<(), Box<dyn Error>> {
    let text = "a".repeat(65_537);
    let session = session(text.as_bytes())?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100_000)?;
    assert!(matches!(
        index.preflight(
            document.parsed(),
            EditorMode::Visual,
            0,
            ItemLayoutBudget::new(65_536, 4096, 256)
        ),
        Err(LayoutError::ItemBytes {
            needed: 65_537,
            limit: 65_536
        })
    ));
    assert!(matches!(
        index.projection(document.parsed(), EditorMode::Visual, 0, 65_536),
        Err(LayoutError::ItemBytes { .. })
    ));
    assert!(
        index
            .projection(document.parsed(), EditorMode::Source, 0, 4096)
            .is_ok()
    );
    assert!(matches!(
        index.preflight(
            document.parsed(),
            EditorMode::Visual,
            0,
            ItemLayoutBudget::new(100_000, 1, 256)
        ),
        Err(LayoutError::ItemNodes { .. })
    ));
    Ok(())
}

#[test]
fn wide_table_column_budget_is_independent_from_small_text_bytes() -> Result<(), Box<dyn Error>> {
    let session = session(b"|a|b|c|\n|-|-|-|\n|x|y|z|\n")?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100)?;
    assert!(matches!(
        index.preflight(
            document.parsed(),
            EditorMode::Visual,
            0,
            ItemLayoutBudget::new(65_536, 4096, 2)
        ),
        Err(LayoutError::TableColumns {
            needed: 3,
            limit: 2
        })
    ));
    assert!(
        index
            .preflight(
                document.parsed(),
                EditorMode::Visual,
                0,
                ItemLayoutBudget::new(65_536, 4096, 3)
            )
            .is_ok()
    );
    Ok(())
}

#[test]
fn internal_source_anchor_survives_mode_width_and_font_layout_keys() -> Result<(), Box<dyn Error>> {
    let text = format!("prefix {} tail", "word ".repeat(3000));
    let session = session(text.as_bytes())?;
    let document = session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100_000)?;
    let visual = index.projection(document.parsed(), EditorMode::Visual, 0, 65_536)?;
    let anchor = index.anchor_from_local(
        document.parsed(),
        &visual,
        10_007,
        aline_buffer::Affinity::Before,
        -5.0,
    )?;
    assert_eq!(anchor.source().byte_offset(), 10_007);
    assert_eq!(anchor.affinity(), aline_buffer::Affinity::Before);
    assert_ne!(
        anchor.source().byte_offset(),
        index
            .source_range(document.parsed(), EditorMode::Visual, 0)?
            .byte_range()
            .start
    );
    let mut cache = LayoutCache::new(8);
    cache.prepare(LayoutKey::new(
        document,
        EditorMode::Visual,
        environment(600.0)?,
    ))?;
    for (mode, environment) in [
        (EditorMode::Source, environment(300.0)?),
        (
            EditorMode::Visual,
            LayoutEnvironment::new(800.0, 2.0, 2, true, 20.0, 30.0)?,
        ),
    ] {
        cache.prepare(LayoutKey::new(document, mode, environment))?;
        let row = index.row_for_anchor(document.parsed(), mode, &anchor)?;
        let projection = index.projection(document.parsed(), mode, row, 65_536)?;
        let local = projection.drawn_at(anchor.source(), aline_buffer::Affinity::Before)?;
        let rebound = projection.source_at(local.value(), aline_buffer::Affinity::Before)?;
        assert_eq!(rebound.value(), anchor.source());
        assert_eq!(anchor.viewport_delta(), -5.0);
    }
    Ok(())
}

#[test]
fn deferred_anchor_restore_rejects_old_tokens_documents_and_cancellation()
-> Result<(), Box<dyn Error>> {
    let a = session(b"first\nsecond")?;
    let b = session(b"first\nsecond")?;
    let a = a.document().ok_or("a")?;
    let b = b.document().ok_or("b")?;
    let index = ReaderIndex::new(a, 100)?;
    let anchor = index.anchor(a.parsed(), EditorMode::Source, 1, 3.0)?;
    let mut cache = layout_cache_for_restore(a)?;
    let old_layout = cache.token().ok_or("old layout")?;
    let mut restores = RestoreState::new(
        a.core().document_id().clone(),
        a.parsed().snapshot().id().clone(),
    );
    let old_request = restores.begin(anchor.clone(), 1, old_layout)?;
    cache.prepare(LayoutKey::new(a, EditorMode::Source, environment(300.0)?))?;
    let layout = cache.token().ok_or("layout")?;
    let request = restores.begin(anchor, 1, layout)?;
    assert!(!restores.cancel(&old_request));
    assert!(
        restores
            .take_for_row(&old_request, a.core().document_id(), layout, 1)
            .is_none()
    );
    assert!(
        restores
            .take_for_row(&request, b.core().document_id(), layout, 1)
            .is_none()
    );
    assert!(
        restores
            .take_for_row(&request, a.core().document_id(), old_layout, 1)
            .is_none()
    );
    assert!(
        restores
            .take_for_row(&request, a.core().document_id(), layout, 0)
            .is_none()
    );
    assert!(restores.pending().is_some());
    assert!(restores.schedule_for_row(&request, a.core().document_id(), layout, 1));
    assert!(!restores.schedule_for_row(&request, a.core().document_id(), layout, 1));
    assert!(restores.cancel(&request));
    assert!(
        restores
            .take_for_row(&request, a.core().document_id(), layout, 1)
            .is_none()
    );
    let request = restores.begin(
        index.anchor(a.parsed(), EditorMode::Source, 1, 3.0)?,
        1,
        layout,
    )?;
    let restored = restores
        .take_for_row(&request, a.core().document_id(), layout, 1)
        .ok_or("restore")?;
    assert_eq!(restored.source().byte_offset(), 6);
    assert!(!restores.cancel_current());
    assert!(
        restores
            .take_for_row(&request, a.core().document_id(), layout, 1)
            .is_none()
    );
    Ok(())
}
fn layout_cache_for_restore(document: &ReadonlyDocument) -> Result<LayoutCache, Box<dyn Error>> {
    let mut cache = LayoutCache::new(8);
    cache.prepare(LayoutKey::new(
        document,
        EditorMode::Source,
        environment(600.0)?,
    ))?;
    Ok(cache)
}

#[test]
fn exact_visual_byte_limit_and_default_table_column_limit_are_enforced()
-> Result<(), Box<dyn Error>> {
    let text = "a".repeat(65_536);
    let bytes_session = session(text.as_bytes())?;
    let document = bytes_session.document().ok_or("document")?;
    let index = ReaderIndex::new(document, 100_000)?;
    assert_eq!(
        index
            .projection(document.parsed(), EditorMode::Visual, 0, 65_536)?
            .drawn_text()
            .len(),
        65_536
    );
    for columns in [256, 257] {
        let row = format!("|{}|\n", vec!["a"; columns].join("|"));
        let delimiter = format!("|{}|\n", vec!["---"; columns].join("|"));
        let text = format!("{row}{delimiter}{row}");
        let table_session = session(text.as_bytes())?;
        let document = table_session.document().ok_or("table")?;
        let index = ReaderIndex::new(document, 100_000)?;
        let projected = index.projection(document.parsed(), EditorMode::Visual, 0, 65_536);
        if columns == 256 {
            assert!(projected.is_ok());
        } else {
            assert!(matches!(
                projected,
                Err(LayoutError::TableColumns {
                    needed: 257,
                    limit: 256
                })
            ));
        }
        assert!(
            index
                .projection(document.parsed(), EditorMode::Source, 0, 4096)
                .is_ok()
        );
    }
    Ok(())
}
