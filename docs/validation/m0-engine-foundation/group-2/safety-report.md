# 源码安全补充守卫（任务 2.3）

该守卫消费自有 Rust 源码，不代替实际成员 lint 继承、Clippy debug/release 诊断、领域非法输入测试或独立调用路径审查。五引擎为 source/buffer/markdown/edit/app-core；ui/plugin/app 同样检查自有 unsafe、static mut 及生产 expect/panic/unreachable/assert 类出口；测试范围允许预期断言，无任何已批准生产初始化例外。引擎独有的 unwrap、线程/运行期全局隔离继续独立检查。

## 实际命令

- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_safety.py --self-test`：45 项正反例符合预期；每项通过独立子进程 CLI 核对退出码及规则名，测试后正常源码重扫通过。
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_safety.py`：当前自有源码扫描通过。

完整 argv、cwd、环境覆盖、时间、退出码和输出见 `safety-self-test.command.json`、`safety-workspace.command.json` 及同名 stdout/stderr。逐例原始输出与完整命令在 `safety-results/`；手写独立预期在 `safety-fixtures/cases.json` 和 self_test 的外置模块用例。夹具从不注入正式 crate，不删除用户文件，也不假装是 Cargo 编译结果。当前扫描文件分类在 stdout 的 scanned 中。正式 root 总门禁/CI 接线由任务 2.5 取证。

## 可自动拒绝的直接形式

- 五引擎所有目标的 `unwrap` token，包括关联方法、函数引用、内联测试、集成测试、示例和基准；不以 cfg(test) 豁免。
- 所有自有生产方法 expect、panic/unreachable/assert/assert_eq/assert_ne/todo/unimplemented 直接宏调用。只认可精确 `#[cfg(test)]` 作用域与 `#[test]` 函数体，集成 tests 目标属于测试；复杂 cfg 保守当生产，不凭文件名 tests 放宽。
- 五引擎直接 OnceLock/LazyLock/Mutex/RwLock、thread 命名空间及 thread_local 宏；static mut、static 中直接出现的常见内部可变类型（Atomic* 保守拒绝）；不可变实体 static/const 表允许。
- 自有 unsafe token；字符串、字符、生命周期、嵌套注释及 raw/byte/C 字符串通过词法区分。损坏词法或无法解析的外置测试模块明确拒绝，不报告合规。

## 盲区与人工检查路径

词法器不是 Rust AST/名字解析/宏展开器。跨模块类型别名、未知内部可变类型、函数间接调用、重命名宏及过程宏输出不会被本工具完整追踪。宏定义体中的直接违规 token 会被扫描，但参数展开后出现的行为仍须审查。嵌套内联模块中的外置 mod 路径因未实现名字空间解析而明确拒绝，不授权同名兄弟文件。所有自有代码的 `#[path]` 及 cfg_attr 中改写路径的属性明确拒绝，防止同文件既作测试又作生产入口而误获测试许可；复杂 cfg/cfg_attr 不自动证明测试范围。需要改为可识别的明确模块边界或逐处审查后改进检查器，不能把诊断当作豁免。

直接 token 检查有意保守：自定义名为 unwrap 的安全方法也会拒绝；Mutex 作为局部类型也会拒绝；这不表示工具已证明所有直接规则的类型语义。测试 expect/panic 类操作的最小例外仍须逐处满足 RS-03，扫描未报错不构成例外批准。

评审每次新增机制时实际跟踪：lib.rs 导出→入口→辅助函数/自有宏及派生宏依赖→索引、切片、整数/容量运算→状态分配/全局访问。核对所有 static/const 类型是否不可变，所有别名最终指向何种类型，所有函数引用最终调用何处，debug_assert 是否仅检查已受检不变量。源码没有命中不证明用户输入无 panic；合法/非法边界与预算测试及 release 路径仍必须实际运行。当前仅有骨架，后续功能加入后须重新进行上述路径检查。

本次无 unsafe 例外、无 lint 例外批准，不声明任何 V 项、领域机制、跨平台能力或性能通过。

守卫不读取 debug/release profile，也不按其过滤源码：除精确测试作用域外均检查，包括 `cfg(not(debug_assertions))` 生产断言的独立反例。Clippy 的 debug/release 实际诊断由主 Agent 的 lint 夹具另行取证，不能把本脚本的两次相同扫描冒充两套 Rust profile 检查。

独立复核追加：外置文件所有消费者先汇总，存在任一生产声明则按生产扫描，覆盖互斥 cfg(test)/cfg(not(test)) 同名消费及 lib/main 双根；不能靠一个测试声明给整文件豁免。未证明测试闭包的普通子模块保守按生产，测试函数本身的 #[test] 仍识别。固定八成员目录之外的 crate 路径明确拒绝，防目录重命名降级为 UI 策略；完整成员名/路径继承仍由工程图守卫验证。

目录身份不只凭目录名：固定八成员路径必须匹配 Cargo `read-manifest --offline` 返回的真实 package name；命令失败或不匹配拒绝。新增把实际 aline-source 放进已知 aline-ui 目录的反例。身份由 Cargo 解析，不自制 TOML 正则；每个只读子命令的完整参数、退出码和原始输出记录于扫描结果 member_reads。新增合法 quoted name 与多行 description 假 name 的误导反例。不解析依赖图，不联网下载，不替代 metadata 守卫。

改用真实 Cargo 清单读取时首次自测退出 1：五个合成 workspace 夹具缺少独立 [workspace]，Cargo 正确拒绝其属于外层 workspace 的错误身份上下文；正常 workspace 扫描退出 0。已为自有合成夹具声明独立 workspace 并重跑，不放宽身份规则。首次失败原始命令/输出/summary 保存于 safety-first-cargo-read*。

## 后续自测计数说明（2026-10-31）

本报告中的 **45 项**是本报告记录时实际执行的历史自测数量，不代表之后每次自测固定为 45 项。后续安全守卫修复追加用例后，2026-10-31 13:43:07Z 的 `--self-test` 实测为 **53 项且全部符合预期**；原始逐例结果见 [`security-followups/self-test.stdout.json`](../group-9/step-9.1/safety-guard-fix/security-followups/self-test.stdout.json)，逐条命令及退出码见 [`security-followups/commands.log`](../group-9/step-9.1/safety-guard-fix/security-followups/commands.log)。这是后续自测证据，不回溯替代本报告当时的 45 项记录。
