# 任务 4.2 独立复核记录

日期：2026-10-31。复核对象：`<repo>` 分支 `main`，基线 HEAD `24e5d68`，4.2 改动尚未提交。复核方式：独立只读 Explore 复核者，按 AGENTS 路由检查规范一致性、编码语义、API 边界、证据与门禁；复核者不参与实现，结论由本记录转写并逐项处置。

## 1. 复核者实际运行并报告的证据

复核者报告运行了：

- `cargo test -p aline-source --locked --offline`：退出 0，当时 9 个测试。
- `cargo fmt --all --check`：退出 0。
- `cargo clippy --workspace --all-targets -- -D warnings`：退出 0。
- `python3 independent-check.py`：退出 0，当时 `checked=28 ok=25 err=3`。
- 独立复算金样、逐一核对报告所列 SHA-256。

复核结论：1 个阻塞项、1 个应修项、8 条建议。复核者另报告对 `{A, CR, LF, 中}` 长度不超过 8 的全部 174,762 个串做过只读穷举：未发现行尾等价反例；移除守卫会产生 1,941 个反例（如 `"\r\r\n"`）。该穷举是复核者自述证据，未作为仓库证据保存；仓库内以 `guard_rule_roundtrips_all_small_cr_lf_sources` 的 2,186 次往返属性检查补充覆盖。

## 2. 阻塞项及处理

### 2.1 行首正文 U+FEFF 在改后编码中被当作头部 BOM 丢弃

复核者指出：输入 `A␣B`（正文 U+FEFF、无头部 BOM）解码为 `A\u{FEFF}B`；删除 `A` 后冻结源码以 U+FEFF 开头，按“仅重放 BOM”编码不写 BOM，再解码时首个 U+FEFF 被当作头部 BOM 剥离，源码变成 `B`。这违反 change spec “再解码仅 CRLF/LF 表示差异上等价”“其余字符不变”与任务 4.2 “正常操作须可表示、其余字符不丢”，且 4.2 金样未覆盖该改后路径。复核者判定该问题可达、属阻塞，修复前不得勾选 4.2。

处置：按 2026-10-31 用户批准的“保护性 BOM 例外”修复，不采用拒绝保存或解码层剥离首字符的方案。

- 规范：`openspec/changes/m0-engine-foundation/specs/engine/source/spec.md` 的“保真编码与内容身份”增加保护性 BOM 规则与“行首正文 U+FEFF 保护”场景；`docs/engine-design.md` §5.2 增加对应表行与裁决说明。
- 实现：`crates/aline-source/src/encode.rs` 新增 `emits_header_bom`，基线原有 BOM 或冻结源码以 U+FEFF 开头时只写一个头部 BOM；预算计入该 BOM；不改写源码，不声称基线原本有 BOM。成功写盘后的新基线记为 `has_bom=true` 属 4.3/4.4，本任务只产出 bytes。
- 金样：新增 `leading-feff-changed-no-bom`、`leading-feff-inserted`、`leading-feff-changed-with-bom` 与错误行 `leading-feff-over-budget`。
- 参考实现与测试：`independent-check.py` 的参考编码器按同一规则补写 BOM；`encode_golden.rs` 的 BOM 断言改为“输入有 BOM 或改后冻结源码以 U+FEFF 开头”，并新增行首 FEFF 逆补丁场景。
- 复核证据：金样 38 例（34 ok、4 err）独立复核通过；`cargo test -p aline-source` 11 个测试通过；直接门禁与统一入口在 2026-10-31T03:32:32Z 起全部退出 0。证据见 `report.md` §4、§6 与 `gates/`、`workspace-gates/`。

## 3. 应修项及处理

### 3.1 报告引用了不存在的 `review.md`

复核者指出 `report.md` 状态行引用 `review.md`，但当时 step-4.2 目录没有该文件。处置：本记录即为该文件；`report.md` 证据清单已加入“独立复核记录：`review.md`”。已解决。

## 4. 建议及处理

