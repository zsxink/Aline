# 4.3 存储往返与宿主基线模拟

状态：4.3 的无头测试、独立金样、API 反例和本机门禁已执行；独立只读复核未发现阻塞。仅覆盖纯转换与测试内宿主模拟，不代表真实文件 I/O、原子替换、保存协调器、Buffer/Undo 历史、完整 AC、V-01～V-09 或 M0 出口通过。

## 边界与独立预期

- 先手写 `roundtrip-golden.v1.txt` 的 16 组输入、源码补丁、冻结源码、待写 bytes 和重开源码；依据 PRD SAFE-01～03、引擎 §5、source delta 和本次已确认的孤立 CR 与保护性 BOM 规则推导，非从 Rust 实现输出录制。`baseline-scenarios.v1.txt` 在测试前手写宿主状态迁移与预期。
- `independent-check.py` 独立复算金样、补丁及逆补丁、编码、保护性 BOM 和限定行尾等价；成功 16 例。金样和实现可能共享对规范的理解，独立复核仍需审查具体字节；这是有限夹具而非穷尽所有 Unicode。
- `crates/aline-source/tests/roundtrip.rs` 消费金样并检查未修改逐字节、指定范围之外的精确源码、局部修改后编码与重开、精确逆补丁及 Undo 回尚未替代的原基线。含混合行尾、孤立 CR、CR-X-LF、保护性 BOM。
- 测试内宿主持有基线、当前源码、已成功写出的冻结输入事实；准备和失败不替换基线，成功仅按模拟实际写出的 bytes 解码建立新基线，较新输入保留。实际 bytes 与准备结果不符的反例不将冻结文本冒充已保存；保护性 BOM 成功后新基线 `has_bom=true`。
- 新基线替代后，Undo 旧源码仅按新基线重新编码，不能恢复旧原 bytes；重新读取即使相同 bytes 也有新身份，旧存储范围不能借新身份使用。存储行尾等价不放行原 bytes 通道。
- `fixtures/api-valid.rs` 为真实公共 API 正例；`unchanged-bool.rs` 和 `old-equality.rs` 分别验证调用方不能传入自报 bool 或把旧 `EncodedBytes` 当作当前基线。它们由独立的 `check-api-fixtures.py` 执行，不属于 `roundtrip` 测试过滤本身。后者分别以 E0061、E0308 退出 101；脚本确认预期诊断后退出 0。编译反例只证明这些公开签名不能接受对应调用，宿主仍须遵守真实写盘事实的编排协议。

## 实际命令与环境

macOS 27.0.1 (26A434)，Apple arm64，`rustc 1.98.1 (48a229cea 2026-09-01)`，`cargo 1.98.1 (797e8a9bc 2026-08-05)`，host `aarch64-apple-darwin`。`CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。统一执行开始 2026-10-31T03:56:35.172687Z，结束 2026-10-31T03:56:53.922653Z；各条 UTC 起止、命令、环境及退出码见 `gates/*.json`，输出见同名 `.log`。

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `cargo test -p aline-source roundtrip` | 0 | 新增 7 项及既有 guard 1 项通过 |
| `python3 docs/validation/m0-engine-foundation/group-4/step-4.3/independent-check.py` | 0 | 16 例独立核对 |
| `python3 docs/validation/m0-engine-foundation/group-4/step-4.3/check-api-fixtures.py` | 0 | 正例通过；两反例以预期错误拒绝 |
| `cargo fmt --all --check` | 0 | 通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 通过 |
| `cargo test --workspace` | 0 | 通过 |
| `openspec validate m0-engine-foundation --strict --no-interactive` | 0 | change 结构有效；不是 M0 验收 |
| `python3 scripts/check_workspace.py --cargo-home <cargo-home> --output-dir docs/validation/m0-engine-foundation/group-4/step-4.3/workspace-gates` | 0 | 子检查及 self-test 均退出 0，逐项日志见 `workspace-gates/` |

开发中 source 局部 Clippy 首次报告 `chunks_exact_to_as_chunks`，修改测试后重跑退出 0；上表为最终版本执行记录。`--all-targets` 仅覆盖当前宿主，不代表其他操作系统或全部 feature 组合。`public-api` 守卫是既有夹具自测，不代表扫描全部公开 API。

## 复核与后续

`review.md` 保存独立只读复核结论及边界。4.3 不增加生产 API；4.1/4.2 代码没有重做。本次按用户“继续处理掉”澄清 SAFE-03 与孤立 CR 守卫的文字边界，并将设计中行尾等价证据误作原 bytes 放行依据的表述改为同一基线的精确源码相等；参见 `docs/decisions.md` §14。4.4 仍需真实文件原语、受控故障及文件系统证据；4.5 回填仍未完成。Windows/Linux、真实保存并发、权限、崩溃和断电未验证。

提交前 `git diff --cached --check` 对工具原样生成的日志与隔离夹具报告末尾空行；为保留原始证据未改写这些文件。排除 `gates/` 与 `workspace-gates/` 后，对源码、规范、报告及手写夹具的暂存差异复查退出 0。
