# R10 派发契约：F-06 独立预期与探针身份勘误

基线 HEAD：派发时 `git rev-parse HEAD`（须晚于 R02/R03/R07/R09 的提交）。仓库根 `/Users/xian/Project/editor/Aline`。前置 R02、R03、R07、R09 已落地。

## 文件所有权（只有你可写这些文件）

- `docs/validation/m0-engine-foundation/remediation/R10/run-1/` 下你新建的全部证据与脚本
- `docs/validation/m0-engine-foundation/group-6/step-6.4/report.md`（**只许追加勘误段**，不得修改任何既有行）
- `docs/validation/m0-engine-foundation/group-6/step-6.4/goldens-check/`（只许新增文件，不得改动 `probe-main.rs`、`gap-probe-main.rs`、`probe-Cargo.toml`、既有 `*.log`）

**不得写** `crates/**` 任何源码、`scripts/**`、`openspec/**`、任何历史 `report.md`（除上面指定的追加）。不得 `git reset`、不得回退他人改动。

## 失败事实（F-06，阻断，维度 D2/D5）

`docs/validation/m0-engine-foundation/group-6/step-6.4/report.md:60` 原文：

> 这些值与 `tests/mapping.rs` 的断言一致，属于"独立工具复算同一期望"，不是由被测实现反向生成。

经我本轮直接核验，`goldens-check/probe-main.rs` 第 2 行的导入是：

```rust
use aline_markdown::{ParseBudget, ParseConfig, parse};
```

即该「独立工具」**调用的就是被测实现本身**（`parse` / `segments()` / `source_at`），只是换了个仓外宿主工程。因此：

- 它能证明的只有「同一实现在不同宿主下结果一致」，**不能**证明「期望被独立推导」；
- `report.md:22` 已承认旧 TSV 的生成方式无法追认（mtime 08:52:17Z 早于消费者 09:31:07Z，但 08:48 已存在修复前解析器），这一段是正确的，不得改动；
- 但 `:60` 的「独立工具复算」表述与该探针的实际身份矛盾，是**事实性错误**，必须勘误。

## 目标

1. **勘误（追加，不改原文）**：在 `step-6.4/report.md` **末尾**追加一节 `## 6. 勘误：2026-11-XX（R10）`，内容必须包含：
   - 引用 `:60` 原句；
   - 指出 `probe-main.rs` 直接导入 `aline_markdown::parse`，因此是「同实现的第二宿主」，不是独立推导；
   - 明确该节此前的证据能支持什么、不能支持什么；
   - 指向本批新建的真实独立推导与原始输出路径；
   - 明确「不追认旧 TSV 的创作方式，不补造先行时间」。
2. **保全真实的独立推导**：新建一个**不调用 `aline_markdown` 任何解析/映射 API** 的独立实现，对 R09 的 `basic.md` 夹具（或 R09 认定的等价夹具）按 M0 映射规则**从原始字节**重算：
   - 允许读取夹具原始字节（`std::fs`）、允许用 `pulldown-cmark`（它是第三方输入而非被测实现）；
   - **禁止** `use aline_markdown::*` 的任何解析、投影、映射函数——这是本批唯一硬门槛；
   - 输出与 R09 dump 同形状（`segment`/`drawn`/`identity` 行）的独立结果；
   - 与 R09 实现输出**逐行 diff**，差异条数与每条差异的性质写入报告。
   - 实现语言不限（Rust 仓外工程或 Python 均可），但必须落盘源码与原始输出，不得只写结论。
3. **输入身份**：记录夹具的 SHA-256 与字节长度，以及你所用工具链版本。不得用指针地址或时间作为身份。
4. **区分三类证据**（报告里必须分节，不能混为一谈）：
   - **独立推导**：不调用被测实现，从字节与规范重算；
   - **第二宿主复算**：同一实现、不同宿主（即既有 probe，其价值仅限于「宿主无关」）；
   - **实现自证**：实现输出本身（不可作为期望来源）。
5. **不造历史**：不得声称新推导「早于实现」；不得修改旧 TSV 或旧日志的 mtime/内容；不得把本次推导写成原复核时就有。所有新证据标注为 2026-11-XX 的本轮产物。

## 适用规范

- `AGENTS.md` 铁律（证据不得补造历史）、`docs/development-process.md` 的证据与勘误规则。
- `docs/maintenance/m0-remediation/tasks.md` §5（证据目录、只追加勘误、不得覆盖旧日志）。
- `docs/engine-design.md` §7 映射规则（作为独立推导的依据来源）。

## 约束

- 全部 Cargo 命令 `--locked --offline`、`CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`；若仓外工程需要初始 lock，单列登记为「initial-lock-generation」。工具缺失/离线失败记「未核实」，不联网安装。
- 不得为通过而放宽 R09 的断言或 dump 格式；发现 R09 实现与你的独立推导不一致时，登记 `待裁决` 并附最小复现，不要改 R09 的代码（不在你的所有权内）。
- 提交（Conventional Commits，scope `workspace`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R10/run-1/`）

- `independent/`（独立实现源码 + 其 `Cargo.toml`/脚本）
- `independent-output.txt`（独立实现的原始 stdout，argv/cwd/时间/exit 一并记录）
- `impl-output.txt`（R09 实现的同夹具输出，同命令口径）
- `diff.txt`（两者逐行 diff 的原始结果）
- `fixture-identity.txt`（SHA-256、字节长度、工具链版本）
- `commands.txt`（argv、cwd、起止时间、exit code）
- `report.md`（三类证据分节、差异逐条性质、勘误链接、未覆盖项）
- `handoff.md`（≤80 行）

## 返回给我（≤800 中文字）

status、提交哈希、独立实现是否真的不 import 被测 crate（如何证明）、差异条数、勘误追加位置与行数、concerns、证据路径。不要贴长日志。
