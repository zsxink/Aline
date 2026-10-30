# 第 2.3 项 workspace lint 实效验证

日期：2026-10-31。环境：macOS aarch64；Rust/Cargo 1.98.1、Clippy 0.1.98。未安装工具链或依赖。命令记录与逐条 stdout/stderr 在 `lint-self-test/`。

## 正式 workspace 配置

根 Cargo lint 使用 Rust `unsafe_code = forbid`、`missing_docs = warn`；Clippy 按项启用 `unwrap_used`、`expect_used`、`panic`、`unreachable`、`cast_possible_truncation`、`cast_sign_loss`、`dbg_macro`、`todo` 和 `unimplemented`，级别均为 deny。没有整组启用 pedantic，也没有放行坐标转换。八个成员显式继承 workspace lint。

主 Agent 已独立对实际八成员运行 `cargo clippy --workspace --all-targets --locked --offline -vv -- -D warnings`，退出码 0；逐成员及 test target 的 clippy-driver 命令各带 11 个配置 lint 标志，证据见 `lint-actual-members.*`。本报告不重复该workspace构建。

## 本报告的可重跑命令

使用从第 1 组隔离记录读取的 `CARGO_HOME`，并固定 `RUSTUP_TOOLCHAIN=1.98.1`、`PYTHONDONTWRITEBYTECODE=1`：

```sh
python3 - <<'PY'
from pathlib import Path
compile(Path('scripts/check_lints.py').read_text(), 'scripts/check_lints.py', 'exec')
print('Python3语法编译通过')
PY
PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_lints.py
python3 scripts/check_lints.py --self-test --output-dir docs/validation/m0-engine-foundation/group-2/lint-self-test
```

最后一条由隔离执行器设置上述 `CARGO_HOME` 与 `RUSTUP_TOOLCHAIN`。完整 argv、环境覆盖、时间和退出码见 `lint-self-test/invocation.command.json`；Cargo/rustc/clippy版本及所有诊断夹具命令分别记录在 `lint-self-test/versions/` 和 `lint-self-test/results/`。

实际结果：语法检查退出码 0，正式workspace配置审查退出码 0，self-test退出码 0。self-test证据来自 Cargo/Clippy 实际运行，非仅Python字符串匹配。

## 正反例

- 八成员隔离 workspace 分别运行 `cargo check -p <member>`；八项均以 exit 101 输出 `unsafe-code` 诊断。每个manifest使用与正式workspace相同的lint继承配置和实际包名。另一个八成员夹具只移除 `aline-source` 的继承，正式检查入口以 exit 1 报告该成员。
- 正例在 debug/release 下以 Clippy 检查测试预期断言和公开、带文档的不可变实体常量表，均 exit 0。
- `unwrap()` 分别放入库、集成测试、example 与 bench target，八种目标/配置运行均以 exit 101 命中 `clippy::unwrap_used`。
- 生产 `expect`、`panic!`、`unreachable!`、可能截断绘制偏移的窄化转换和有符号转无符号转换，分别在 debug/release 下均以 exit 101 命中各自 lint。
- 源码尝试在 `unsafe_code = forbid` 下使用局部 `allow`，`cargo check` 仍以 exit 101 拒绝。

每个反例都有独立 Rust 源文件和命令记录。exit 101 只有在同时匹配预期诊断时才记成功，普通编译错误不会被当作lint拒绝通过。漏继承守卫反例期望exit 1及准确成员诊断。

## 范围和盲区

这些隔离 Cargo crate 使用正式lint表副本，证明规则在该工具链中有效；单独证明实际正式成员 manifest 的继承声明及其真实workspace clippy-driver参数，未把隔离构建说成完整业务 crate 测试。

Rust lint对宏调用语义、别名/重导出、过程宏展开和调用路径只有编译器所能观察的范围。Rust生产断言、线程/运行期全局状态及未经检查的索引由独立 `scripts/check_safety.py` 任务验证和人工路径审查，不声称 Clippy 已覆盖。强类型坐标混用的编译失败属于任务 3.2；本项的受检转换反例覆盖的是明确命名的绘制字节偏移窄化及符号丢失。没有测试所有 CPU/OS/profile 组合，隔离debug/release只验证当前主机工具链的lint执行。
