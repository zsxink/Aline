# 任务 2.3 安全作用域独立审查（2026-10-31）

结论：当前源码安全补充守卫的已规定直接形式与测试作用域检查可接受，无尚未修复的已确认问题。本结论仅覆盖 `check_safety.py`、`safety_lex.py` 及其范围；任务 2.3 的八真实成员 lint 继承、debug/release Cargo 诊断由主 Agent 独立于实现者验证，不能用词法扫描代替。M0 出口任务 9.6 仍需独立复核。

审查曾因会话额度中断，未将中断当作检查通过。下列独立定向结果已有实际原始输出；主 Agent 随后另重跑这些夹具、45 项完整自测及正常 workspace 扫描。本总结为恢复后的事实补记，不声称中断时已完成最终出口审查。

## 已发现并修复的问题

- `#[path]` 使一个文件同时进入测试与生产，原先文件级测试许可可漏过生产 assert。现对自有代码无法证明的路径属性（包括 cfg_attr 改写）明确拒绝，不猜测字面路径。
- 内联嵌套模块中的外置同名测试声明，原先可错误授权顶层生产兄弟文件。现对未解析的嵌套外置名字空间拒绝，不把兄弟文件标为测试。
- 互斥 cfg(test)/cfg(not(test)) 及 lib/main 双根共用外置文件，原先见一次测试声明就免除整文件生产规则。现汇总全部标准外置模块消费者，生产消费优先。
- 引擎目录改名或真实引擎包放入已知 UI 目录，原先可按 UI 策略扫描。现固定八成员路径并核对 Cargo read-manifest 返回的真实包名；失败或不符明确拒绝。
- 最初包名 TOML 正则会把多行 description 中假 name 当身份并漏读合法 quoted 真名。现由 Cargo 读取真实身份，不自行解析 TOML；只读命令和退出结果留在 member_reads。
- 已知 AtomicU32 等静态内部可变类型及 grouped thread import 的直接形式补入规则；Atomic* 采用保守拒绝，未声称可推断任意用户类型内部可变性。

## 实际证据

- `safety-review-path-scope.*`：最初 path 同源反例的合法生产 Rust 由 Cargo check 实际编译，退出码 0；原始输入保存在命令记录中。其错误静态放行已修复。
- `PYTHONDONTWRITEBYTECODE=1 python3 docs/validation/m0-engine-foundation/group-2/safety-review-recheck.py`：独立六项定向 CLI 反例全部按预期拒绝，脚本退出码 0。完整参数、cwd、时间、退出码及规则在 `safety-review-recheck.stdout.json`，各夹具和原始输出在 `safety-review-fixtures/`；不进入正式 crate。
- 六项分别为 path 同源、嵌套兄弟同名、双 cfg 生产消费、未知目录、quoted 真名与 description 假名、引擎放已知 UI 目录。复核脚本同时记录当时两份实现的 SHA-256，不虚构 Rust 编译或平台运行结果。
- `safety-results/summary.json` 显示最新 45 项全部符合预期，测试后正常源码无违规；主 Agent 的 root-safety-checks 另保留独立重跑。与这些结果并列的 debug/release lint 诊断及成员继承属于另一套 Cargo 证据。

**计数时间边界：** 上述 45 项是本次独立复核当时的完整自测结果。之后安全守卫继续修复并扩展用例；2026-10-31 13:43:07Z 的后续 `--self-test` 共 53 项且全部符合预期，原始逐例结果见 [`security-followups/self-test.stdout.json`](../group-9/step-9.1/safety-guard-fix/security-followups/self-test.stdout.json)，命令与退出码见 [`security-followups/commands.log`](../group-9/step-9.1/safety-guard-fix/security-followups/commands.log)。后续 53 项不表示本复核执行了 53 项，也不改写 45 项历史结果。

## 边界

词法检查不是 Rust 解析、名字解析或宏展开器；不证明索引、容量、间接函数、未知内部可变类型、过程宏和第三方调用无 panic。复杂 cfg 与无法证明的模块路径采用保守拒绝。新增公开实现仍须按入口→辅助函数/宏→受检运算→状态所有者实际追踪，并补相应非法输入和预算测试。

生产直接 expect/panic/unreachable/assert 等规则适用于所有自有代码；五引擎 unwrap（含测试/示例/基准）、线程和运行期全局状态约束独立保持。预期测试断言与编译期不可变表有正例；词法未报错不构成测试 panic 类例外批准。当前没有 unsafe 例外或全局 lint 放行。

本次不声明 V-01～V-09、真实 UI/IME、性能或跨平台支持通过。
