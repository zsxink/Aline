# R06 修复报告：`scripts/check_safety.py`（F-09 / F-15）

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`（开工时）。唯一代码改动：
`scripts/check_safety.py`（+321 / −122）。`scripts/safety_lex.py` 未改。
本报告由主 Agent 依据 worker 返回正文转存，并追加主 Agent 独立复核（§7）。

## 1. 根因

### F-09（严重）：目录外 target 不受审

旧 `workspace_scan()` 用 `cargo read-manifest` 只取**包身份**（`name`），随后仍然
`directory.rglob('*.rs')` 遍历成员目录下的源文件。Cargo 合法但落在成员目录外的入口
——`[lib] path = "../../outside.rs"`、`build = "../../outside-build.rs"`——虽然出现在
`targets[].src_path` / `custom-build` 里，却从未被打开扫描。

后果：该文件可写生产 `unwrap()` 并带局部 `#![allow(clippy::unwrap_used)]`，
**同时躲过词法扫描与 Clippy deny**，而守卫输出 `failures=[]`、exit 0。
实测旧脚本在该夹具上 `scanned=8`、`member_reads=8`（read-manifest 已看见 custom-build），
但 `outside.rs` 不在 `scanned` 里。

### F-15（一般）：零命中等于通过

根不存在时 `--root /不存在路径` 走 `root/'crates'.glob('*')` → 空目录列表 →
`failures=[]`、`scanned=[]`、exit 0。空扫描与「真的没问题」不可区分；
根是目录但没有 `Cargo.toml` 时同样静默通过。

## 2. 规则设计

### 2.1 扫描范围改由 Cargo 实际 targets 驱动

```python
argv = ['cargo', 'metadata', '--no-deps', '--format-version', '1',
        '--manifest-path', str(manifest), '--locked', '--offline']
```

每个成员的 `targets[].src_path` 逐条校验：

| 情形 | 规则 | 处置 |
|------|------|------|
| `src_path` 缺失 | `target-path-missing` | 拒绝 |
| 真实路径落在成员目录外 | `external-target-path` | 拒绝（detail 带 kind + 原始 src_path） |
| 声明路径不存在 | `target-path-missing` | 拒绝 |
| 成员内符号链接指向目录外 | `external-target-path` | 拒绝（rustc 会编译它，等同目录外 target） |

判定用 `os.path.realpath` 比较，因此 `../../outside.rs` 这类相对写法与符号链接逃逸
都不能绕过。契约给了「受审或明确拒绝」二选一，本批选**明确拒绝**（fail-closed）：
不受审就不会误判 kind，也不会把目录外内容纳入 `scanned` 的覆盖面统计。

### 2.2 根与成员集合受检

| 情形 | 规则 |
|------|------|
| 根不存在 | `root-missing` |
| 根无 `Cargo.toml` | `root-not-workspace` |
| `workspace_root` 不等于该根，或根自身是成员包 | `root-not-workspace` |
| `cargo metadata` 失败 / JSON 非法 | `workspace-metadata-failure` |
| 八成员目录或清单缺失 | `missing-member` |
| 成员清单存在但 Cargo 不认它为成员 | `member-not-in-workspace` |
| 成员包名与目录身份不符 | `member-package-identity` |
| `crates/` 下有清单但不在固定八成员 | `unrecognized-member-path` |
| `scanned` 为空 | `empty-scan` |

`CANONICAL_MEMBERS` 固定为 `app`（包名 `aline`）+ 七个 `crates/*`，不再由
`glob` 结果反向推断「有哪些成员」。

### 2.3 self-test 夹具移出仓库受跟踪路径

旧版把 case 夹具与 fake root 直接写进 `docs/validation/.../safety-fixtures/`（受跟踪）。
新版统一在 `tempfile.mkdtemp(prefix='aline-safety-self-test-')` 下构造，
`fixture_root` 写入 `summary.json`，仓库树不再被自测污染。

## 3. 逐条断言对照（自证有效性）

新增 7 项 case，期望**先手写**再跑实现：

| case | 构造 | 期望规则 |
|------|------|----------|
| `external-lib-target-path` | buffer `[lib] path = "../../outside.rs"`（内含 unwrap） | `external-target-path` |
| `external-build-script-path` | buffer `build = "../../outside-build.rs"`（内含 unwrap） | `external-target-path` |
| `member-not-in-workspace` | 成员目录与清单齐备，但未写进 `workspace.members` | `member-not-in-workspace` |
| `root-missing` | `--root` 指向不存在路径 | `root-missing` |
| `root-is-member-manifest` | `--root` 指向成员清单而非 workspace 根 | `root-not-workspace` |
| `root-not-workspace` | 根是目录但无 `Cargo.toml` | `root-not-workspace` |
| `empty-scan` | `members = []` 的空 workspace | `empty-scan` + `missing-member` |

主 Agent 变异自证（4 处，均变红，副本在 `/tmp/r06main/mut/`）：

| 变异 | 变红的 case |
|------|-------------|
| 目录外 target 检查改 `if False and ...` | `external-lib-target-path`、`external-build-script-path` |
| `empty-scan` 判断改 `if False and ...` | `empty-scan` |
| `root-not-workspace` 判断改 `if False:` | `root-is-member-manifest` |
| `missing-member` 规则改名 | `wrong-package-in-known-ui-path`、`quoted-package-and-description-decoy`、`empty-scan` |

### 两个既有 case 的期望被「加强」

