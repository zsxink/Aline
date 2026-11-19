//! 显式启用的 M2 操作取证：按固定间隔采样真实文档状态，关闭时只新建证据文件。
//!
//! 目的：为真实 macOS 中文操作矩阵逐项对照源码 bytes、选区、草稿、版本与历史。
//! 采样只读文档状态，不执行导航、点击、编辑或文件 I/O；磁盘读取只在窗口关闭
//! 后的后台任务中进行。正常启动无此状态；证据目录必须已存在于 `logs/validation`
//! 内，且全部输出文件只创建新文件、拒绝覆盖。
//!
//! 采集不替代验收：它只记录真实回调造成的状态，不判断行为是否正确。
//! 记录使用 TSV + 十六进制文本，避免 JSON 转义与直出用户正文的编码歧义；
//! 文本超过单条上限时只记录长度并留空，读取方据此判定“未完整记录”。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{DocumentId, EditorMode, ReadonlyDocument, WaitStatus};

/// 状态采样间隔；只影响证据粒度，不驱动任何文档行为。
pub(super) const SAMPLE_TICK: Duration = Duration::from_millis(200);

/// 单条记录允许内联的文本字节上限；超过时只记长度，避免状态采样无界增长。
const RECORD_TEXT_LIMIT: usize = 4096;

/// 采样记录上限；达到后停止追加并留下 `truncated` 标记。
const RECORD_LIMIT: usize = 8192;

/// 采样不覆盖的超大文档阈值；超过时跳过全文快照类判断，避免每 tick 复制全文。
const SNAPSHOT_TEXT_LIMIT: usize = 256 * 1024;

/// 窗口层持有的证据采集器；关闭时交给后台写盘。
pub(super) struct M2Evidence {
    directory: PathBuf,
    started: Instant,
    records: Vec<Record>,
    next_seq: u64,
    document_seq: u64,
    document: Option<DocumentId>,
    truncated: bool,
    closed: bool,
}

struct Record {
    seq: u64,
    elapsed_ms: u128,
    state: State,
}

struct State {
    identity: u64,
    mode: &'static str,
    revision: u64,
    text_len: usize,
    text_hex: String,
    selection_anchor: usize,
    selection_head: usize,
    selection_reversed: bool,
    open_block: bool,
    draft_len: usize,
    draft_hex: String,
    composition: Option<(usize, usize, usize, String)>,
    undo_depth: usize,
    redo_depth: usize,
    history_nodes: usize,
    parse_current: bool,
    editable: bool,
    wait: &'static str,
    save: &'static str,
    dirty: &'static str,
    autosave_paused: bool,
    close: &'static str,
    external: &'static str,
}

impl State {
    fn fingerprint(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.identity,
            self.mode,
            self.revision,
            self.selection_anchor,
            self.selection_head,
            self.selection_reversed,
            self.open_block,
            self.draft_hex,
            self.composition
                .as_ref()
                .map(|(start, end, len, hex)| format!("{start}:{end}:{len}:{hex}"))
                .unwrap_or_default(),
            self.undo_depth,
            self.redo_depth,
            self.history_nodes,
            self.parse_current,
            self.editable,
            self.wait,
            self.save,
            self.dirty,
            self.autosave_paused,
            self.close,
            self.external,
            self.text_hex,
        )
    }
}

/// 关闭时冻结的证据包；真实磁盘 bytes 由后台读取后写入。
pub(super) struct FinishPacket {
    directory: PathBuf,
    records: Vec<Record>,
    truncated: bool,
    document: DocumentSnapshot,
}

pub(super) struct DocumentSnapshot {
    name: Option<String>,
    path: PathBuf,
    source: Vec<u8>,
    baseline_storage: Vec<u8>,
}

impl M2Evidence {
    /// 仅当显式设置 `ALINE_M2_EVIDENCE_DIR` 且目录位于仓库 `logs/validation`
    /// 内时启用；与自动探针互斥，避免程序化动作被误认为真实输入。
    pub(super) fn from_env() -> Option<Self> {
        let directory = std::env::var_os("ALINE_M2_EVIDENCE_DIR")?;
        if std::env::var_os("ALINE_M1_PROBE_SPEC").is_some() {
            eprintln!("m2-evidence rejected: automatic probe enabled");
            return None;
        }
        let directory = PathBuf::from(directory).canonicalize().ok()?;
        let root = std::env::current_dir().ok()?.join("logs/validation");
        if !directory.is_dir() || !directory.starts_with(root.canonicalize().ok()?) {
            eprintln!("m2-evidence rejected: invalid evidence directory");
            return None;
        }
        Some(Self {
            directory,
            started: Instant::now(),
            records: Vec::new(),
            next_seq: 0,
            document_seq: 0,
            document: None,
            truncated: false,
            closed: false,
        })
    }