| 建议 | 处置 |
|---|---|
| 1. 守卫对抗性覆盖，避免实现与参考脚本共享同一邻接盲点 | 金样新增 `guard-cr-run-one`、`guard-cr-run-two`、`guard-crlf-then-crlf`、`guard-crlf-then-lone-crlf`；新增 2,186 次 encode→decode 属性检查 `guard_rule_roundtrips_all_small_cr_lf_sources` |
| 2. 逆补丁矩阵补 `boundary-delete-cr`、`paste-isolated-cr-into-crlf` 与 CRLF 策略场景 | 三个场景均加入 `inverse_patches_restore_exact_source_and_original_bytes`，另加行首 FEFF 逆补丁；共 9 个场景 |
| 3. 补 `bom-crlf-changed`、`nul-changed` 与行首 FEFF 改后成功用例 | 全部加入金样；行首 FEFF 另有 BOM 已存在与预算错误用例 |
| 4. `LengthOverflow` 可达性与报告措辞 | `EncodeError::LengthOverflow` 文档标注为防御性分支；报告 §5 明确本任务错误证据只覆盖 `BudgetExceeded` |
| 5. 3.3 错误类别映射 | `encode.rs` 模块文档写明：预算→`BudgetExceeded`，编码长度失败→防御性 `LengthOverflow`，证据不匹配按构造不可达 |
| 6. `SourceBaseline::new` 不校验三部分一致 | `baseline.rs` 类型文档固定不变量：`source` 必须恰为 `storage.bytes()` 的解码结果；`new` 保持 `pub(crate)`，4.3/4.4 增加成功写盘基线时沿用该不变量并在测试中固定 |
| 7. 公开面不可伪造性缺少自动测试 | 新增 `public_encoding_types_are_send_and_sync` 静态断言；未增加 compile-fail 测试，因不引入 `trybuild` 等新依赖；外部不可构造性由 `pub(crate)` 可见性保证。`public-api` 门禁仍只是夹具 self-test，报告已限定 |
| 8. 门禁脚本措辞 | `run-direct-gates.sh` 注释改为“不修改代码与规范文档，仅写入本任务证据日志” |

## 5. 复核者已核实且未发现问题的部分

复核者报告确认：原 bytes 通道由同一基线上的逐字比较决定，不存在调用方 `unchanged: bool`，`boundary-delete-cr` 证明结果 bytes 与输入相同仍 `reused_original=false`；两条路径都先统计完整字节（含 BOM）再比较预算、再分配；等号边界接受，三个既有预算错误的 `needed/limit` 与独立复算一致（含 `expansion-over-budget` 的 `needed=13`）；CRLF 对/未配对 LF 单元、多数派与平票 LF、CRLF 策略、孤立 CR 保留、LF 策略守卫均正确；CR-X-LF 删除、保留例外、边界插入/删除、粘贴均有冻结源码精确值、编码、再解码限定行尾等价与逆补丁测试；不增删末尾换行；多字节 UTF-8 不被拆分；`aline-source` 无依赖、无 GPUI/线程/全局状态、无 `unwrap/expect/panic!/unreachable!/unsafe/todo!`；`DecodedSource` 公开面无回归；报告 §6 的 SHA-256 与当时文件逐一匹配；报告未声称 4.3/4.4、V-01～V-09 或 M0 出口通过，`tasks.md` 4.2 当时未勾选。

## 6. 复核后重跑的证据

修复后重跑（UTC）：

- 直接门禁：2026-10-31T03:32:32Z～03:32:33Z，8 项全部 `exit=0`；`cargo test -p aline-source` 11 passed；`cargo test --workspace` 29 passed；`fmt`/`clippy` 退出 0（`gates/commands.log` 与各 stdout/stderr 日志）。
- 独立金样复核：2026-10-31T03:31:27Z，`checked=38 ok=34 err=4`，退出 0（`independent-check.command.log`、`independent-check.stdout.log`）。
- 统一入口：`scripts/check_workspace.py` 全部检查退出 0，证据在 `workspace-gates/`。

## 7. 结论

阻塞项已按用户批准的规范裁决修复并补入金样、参考实现与测试；应修项已解决；建议 1～8 已逐项处置或记录不采用理由。修复后没有未处理的复核阻塞项，4.2 可如实勾选。本记录不把 4.3/4.4 的原子写盘、基线发布或 V-01～V-09/M0 出口写成已通过。