`wrong-package-in-known-ui-path` 与 `quoted-package-and-description-decoy` 原期望
`['member-package-identity']`，现为 `['member-package-identity','missing-member']`。

理由：这两例把**真实的** `aline-source` 包放进 `crates/aline-ui`。一个 workspace 里
包名 `aline-source` 只能出现一次，因此规范目录 `crates/aline-source` 必然缺失，
`missing-member` 是同一夹具的必然结论而非新增缺陷。这是**加强**（多一条必报规则），
不是删除或放宽；既有 53 项一条未删、未削弱。

## 4. 验证结果

| 项 | 结果 |
|----|------|
| 目录外 `[lib] path` 夹具 | 修复前 exit 0 / `failures=[]`；修复后 exit 1 / `external-target-path` |
| 目录外 build script | 修复前 exit 0 / `failures=[]`；修复后 exit 1 / `external-target-path`（kind `custom-build`） |
| 根不存在 | 修复前 exit 0 / 全空；修复后 exit 1 / `root-missing` |
| 根无 `Cargo.toml` | 修复前 exit 0；修复后 exit 1 / `root-not-workspace` |
| 正常仓库 | exit 0、`failures=[]`、`scanned=89`、覆盖八成员 |
| `--self-test` | exit 0、`passed=true`、60/60（基线 53 一条未减 + 新增 7） |
| 新旧扫描集合对照 | 89 条 `(file, kind)`，差集为空（范围无缩小、kind 未漂移） |
| 成员内 `build.rs` | 现被扫为 `engine-production`（范围扩大，预期内） |

## 5. 未覆盖范围 / 已知限制

1. 目录外 target 只**拒绝**、不受审：`outside.rs` 里的 unwrap 不会被报告，但退出码
   已确定性非零，不会静默通过。
2. 依赖树、过程宏与别名、`include!` 片段、非 `*.rs` 入口不在扫描范围（既有边界，
   与本批无关）。
3. 不自动向上寻找 workspace 根：`--root` 必须是 workspace 根。
4. 成员内 `build.rs` 纳入 `engine-production` 扫描，属范围扩大；若未来 build 脚本
   需要测试豁免，要显式声明。
5. `cargo metadata --no-deps` 在空 registry 的隔离 `CARGO_HOME` 下仍成功
   （`--no-deps` 不解析依赖）；若将来去掉 `--no-deps`，会退化为
   `workspace-metadata-failure`（fail-closed），属产品行为。
6. 零 Rust 改动，未跑 `cargo test` / `cargo build`。

## 6. 证据索引

- `pre-fix.txt` —— 修复前实际输出（含 `zz_probe_seg.rs` 说明，见 §7 第 3 条）
- `post-fix.txt` —— 修复后各命令输出；主 Agent 复核追加在「[追加]」段
- `commands.txt` —— argv / cwd / 起止 UTC / exit code / CARGO_HOME 探测
- 原始输出：`/tmp/r06main/`（主 Agent 复核）、`/tmp/r06-*`（worker）

## 7. 主 Agent 独立复核（追加，不改上文）

复核于 2026-11-01 独立执行，不采信 worker 自述：

1. **自测**：`python3 scripts/check_safety.py --self-test --output-dir /tmp/r06main`
   start `2026-11-01T06:47:01Z` / end `06:47:06Z`，exit 0，`cases=60`、`passed=true`、
   `failed cases=[]`、`workspace_after_fixtures failures=0`、`scanned=89`。
   基线计数独立复算：`git show HEAD:scripts/check_safety.py` 导出副本跑 `--self-test`
   → exit 0、`cases=53`、`passed=true`。故 60 = 53 + 7，既有项无删减。
2. **扫描范围无漂移**：旧脚本 `--root .` → `scanned=89`；新脚本 → `scanned=89`；
   `(file, kind)` 集合差集**双向为空**，`failures` 均为 0。
3. **F-09 自建夹具**（不复用 worker 的 `/tmp/aline-m0-independent-review.pTFf8u`）：
   在 `/tmp/r06main/fx/ext-target` 按八成员形状重建，`crates/aline-buffer/Cargo.toml`
   写 `[lib] path = "../../outside.rs"`，`outside.rs` 内含 `v.unwrap()`。
   旧脚本 exit 0、`failures=[]`、`scanned=8`（静默 fail-open）；
   新脚本 exit 1、`external-target-path`、detail `lib .../crates/aline-buffer/../../outside.rs`。
4. **F-15 自建夹具**：`--root /tmp/r06main/fx/nope` → exit 1 `root-missing`；
   `--root <成员清单目录>` → exit 1 `root-not-workspace`，
   detail `workspace_root=... root_is_package=True`。
5. **变异自证**：§3 表格 4 处变异全部使 self-test exit 1 并命中预期 case。
6. **关于 `pre-fix.txt` 第 5/6 节的 `zz_probe_seg.rs`**：那是 R09 期间的临时探针
   （`crates/aline-markdown/examples/zz_probe_seg.rs`），已删除，与 R06 无关；
   worker 已在原文件就地注明，`cases=53` 的计数不受影响（探针是被扫文件、不是 case）。
   干净树重采 exit 0。
7. **残留（不阻断）**：目录外 target 只拒绝不受审；`build.rs` 扫描范围扩大；
   两个既有 case 期望加强为 `member-package-identity + missing-member`（复核者需裁决
   这是否可接受——主 Agent 判断为同一夹具的必然结论，非放宽）。