    /// 采样一次当前状态；状态无变化时不追加记录。
    pub(super) fn sample(&mut self, document: &mut ReadonlyDocument) {
        if self.closed || self.truncated || self.records.len() >= RECORD_LIMIT {
            self.truncated = true;
            return;
        }
        let id = document.core().document_id();
        if self.document.as_ref() != Some(id) {
            self.document = Some(id.clone());
            self.document_seq = self.document_seq.saturating_add(1);
        }
        let identity = self.document_seq;
        let Some(state) = capture(document, identity) else {
            return;
        };
        if self
            .records
            .last()
            .is_some_and(|record| record.state.fingerprint() == state.fingerprint())
        {
            return;
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.records.push(Record {
            seq,
            elapsed_ms: self.started.elapsed().as_millis(),
            state,
        });
    }

    /// 关闭判定通过后冻结证据包；关闭前的最后一次采样由调用方先经
    /// `document_mut` 完成，本入口只读取不改变文档。
    pub(super) fn closing(&mut self, document: &ReadonlyDocument) -> FinishPacket {
        self.closed = true;
        let source = document
            .core()
            .buffer_text()
            .unwrap_or_default()
            .into_bytes();
        let baseline_storage = document.baseline().storage().bytes().to_vec();
        let name = document
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        FinishPacket {
            directory: self.directory.clone(),
            records: std::mem::take(&mut self.records),
            truncated: self.truncated,
            document: DocumentSnapshot {
                name,
                path: document.path().to_owned(),
                source,
                baseline_storage,
            },
        }
    }
}

impl FinishPacket {
    /// 后台写盘：只创建新文件，磁盘读取失败不伪造成功。
    pub(super) fn write(self) -> io::Result<()> {
        let mut session = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join("session.tsv"))?;
        writeln!(
            session,
            "seq\telapsed_ms\tidentity\tmode\trevision\ttext_len\ttext_hex\t\
             selection_anchor\tselection_head\tselection_reversed\topen_block\t\
             draft_len\tdraft_hex\tcomposition_start\tcomposition_end\t\
             composition_len\tcomposition_hex\tundo_depth\tredo_depth\t\
             history_nodes\tparse_current\teditable\twait\tsave\tdirty\t\
             autosave_paused\tclose\texternal"
        )?;
        for record in &self.records {
            writeln!(session, "{}", record.line())?;
        }
        writeln!(
            session,
            "truncated={}",
            if self.truncated { "true" } else { "false" }
        )?;
        session.sync_all()?;

        let document = self.document;
        let mut final_state = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join("final.tsv"))?;
        writeln!(
            final_state,
            "name={}\tsource_len={}\tstored_len={}",
            document.name.as_deref().unwrap_or("<unnamed>"),
            document.source.len(),
            document.baseline_storage.len()
        )?;
        let disk = fs::read(&document.path).ok();
        if let Some(disk) = &disk {
            writeln!(
                final_state,
                "disk_len={}\tdisk_equals_source={}\tdisk_equals_stored={}",
                disk.len(),
                disk == &document.source,
                disk == &document.baseline_storage
            )?;
        } else {
            writeln!(final_state, "disk_read=failed")?;
        }
        final_state.sync_all()?;

        new_file(&self.directory, "source-after.bin", &document.source)?;
        new_file(
            &self.directory,
            "stored-after.bin",
            &document.baseline_storage,
        )?;
        if let Some(disk) = disk {
            new_file(&self.directory, "disk-after.bin", &disk)?;
        }
        Ok(())
    }
}

