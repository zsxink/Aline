# 任务 2.3 主 Agent 复核

日期：2026-10-31。任务 2.3 已完成并勾选；这只是工程策略有效性，不代表引擎业务、完整 M0 或 V 项通过。

## 实际结果

- `cargo clippy --workspace --all-targets --locked --offline -vv -- -D warnings`：退出 0。`lint-actual-members.*` 保存完整命令、环境、日志；`lint-actual-members-inheritance.json` 从实际 clippy-driver/rustc 命令核对八成员各两个调用（含 test）均携带登记的 11 项 lint 参数，未以根配置存在代替继承验证。
- `python3 scripts/check_lints.py --self-test --output-dir …/root-lint-checks/self-test`：退出 0。八个成员隔离继承反例、库/test/example/bench 的 unwrap、expect/panic/unreachable、坐标窄化/符号丢失及 debug/release 正例均在预期诊断处得到对应退出码；单成员漏继承被真实检查入口拒绝。详见 [lint 报告](lint-report.md)与 root-lint-checks/。
- `python3 scripts/check_safety.py --self-test --output-dir …/root-safety-reviewed/self-test`：退出 0，最终 45 项正反例符合预期；结束时重新扫描正式八成员源码无命中。完整 argv、原始输出、Cargo 清单读取、时间和退出码见 root-safety-reviewed/。
- `python3 docs/validation/m0-engine-foundation/group-2/safety-review-recheck.py`：主 Agent 重跑退出 0，独立 reviewer 预先编写的六个定向反例全部在预期规则拒绝；证据 safety-root-final-review.*。独立终核随后完成，见 [安全终核](safety-review-final.md)。

## 策略与作用域

Rust unsafe_code=forbid、missing_docs=warn；Clippy 九项 deny，具体表见 lint-report。无 pedantic 全组、无全局坐标转换 allow、无自有 unsafe 例外。

五引擎所有自有目标禁 unwrap。自有生产代码拒绝 expect、panic/unreachable/todo/unimplemented 及发布断言，测试预期断言允许；debug_assert 只能建立在受检输入之后。源码守卫扫描全部 cfg 分支，不依据 debug/release 把发布分支漏掉；实际两种 Cargo profile 的诊断另由 lint 夹具证明，不能混淆两类证据。

OnceLock/LazyLock、线程/线程局部状态、可变静态和已知原子/内部可变静态在引擎直接 token 路径被拒绝，编译期不可变表允许。Cargo read-manifest 提供真实包身份；测试作用域按声明检查，共享文件存在生产消费者时优先按生产检查。无法证明的 path 属性、嵌套外置模块归属明确拒绝，不默许生产断言。

## 审查与盲区

独立审查发现的外置模块路径、同名嵌套模块、共享文件双重消费、错误目录包身份及清单多行字符串误读均保留反例并修复；清单最终交给 Cargo 解析，不再自建 TOML 名称正则。审查子代理一度触及用量限制，主 Agent 补跑既有独立夹具后，子代理恢复并完成终核；不把中断写成通过。

当前骨架源码经人工查看，除空 main 外无业务函数、无运行期全局 locale/缓存、无线程调用、无 unsafe 豁免。该观察只针对任务 2.3 当时的骨架，后续新代码仍须持续检查。

静态守卫不是完整 Rust 语义证明：过程宏输出、跨模块别名/重导出、自定义内部可变包装、隐式索引/分配 panic 和第三方调用需编译器、领域坏输入测试及人工路径审查。后续坐标 newtype 混传的编译失败测试属于 3.2，不能以本次 cast 诊断代替。实际平台仅 macOS aarch64，其他平台及 CI 未执行。
