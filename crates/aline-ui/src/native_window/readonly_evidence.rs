//! 显式启用的只读操作取证；窗口只收集真实回调与状态，关闭后在后台写证据。
//!
//! 不执行导航、点击或编辑，不替代验收。正常启动无此状态；证据目录须已存在于
//! logs/validation 内。只创建新文件，拒绝覆盖；磁盘后读和编码均在关闭后的任务中。

use std::cell::RefCell;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::{EditorMode, ReadonlyDocument};

pub(super) type SharedEvidence = Rc<RefCell<ReadonlyEvidence>>;

pub(super) struct ReadonlyEvidence {
    directory: PathBuf,
    next: u64,
    active: Option<Active>,
}

struct Active {
    name: String,
    path: PathBuf,
    opened: Vec<u8>,
    source_before: Vec<u8>,
    revision_before: u64,
    document: crate::DocumentId,
    initial_state: String,
    events: Vec<String>,
}

pub(super) struct ClosedEvidence {
    directory: PathBuf,
    active: Active,
    source_after: Vec<u8>,
    baseline: aline_source::SourceBaseline,
    state: String,
}

fn new_file(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?;
    file.write_all(bytes)
}

impl ReadonlyEvidence {
    pub(super) fn from_env() -> Option<SharedEvidence> {
        let directory = std::env::var_os("ALINE_M1_READONLY_EVIDENCE_DIR")?;
        // 自动探针与真实回调取证不得混用，避免程序化动作被误认成平台输入。
        if std::env::var_os("ALINE_M1_PROBE_SPEC").is_some() {
            eprintln!("m1-readonly-evidence rejected: automatic probe enabled");
            return None;
        }
        let directory = PathBuf::from(directory).canonicalize().ok()?;
        let root = std::env::current_dir().ok()?.join("logs/validation");
        if !directory.is_dir() || !directory.starts_with(root.canonicalize().ok()?) {
            eprintln!("m1-readonly-evidence rejected: invalid evidence directory");
            return None;
        }
        Some(Rc::new(RefCell::new(Self {
            directory,
            next: 0,
            active: None,
        })))
    }

    pub(super) fn opened(&mut self, document: &ReadonlyDocument) -> Result<(), String> {
        if self.active.is_some() {
            return Err("previous evidence document was not closed".to_owned());
        }
        self.next = self.next.checked_add(1).ok_or("evidence id exhausted")?;
        let source = document
            .core()
            .buffer_text()
            .map_err(|error| format!("{error:?}"))?;
        self.active = Some(Active {
            name: format!("case-{:03}", self.next),
            path: document.path().to_owned(),
            opened: document.baseline().storage().bytes().to_vec(),
            source_before: source.into_bytes(),
            revision_before: document.core().version().revision().value(),
            document: document.core().document_id().clone(),
            initial_state: format!(
                "has_open_block_before={}\thas_draft_before={}\thas_composition_before={}\tundo_depth_before={}\tredo_depth_before={}\thistory_len_before={}",
                document.core().has_open_block(),
                document.core().open_block_draft().is_some(),
                document.core().composition().is_some(),
                document.core().undo_depth(),
                document.core().redo_depth(),
                document.core().history_node_count()
            ),
            events: vec!["open".to_owned()],
        });
        Ok(())
    }

    /// 仅由真实平台输入的成功回调调用；不从计划、render 或程序化探针推断。
    pub(super) fn event(&mut self, action: &str) {
        if let Some(active) = &mut self.active {
            // 此入口仅服务小型人工夹具；溢出明确留下错误，不能伪装完整序列。
            if active.events.len() < 256 {
                active.events.push(action.to_owned());
            } else if active
                .events
                .last()
                .is_none_or(|event| event != "event-limit-exceeded")
            {
                active.events.push("event-limit-exceeded".to_owned());
            }
        }
    }

    pub(super) fn switching(&mut self, before: EditorMode, after: EditorMode) {
        match (before, after) {
            (EditorMode::Visual, EditorMode::Source) => self.event("visual_to_source"),
            (EditorMode::Source, EditorMode::Visual) => self.event("source_to_visual"),
            _ => {}
        }
    }

    pub(super) fn closing(
        &mut self,
        document: &ReadonlyDocument,
    ) -> Result<ClosedEvidence, String> {
        let source = document
            .core()
            .buffer_text()
            .map_err(|error| format!("{error:?}"))?;
        if self
            .active
            .as_ref()
            .is_none_or(|active| &active.document != document.core().document_id())
        {
            return Err("evidence document identity mismatch".to_owned());
        }
        let active = self.active.take().ok_or("missing open evidence")?;
        let core = document.core();
        let state = format!(
            "revision_before={}\trevision_after={}\thas_open_block={}\thas_draft={}\thas_composition={}\tundo_depth={}\tredo_depth={}\thistory_len={}\tsource_bytes_equal={}\n",
            active.revision_before,
            core.version().revision().value(),
            core.has_open_block(),
            core.open_block_draft().is_some(),
            core.composition().is_some(),
            core.undo_depth(),
            core.redo_depth(),
            core.history_node_count(),
            active.source_before == source.as_bytes()
        );
        Ok(ClosedEvidence {
            directory: self.directory.clone(),
            active,
            source_after: source.into_bytes(),
            baseline: document.baseline().clone(),
            state,
        })
    }
}

impl ClosedEvidence {
    /// 只能在只读会话确已关闭后交给后台任务；保存前后独立字节而非仅存自报哈希。
    pub(super) fn write(mut self, document_closed: bool) -> io::Result<()> {
        self.active.events.push("close".to_owned());
        let output = self.directory.join(&self.active.name);
        fs::create_dir(&output)?;
        let disk = fs::read(&self.active.path)?;
        let source = std::str::from_utf8(&self.source_after).map_err(io::Error::other)?;
        let stored = aline_source::encode(
            source,
            &self.baseline,
            super::StorageBudget::new(super::MAX_DOCUMENT_BYTES),
        )
        .map_err(io::Error::other)?;
        new_file(&output, "opened.bin", &self.active.opened)?;
        new_file(&output, "source-before.bin", &self.active.source_before)?;
        new_file(&output, "source-after.bin", &self.source_after)?;
        new_file(&output, "stored.bin", stored.bytes())?;
        new_file(&output, "disk-after.bin", &disk)?;
        let record = format!(
            "{}\n{}document_closed={}\tdisk_bytes_equal={}\tsteps={}\n",
            self.active.initial_state,
            self.state,
            document_closed,
            disk == self.active.opened,
            self.active.events.join(",")
        );
        new_file(&output, "state.tsv", record.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_files_refuse_overwrite_and_hard_links() -> io::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "aline-evidence-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        fs::create_dir(&root)?;
        new_file(&root, "original", b"original")?;
        assert!(new_file(&root, "original", b"changed").is_err());
        fs::hard_link(root.join("original"), root.join("linked"))?;
        assert!(new_file(&root, "linked", b"changed").is_err());
        assert_eq!(fs::read(root.join("original"))?, b"original");
        fs::remove_dir_all(root)
    }
}