impl Record {
    fn line(&self) -> String {
        let state = &self.state;
        let (composition_start, composition_end, composition_len, composition_hex) = state
            .composition
            .as_ref()
            .map(|(start, end, len, hex)| {
                (
                    start.to_string(),
                    end.to_string(),
                    len.to_string(),
                    hex.clone(),
                )
            })
            .unwrap_or_else(|| ("".to_owned(), "".to_owned(), "".to_owned(), String::new()));
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.seq,
            self.elapsed_ms,
            state.identity,
            state.mode,
            state.revision,
            state.text_len,
            state.text_hex,
            state.selection_anchor,
            state.selection_head,
            state.selection_reversed,
            state.open_block,
            state.draft_len,
            state.draft_hex,
            composition_start,
            composition_end,
            composition_len,
            composition_hex,
            state.undo_depth,
            state.redo_depth,
            state.history_nodes,
            state.parse_current,
            state.editable,
            state.wait,
            state.save,
            state.dirty,
            state.autosave_paused,
            state.close,
            state.external,
        )
    }
}

fn capture(document: &mut ReadonlyDocument, identity: u64) -> Option<State> {
    let (
        mode,
        revision,
        text,
        selection,
        open_block,
        draft,
        composition,
        undo_depth,
        redo_depth,
        history_nodes,
        wait,
    ) = {
        let core = document.core();
        let text = core.buffer_text().ok()?;
        let selection = core.current_selection();
        let draft = core.open_block_draft().unwrap_or_default().to_owned();
        let composition = core.composition().map(|view| {
            let range = view.range();
            (
                range.start,
                range.end,
                view.candidate().len(),
                bounded_hex(view.candidate().as_bytes()),
            )
        });
        let wait = match core.wait().map(|wait| wait.status()) {
            None => "idle",
            Some(WaitStatus::Pending) => "pending",
            Some(WaitStatus::Cancelled) => "cancelled",
        };
        (
            match core.mode() {
                EditorMode::Visual => "visual",
                EditorMode::Source => "source",
            },
            core.version().revision().value(),
            text,
            selection,
            core.has_open_block(),
            draft,
            composition,
            core.undo_depth(),
            core.redo_depth(),
            core.history_node_count(),
            wait,
        )
    };
    let parse_current = document.is_parse_current();
    let editable = document.is_editable();
    let save = document.evidence_save_status();
    let large = text.len() > SNAPSHOT_TEXT_LIMIT;
    // 脏判断与关闭判定都可能生成当前快照；超大文档在采样路径上明确跳过。
    let dirty = if large {
        "skipped-large"
    } else {
        match document.evidence_dirty() {
            Some(true) => "true",
            Some(false) => "false",
            None => "unknown",
        }
    };
    let close = if large {
        "skipped-large"
    } else {
        document.evidence_close_label()
    };
    let autosave_paused = document.autosave_paused();
    let external = document.evidence_external_label();
    Some(State {
        identity,
        mode,
        revision,
        text_len: text.len(),
        text_hex: bounded_hex(text.as_bytes()),
        selection_anchor: selection.anchor(),
        selection_head: selection.head(),
        selection_reversed: selection.is_reversed(),
        open_block,
        draft_len: draft.len(),
        draft_hex: bounded_hex(draft.as_bytes()),
        composition,
        undo_depth,
        redo_depth,
        history_nodes,
        parse_current,
        editable,
        wait,
        save,
        dirty,
        autosave_paused,
        close,
        external,
    })
}

fn bounded_hex(bytes: &[u8]) -> String {
    if bytes.len() > RECORD_TEXT_LIMIT {
        return String::new();
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

fn new_file(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::{bounded_hex, new_file};

    #[test]
    fn bounded_hex_encodes_lowercase_and_skips_oversized_text() {
        assert_eq!(bounded_hex("你好".as_bytes()), "e4bda0e5a5bd");
        assert_eq!(bounded_hex(b""), "");
        let oversized = vec![0u8; super::RECORD_TEXT_LIMIT + 1];
        assert_eq!(bounded_hex(&oversized), "");
    }

    #[test]
    fn evidence_files_refuse_overwrite() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "aline-m2-evidence-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        std::fs::create_dir(&root)?;
        new_file(&root, "session.tsv", b"first")?;
        assert!(new_file(&root, "session.tsv", b"second").is_err());
        assert_eq!(std::fs::read(root.join("session.tsv"))?, b"first");
        std::fs::remove_dir_all(root)
    }
}
