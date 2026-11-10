//! M1 验证探针：默认关闭，由四个显式环境变量同时启用。
//!
//! 在真实原生窗口内按固定序列重放只读阅读生命周期，把分段计时、虚拟化规模
//! 与内存可写回字节写成可复跑的原始证据。探针不参与业务判定、不写用户文档、
//! 不新增依赖、不新增公开 API；缺少任一环境变量时生产行为完全不变。
//!
//! 输出是制表符分隔的 `key=value` 行，由 `scripts/collect_m1_readonly.py`
//! 组装成 `run_m1_readonly.py` 需要的 JSONL/JSON；这里不做结果判定。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Context, Window};

use crate::EditorMode;
use crate::document::{OpenTimings, StageTimer, StageTiming};

use super::{CloseDocument, NativeDocumentWindow, ReaderSurface};

/// 探针循环让出主线程的最小间隔；真实帧仍由 GPUI 事件循环驱动。
pub(super) const PROBE_TICK: Duration = Duration::from_millis(2);

/// 探针步骤：每步触发一次真实窗口帧，部分步骤对应 V-05 的阶段。
#[derive(Clone, Copy)]
struct Step {
    action: Action,
    stage: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum Action {
    Present,
    Scroll,
    Hit,
    Jump,
    Source,
    Visual,
}

const STEPS: [Step; 6] = [
    Step {
        action: Action::Present,
        stage: Some("first_present"),
    },
    Step {
        action: Action::Scroll,
        stage: Some("scroll"),
    },
    Step {
        action: Action::Hit,
        stage: None,
    },
    Step {
        action: Action::Jump,
        stage: None,
    },
    Step {
        action: Action::Source,
        stage: Some("mode_switch"),
    },
    Step {
        action: Action::Visual,
        stage: None,
    },
];

/// 这里只记录真实已执行的自动动作；像素命中与目录/脚注激活仍须真实 UI 事件。
const BYTE_STEPS: &str = "open,scroll,visual_to_source,source_to_visual,close";

/// 仅向验证目录创建新文件，拒绝覆盖已有文件、符号链接或夹具的硬链接。
fn write_evidence(path: &Path, contents: &[u8]) -> io::Result<()> {
    let root = std::env::current_dir()?.join("logs/validation");
    write_evidence_under(&root, path, contents)
}

fn write_evidence_under(root: &Path, path: &Path, contents: &[u8]) -> io::Result<()> {
    let root = root.canonicalize()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("缺少证据目录"))?;
    if !parent.canonicalize()?.starts_with(root) {
        return Err(io::Error::other("证据目录不在 logs/validation 内"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents)
}

/// 单个采样用例；条件/冷热/编号只用于证据分组，不参与业务。
#[derive(Clone)]
struct ProbeCase {
    name: String,
    condition: String,
    temperature: String,
    sample_id: u64,
    path: PathBuf,
    suite: Suite,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Suite {
    Performance,
    Bytes,
    Warmup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Start,
    WaitingRead,
    WaitingFrame,
    Done,
}

/// 窗口内唯一的探针状态；不设全局状态，随窗口实体一起释放。
pub(super) struct ProbeState {
    cases: Vec<ProbeCase>,
    index: usize,
    phase: Phase,
    step: usize,
    lines: Vec<String>,
    errors: Vec<String>,
    out: PathBuf,
    done: PathBuf,
    memory_dir: PathBuf,
    background: OpenTimings,
    reader_index: StageTiming,
    adopt: StageTiming,
    applied: bool,
    read_done: bool,
    frame_started: Option<StageTimer>,
    frame_done: bool,
    frame_timing: StageTiming,
    frame_rendered: usize,
    revision_before: u64,
    initial_mode: EditorMode,
    open_started: Option<StageTimer>,
    render_started: Option<StageTimer>,
    run_id: String,
    clock_origin: Instant,
    frame_serial: u64,
    frame_scheduled: bool,
    /// 外部 trace 采集器释放后才进入测量序列；未设置时生产行为不变。
    gate: Option<PathBuf>,
    gate_ready: Option<PathBuf>,
    gate_ready_written: bool,
    gate_passed: bool,
    gate_epoch_ns: u128,
}

#[cfg(test)]
fn epoch_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

fn parse_spec(text: &str) -> Result<Vec<ProbeCase>, String> {
    let mut cases = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 6 {
            return Err(format!("spec 第 {} 行字段数不为 6", number + 1));
        }
        let suite = match fields[5] {
            "perf" => Suite::Performance,
            "warmup" => Suite::Warmup,
            "bytes" => Suite::Bytes,
            other => return Err(format!("spec 第 {} 行套件未知: {other}", number + 1)),
        };
        let sample_id = fields[3]
            .parse::<u64>()
            .map_err(|_| format!("spec 第 {} 行编号无效", number + 1))?;
        if fields[0].is_empty() || fields[4].is_empty() {
            return Err(format!("spec 第 {} 行缺少名称或路径", number + 1));
        }
        if !fields[0]
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-' || value == b'_')
        {
            return Err(format!(
                "spec 第 {} 行名称必须是 ASCII 字母数字、短横线或下划线",
                number + 1
            ));
        }
        cases.push(ProbeCase {
            name: fields[0].to_owned(),
            condition: fields[1].to_owned(),
            temperature: fields[2].to_owned(),
            sample_id,
            path: PathBuf::from(fields[4]),
            suite,
        });
    }
    if cases.is_empty() {
        return Err("spec 没有用例".to_owned());
    }
    Ok(cases)
}

impl ProbeState {
    /// 四个环境变量齐全才启用；解析失败也写出可检查的失败证据。
    pub(super) fn from_env() -> Option<Self> {
        let spec = std::env::var_os("ALINE_M1_PROBE_SPEC")?;
        let out = std::env::var_os("ALINE_M1_PROBE_OUT")?;
        let done = std::env::var_os("ALINE_M1_PROBE_DONE")?;
        let memory_dir = std::env::var_os("ALINE_M1_PROBE_MEMORY_DIR")?;
        let text = fs::read_to_string(PathBuf::from(spec)).unwrap_or_default();
        let (cases, errors) = match parse_spec(&text) {
            Ok(cases) => (cases, Vec::new()),
            Err(error) => (Vec::new(), vec![error]),
        };
        eprintln!(
            "m1-probe enabled cases={} errors={}",
            cases.len(),
            errors.len()
        );
        Some(Self {
            cases,
            index: 0,
            phase: Phase::Start,
            step: 0,
            lines: vec![format!(
                "kind=run\trun_id={}\tapp_sha256={}\tspec_sha256={}\tpid={}",
                std::env::var("ALINE_M1_PROBE_RUN_ID").unwrap_or_default(),
                std::env::var("ALINE_M1_PROBE_APP_SHA256").unwrap_or_default(),
                std::env::var("ALINE_M1_PROBE_SPEC_SHA256").unwrap_or_default(),
                std::process::id(),
            )],
            errors,
            out: PathBuf::from(out),
            done: PathBuf::from(done),
            memory_dir: PathBuf::from(memory_dir),
            background: OpenTimings::default(),
            reader_index: StageTiming::default(),
            adopt: StageTiming::default(),
            applied: false,
            read_done: false,
            frame_started: None,
            frame_done: false,
            frame_timing: StageTiming::default(),
            frame_rendered: 0,
            revision_before: 0,
            initial_mode: EditorMode::Visual,
            open_started: None,
            render_started: None,
            run_id: std::env::var("ALINE_M1_PROBE_RUN_ID").unwrap_or_default(),
            clock_origin: Instant::now(),
            frame_serial: 0,
            frame_scheduled: false,
            gate: std::env::var_os("ALINE_M1_PROBE_GATE").map(PathBuf::from),
            gate_ready: std::env::var_os("ALINE_M1_PROBE_GATE_READY").map(PathBuf::from),
            gate_ready_written: false,
            gate_passed: false,
            gate_epoch_ns: 0,
        })
    }

    /// 帧提交后的 effect 回调；这不是 GPU 完成或屏幕扫描输出时间。
    pub(super) fn on_frame(&mut self, serial: u64, rendered: usize) {
        if serial != self.frame_serial || self.frame_done {
            return;
        }
        let Some(started) = self.frame_started.take() else {
            return;
        };
        self.frame_timing = started.finish();
        self.frame_rendered = rendered;
        self.frame_done = true;
    }

    /// 记录后台读取三段、主线程 layout/adopt 的真实耗时与采纳结果。
    pub(super) fn on_read_finished(
        &mut self,
        background: OpenTimings,
        reader_index: StageTiming,
        adopt: StageTiming,
        applied: bool,
    ) {
        self.background = background;
        self.reader_index = reader_index;
        self.adopt = adopt;
        self.applied = applied;
        self.read_done = true;
        if applied {
            self.record_background_stages();
            self.step = 0;
            self.frame_serial = self.frame_serial.saturating_add(1);
            self.frame_scheduled = false;
            self.frame_started = self.open_started.take();
            self.phase = Phase::WaitingFrame;
        }
    }

    /// 第一份待测元素树开始构造；后续重绘不覆盖同一动作的开始时间。
    pub(super) fn begin_render(&mut self) -> Option<u64> {
        if self.frame_started.is_none() || self.frame_done || self.frame_scheduled {
            return None;
        }
        self.frame_scheduled = true;
        self.render_started = Some(StageTimer::start());
        Some(self.frame_serial)
    }

    /// 全部根子节点 prepaint 完成；成本包含 root render、布局与 prepaint。
    pub(super) fn on_layout(&mut self, serial: u64, rendered: usize) {
        if serial != self.frame_serial {
            return;
        }
        let Some(started) = self.render_started.take() else {
            return;
        };
        if self.step == 0
            && let Some(case) = self.cases.get(self.index).cloned()
        {
            self.stage_line(&case, "first_layout", started.finish(), rendered);
        }
    }

    fn case_label(&self) -> String {
        self.cases
            .get(self.index)
            .map(|case| case.name.clone())
            .unwrap_or_else(|| "unknown".to_owned())
    }

    fn error(&mut self, reason: String) {
        let label = self.case_label();
        self.errors.push(reason.clone());
        self.lines
            .push(format!("kind=error\tcase={label}\treason={reason}"));
    }

    fn stage_line(&mut self, case: &ProbeCase, stage: &str, timing: StageTiming, rendered: usize) {
        if case.suite == Suite::Warmup {
            return;
        }
        let Some((started_monotonic_ns, finished_monotonic_ns)) =
            timing.monotonic_bounds(self.clock_origin)
        else {
            self.error(format!("{}: {stage} 缺少真实单调边界", case.name));
            return;
        };
        self.lines.push(format!(
            "kind=stage\trun_id={}\tcase={}\tcondition={}\ttemperature={}\tsample_id={}\tstage={}\tduration_ns={}\tstarted_epoch_ns={}\tepoch_ns={}\tclock_uncertainty_ns={}\ttiming_origin=measured-boundaries\ttiming_clock=instant-monotonic\tstarted_monotonic_ns={started_monotonic_ns}\tfinished_monotonic_ns={finished_monotonic_ns}\tvirtual_items={rendered}",
            self.run_id, case.name, case.condition, case.temperature, case.sample_id, stage,
            timing.duration_ns, timing.started_epoch_ns, timing.finished_epoch_ns,
            timing.clock_uncertainty_ns,
        ));
    }

    /// 后台真实阶段保留各自起止边界，不能以回到主线程的时刻倒推。
    fn record_background_stages(&mut self) {
        let Some(case) = self.cases.get(self.index).cloned() else {
            return;
        };
        for (stage, timing) in [
            ("read_decode", self.background.read_decode),
            ("snapshot", self.background.snapshot),
            ("parse_mapping", self.background.parse_mapping),
            ("main_thread_adopt", self.adopt),
        ] {
            self.stage_line(&case, stage, timing, 0);
        }
        self.diagnostic_line(&case, "reader_index_build_ns", self.reader_index);
        self.diagnostic_line(
            &case,
            "background_dispatch_wait_ns",
            self.background.dispatch_wait,
        );
        self.diagnostic_line(
            &case,
            "main_thread_result_wait_ns",
            self.background.result_wait,
        );
    }

    fn diagnostic_line(&mut self, case: &ProbeCase, metric: &str, timing: StageTiming) {
        let monotonic = timing.monotonic_bounds(self.clock_origin).map_or_else(
            || "timing_clock=unavailable".to_owned(),
            |(started, finished)| format!("timing_clock=instant-monotonic\tstarted_monotonic_ns={started}\tfinished_monotonic_ns={finished}"),
        );
        self.lines.push(format!(
            "kind=diagnostic\trun_id={}\tcase={}\tmetric={metric}\tvalue={}\tstarted_epoch_ns={}\tepoch_ns={}\tclock_uncertainty_ns={}\t{monotonic}",
            self.run_id, case.name, timing.duration_ns, timing.started_epoch_ns,
            timing.finished_epoch_ns, timing.clock_uncertainty_ns,
        ));
    }

    fn record_frame_stage(&mut self) {
        let Some(step) = STEPS.get(self.step) else {
            return;
        };
        let Some(stage) = step.stage else {
            return;
        };
        let Some(case) = self.cases.get(self.index).cloned() else {
            return;
        };
        // effect 只证明平台 draw 提交。首次屏幕呈现必须由外部真实能力补齐，
        // 这里故意不产生 first_present 样本，标准汇总因此保持失败关闭。
        if stage == "first_present" {
            self.diagnostic_line(&case, "first_draw_submission_ns", self.frame_timing);
        } else {
            self.stage_line(&case, stage, self.frame_timing, self.frame_rendered);
        }
    }

    fn write_output(&mut self) {
        for reason in &self.errors {
            self.lines
                .push(format!("kind=error\tcase=none\treason={reason}"));
        }
        self.lines.push("kind=method\tfirst_layout=root-render-through-children-prepaint\tfirst_present=external-trace-display-chain\tfirst_present_fallback=request-through-platform-draw-submission-effect\tscanout_measured=external-trace\tmain_thread_stall=external-trace-runloop-busy\tcold_cache_managed_by=external-collector".to_owned());
        self.lines.push("kind=done".to_owned());
        let content = self.lines.join("\n") + "\n";
        let written = write_evidence(&self.out, content.as_bytes()).is_ok();
        let status = if written && self.errors.is_empty() && !self.cases.is_empty() {
            "ok"
        } else {
            "failed"
        };
        let _ = write_evidence(&self.done, status.as_bytes());
    }

    fn next_case(&mut self) {
        self.index += 1;
        self.step = 0;
        self.frame_done = false;
        self.frame_started = None;
        if self.index >= self.cases.len() {
            self.phase = Phase::Done;
            self.write_output();
        } else {
            self.phase = Phase::Start;
        }
    }
}

impl NativeDocumentWindow {
    /// 验证探针的一次推进；返回 true 表示已写出证据，可以结束循环。
    pub(super) fn probe_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(mut probe) = self.probe.take() else {
            return true;
        };
        let finished = self.probe_tick_with(&mut probe, window, cx);
        self.probe = Some(probe);
        if finished {
            cx.quit();
        }
        finished
    }

    fn probe_tick_with(
        &mut self,
        probe: &mut ProbeState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if probe.phase == Phase::Done {
            return true;
        }
        if probe.cases.is_empty() {
            probe.phase = Phase::Done;
            probe.write_output();
            return true;
        }
        if !probe.gate_passed {
            let gate = probe.gate.clone();
            if let Some(gate) = gate.as_ref() {
                if !probe.gate_ready_written {
                    let ready = probe.gate_ready.clone();
                    if let Some(ready) = ready.as_ref()
                        && let Err(error) = write_evidence(
                            ready,
                            format!("pid={}\n", std::process::id()).as_bytes(),
                        )
                    {
                        probe.error(format!("gate ready 写出失败 {error}"));
                    }
                    probe.gate_ready_written = true;
                }
                if !gate.exists() {
                    return false;
                }
            }
            probe.gate_passed = true;
            let passed = StageTimer::start().finish();
            probe.gate_epoch_ns = passed.started_epoch_ns;
            probe.lines.push(format!(
                "kind=gate\trun_id={}\tpid={}\tepoch_ns={}",
                probe.run_id,
                std::process::id(),
                probe.gate_epoch_ns,
            ));
        }
        match probe.phase {
            Phase::Done => true,
            Phase::Start => {
                let Some(case) = probe.cases.get(probe.index).cloned() else {
                    probe.phase = Phase::Done;
                    probe.write_output();
                    return true;
                };
                probe.open_started = Some(StageTimer::start());
                probe.applied = false;
                probe.read_done = false;
                probe.background = OpenTimings::default();
                probe.reader_index = StageTiming::default();
                probe.adopt = StageTiming::default();
                match self.state.request_open() {
                    Ok(request) => {
                        let job = self
                            .state
                            .accept_dialog(&request, Ok(Some(vec![case.path.clone()])));
                        match job {
                            Some(job) => {
                                self.start_read(job, window, cx);
                                probe.phase = Phase::WaitingRead;
                            }
                            None => {
                                probe.error(format!("{}: 打开请求被拒绝", case.name));
                                probe.next_case();
                            }
                        }
                    }
                    Err(error) => {
                        probe.error(format!("{}: 打开请求失败 {error:?}", case.name));
                        probe.next_case();
                    }
                }
                false
            }
            Phase::WaitingRead => {
                if !probe.read_done {
                    return false;
                }
                if !probe.applied {
                    let label = probe.case_label();
                    probe.error(format!("{label}: 文档未采纳"));
                    probe.next_case();
                    return false;
                }
                // 成功完成回调已在首个内容帧之前切换到 WaitingFrame。
                false
            }
            Phase::WaitingFrame => {
                if !probe.frame_done {
                    return false;
                }
                probe.record_frame_stage();
                probe.frame_done = false;
                probe.step += 1;
                if probe.step < STEPS.len() {
                    self.start_step(probe, window, cx);
                    return false;
                }
                self.probe_emit_bytes(probe);
                self.close_document(&CloseDocument, window, cx);
                probe.next_case();
                false
            }
        }
    }

    /// 触发一步真实交互并等待下一帧；帧耗时含动作、排队与 render 全过程。
    fn start_step(&mut self, probe: &mut ProbeState, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = STEPS.get(probe.step).copied() else {
            return;
        };
        probe.frame_serial = probe.frame_serial.saturating_add(1);
        probe.frame_scheduled = false;
        probe.frame_started = Some(StageTimer::start());
        probe.frame_done = false;
        let height = window.viewport_size().height;
        match step.action {
            Action::Present => {}
            Action::Scroll => {
                if let Some(reader) = &self.reader {
                    reader.page(1.0, height);
                }
            }
            Action::Hit | Action::Jump => {
                if probe
                    .cases
                    .get(probe.index)
                    .is_some_and(|case| case.suite == Suite::Bytes)
                {
                    probe.error(
                        "自动探针未提供真实像素命中与目录/脚注激活事件，不能认定完整阅读步骤通过"
                            .to_owned(),
                    );
                }
            }
            Action::Source => {
                let target = match probe.initial_mode {
                    EditorMode::Visual => EditorMode::Source,
                    EditorMode::Source => EditorMode::Visual,
                };
                self.switch_view(target, cx);
            }
            Action::Visual => self.switch_view(probe.initial_mode, cx),
        }
        cx.notify();
    }

    /// 由 `start_read` 的真实完成回调调用，记录打开分段耗时与采纳结果。
    pub(super) fn probe_read_finished(
        &mut self,
        background: OpenTimings,
        reader_index: StageTiming,
        adopt: StageTiming,
        applied: bool,
    ) {
        if let Some(probe) = &mut self.probe {
            probe.initial_mode = self
                .state
                .session()
                .document()
                .map_or(EditorMode::Visual, |document| document.core().mode());
            probe.revision_before = self
                .state
                .session()
                .document()
                .map(|document| document.core().version().revision().value())
                .unwrap_or(0);
            probe.on_read_finished(background, reader_index, adopt, applied);
        }
    }

    /// 根节点的子节点全部 prepaint 后，读取实际构造的虚拟项数。
    pub(super) fn probe_layout(&mut self, serial: u64) {
        let rendered = self
            .reader
            .as_ref()
            .map_or(0, ReaderSurface::rendered_item_count);
        if let Some(probe) = &mut self.probe {
            probe.on_layout(serial, rendered);
        }
    }

    /// 由 draw/present 所在 effect 周期结束后的回调调用。
    pub(super) fn probe_frame(&mut self, serial: u64) {
        let rendered = self
            .reader
            .as_ref()
            .map_or(0, ReaderSurface::rendered_item_count);
        if let Some(probe) = &mut self.probe {
            probe.on_frame(serial, rendered);
        }
    }

    /// bytes 套件：直接导出当前缓冲区源码，不经过存储编码或行尾重放。
    fn probe_emit_bytes(&mut self, probe: &mut ProbeState) {
        let Some(case) = probe.cases.get(probe.index).cloned() else {
            return;
        };
        let Some(document) = self.state.session().document() else {
            probe.error(format!("{}: 关闭前无文档", case.name));
            return;
        };
        if case.suite != Suite::Warmup
            && let Err(error) = write_evidence(
                &probe.memory_dir.join(format!("{}.opened.bin", case.name)),
                document.baseline().storage().bytes(),
            )
        {
            probe.error(format!("实际打开字节写出失败 {error}"));
        }
        if case.suite != Suite::Bytes {
            return;
        }
        let revision_after = document.core().version().revision().value();
        let has_draft = document.core().open_block_range().is_some()
            || document.core().open_block_draft().is_some()
            || document.core().composition().is_some();
        let history_len = document.core().history_node_count();
        let source = document
            .core()
            .buffer_text()
            .map_err(|error| format!("{error:?}"));
        let memory_file = probe.memory_dir.join(format!("{}.bin", case.name));
        let write_result = source.and_then(|value| {
            write_evidence(&memory_file, value.as_bytes()).map_err(|error| format!("{error:?}"))
        });
        match write_result {
            Ok(()) => probe.lines.push(format!(
                "kind=bytes\tcase={}\tsteps={}\tmemory_file={}\thas_draft={}\thistory_len={}\trevision_before={}\trevision_after={}",
                case.name,
                BYTE_STEPS,
                format_args!("{}.bin", case.name),
                has_draft,
                history_len,
                probe.revision_before,
                revision_after
            )),
            Err(error) => probe.error(format!("{}: 内存字节写出失败 {error}", case.name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_rejects_existing_files_and_outside_directory() -> io::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("aline-probe-{}-{}", std::process::id(), epoch_ns()));
        let root = directory.join("evidence");
        fs::create_dir_all(&root)?;
        let output = root.join("sample");
        write_evidence_under(&root, &output, b"original")?;
        assert!(write_evidence_under(&root, &output, b"replacement").is_err());
        assert_eq!(fs::read(&output)?, b"original");
        let linked = root.join("linked");
        fs::hard_link(&output, &linked)?;
        assert!(write_evidence_under(&root, &linked, b"replacement").is_err());
        assert_eq!(fs::read(&output)?, b"original");
        assert!(write_evidence_under(&root, &directory.join("outside"), b"x").is_err());
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn spec_rejects_unknown_suite_and_invalid_sample() {
        assert!(parse_spec("a|32KiB|cold|1|sample.md|unknown").is_err());
        assert!(parse_spec("a|32KiB|cold|bad|sample.md|perf").is_err());
        assert!(parse_spec("").is_err());
        assert!(parse_spec("../outside|32KiB|cold|1|sample.md|perf").is_err());
        assert!(parse_spec("a|32KiB|cold|1|sample.md|perf").is_ok());
    }

    fn probe() -> ProbeState {
        ProbeState {
            cases: vec![ProbeCase {
                name: "sample".to_owned(),
                condition: "32KiB".to_owned(),
                temperature: "cold".to_owned(),
                sample_id: 1,
                path: PathBuf::from("sample.md"),
                suite: Suite::Performance,
            }],
            index: 0,
            phase: Phase::WaitingRead,
            step: 0,
            lines: Vec::new(),
            errors: Vec::new(),
            out: PathBuf::new(),
            done: PathBuf::new(),
            memory_dir: PathBuf::new(),
            background: OpenTimings::default(),
            reader_index: StageTiming::default(),
            adopt: StageTiming::default(),
            applied: false,
            read_done: false,
            frame_started: None,
            frame_done: false,
            frame_timing: StageTiming::default(),
            frame_rendered: 0,
            revision_before: 0,
            initial_mode: EditorMode::Visual,
            open_started: Some(StageTimer::start()),
            render_started: None,
            run_id: std::env::var("ALINE_M1_PROBE_RUN_ID").unwrap_or_default(),
            clock_origin: Instant::now(),
            frame_serial: 0,
            frame_scheduled: false,
            gate: None,
            gate_ready: None,
            gate_ready_written: false,
            gate_passed: true,
            gate_epoch_ns: 0,
        }
    }

    #[test]
    fn first_content_frame_is_armed_before_render_and_stale_callbacks_are_ignored()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut probe = probe();
        probe.on_read_finished(
            OpenTimings {
                read_decode: StageTimer::start().finish(),
                snapshot: StageTimer::start().finish(),
                parse_mapping: StageTimer::start().finish(),
                ..OpenTimings::default()
            },
            StageTimer::start().finish(),
            StageTimer::start().finish(),
            true,
        );
        assert_eq!(probe.phase, Phase::WaitingFrame);
        assert!(probe.frame_started.is_some());
        let serial = probe.begin_render().ok_or("frame was not armed")?;
        assert!(probe.begin_render().is_none());
        probe.on_layout(serial + 1, 99);
        assert!(
            !probe
                .lines
                .iter()
                .any(|line| line.contains("stage=first_layout"))
        );
        probe.on_layout(serial, 7);
        assert!(probe.lines.iter().any(|line| {
            line.contains("stage=first_layout") && line.contains("virtual_items=7")
        }));
        probe.on_frame(serial + 1, 99);
        assert!(!probe.frame_done);
        probe.on_frame(serial, 7);
        assert!(probe.frame_done);
        assert_eq!(probe.frame_rendered, 7);
        Ok(())
    }

    #[test]
    fn byte_steps_do_not_claim_unprovided_ui_events() {
        let steps: Vec<_> = BYTE_STEPS.split(',').collect();
        assert!(!steps.contains(&"hit"));
        assert!(!steps.contains(&"jump"));
    }
}
