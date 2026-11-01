# R06 交接

## 状态

- 产品改动：`scripts/check_safety.py`（唯一文件）。`scripts/safety_lex.py` 未改。
- 未提交；由主 Agent 串行提交。建议：`fix(workspace): 安全扫描拒绝目录外 target 与无效根`。
- 验证：`--self-test` exit 0、passed=true、60/60；正常仓库 exit 0、`scanned=89`、覆盖八成员。

## 修了什么

1. F-09：扫描集合改由 `cargo metadata --no-deps` 的实际 `targets[].src_path` 驱动；
   落在成员目录外的 target / `build` 脚本一律 `external-target-path` 拒绝（detail 带 kind + 原始路径，
   argv/stdout/stderr 在 `member_reads`）。成员内符号链接逃逸同样拒绝。
2. F-15：根必须存在、是目录、含 `Cargo.toml`、且 `workspace_root` 等于该根；
   `root-missing` / `root-not-workspace` / `workspace-metadata-failure`。
3. 八成员集合固定为常量；缺目录/缺清单 `missing-member`、未声明 `member-not-in-workspace`、
   身份不符 `member-package-identity`、越界包 `unrecognized-member-path`。
4. `scanned` 为空 → `empty-scan`（零命中不再等于通过）。

## 复核要点

- 两个既有 case 期望从 `['member-package-identity']` 增强为
  `['member-package-identity','missing-member']`（工作区只能有一份 `aline-source`，规范目录必然缺失）。
  既有 53 项无删除、无削弱。
- 新旧脚本对正常仓库的 `(file, kind)` 集合完全一致（89 条，差集空）——范围无缩小、无漂移。
- 成员内 `build.rs` 现被扫为 `engine-production`（范围扩大，预期内）。
- 目录外 target 只拒绝、不受审（契约二选一中的"明确拒绝"）。

## 未覆盖

依赖树、过程宏/别名、`include!` 片段、非 `*.rs` 入口、自动向上寻找 workspace 根。

## 证据

- `pre-fix.txt`：修前两 bug 实测（第 5/6 节含 `zz_probe_seg.rs`，为他人临时探针，主 Agent 已删，与 R06 无关）。
- `post-fix.txt`：修后拒绝 + 正常仓库 + 自测 + 4 个附加反例。
- `commands.txt`：argv / cwd / 起止时间 / exit / CARGO_HOME 探测结论。
- `report.md` **未落盘**（worker 环境禁止写报告类文件）：根因、规则表、逐条断言对照、未覆盖范围
  见本次 worker 最终回复正文，请主 Agent 需要时转存。
