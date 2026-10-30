# 第 2 组：八 crate、工程门禁与 catalog

日期：2026-10-31。范围：任务 2.1～2.6。环境：沿用第 1 组登记的 macOS 27.0.1 / Apple M5 aarch64、Rust/Cargo 1.98.1、rustfmt 1.9.0、Clippy 0.1.98，未声明 MSRV。取证基于提交 `2b1409ba51fc6325a0ea8fe0edb9e3bc4a1c16eb` 加本组工作区改动；本组未创建分支或提交，没有安装工具、修改全局配置或删除用户文件。

## 分项及任务勾选

| 任务 | 实际行为 / 证据 |
|---|---|
| 2.1 | 正式八成员、统一依赖/工具链/锁文件；workspace-shape.json 和逐引擎 build/test 日志。source/buffer/markdown/edit 仍是空骨架，不能称业务测试通过 |
| 2.2 | 正式 metadata/tree 依赖守卫、77 条实际采集/正反例子命令、恢复七场景正常图；[实现报告](engineering-guard-report.md)、[独立终审](review-final.md)，主 Agent 复跑在 root-dependency-checks/ |
| 2.3 | 11 项实际 lint 参数在八成员及测试目标继承；隔离 debug/release 诊断、45 项最终安全正反例；[主 Agent 复核](lint-safety-summary.md)、[独立安全审查](safety-review-final.md) |
| 2.4 | app-core 显式 Locale/Translator、双语 TOML/rust-embed；5 个测试覆盖键、插值、编码/schema 拒绝和实例隔离，UI 另有 1 个真实 key 消费测试；[catalog 报告](catalog-report.md)、[独立审查](catalog-review-final.md)、root-catalog-checks/ |
| 2.5 | `scripts/check_workspace.py` 和 `.github/workflows/m0.yml` 最小接线；完整本地入口退出 0，CI 未执行，原始记录在 workspace-gates/final/ |
| 2.6 | 回填 Rust §2/17/19、架构 §4/10.3 和本证据入口；下方记录实际公开面、所有者、例外及盲区 |

各项只有对应行为实际实现并验证后才勾选。本组完成不代表第 3～9 组、M0 或 E1 完整出口通过。

## 命令、失败与恢复

完整 argv、cwd、UTC 时间、环境覆盖、退出码及 stdout/stderr 均在同名 command.json/log。以下列出的命令均实际执行；不把结构校验或模拟反例写成平台运行验证。

- 五引擎逐包 `cargo build -p <crate> --locked` / `cargo test -p <crate> --locked`：均退出 0；当时都是骨架目标。
- `python3 scripts/check_engineering.py --self-test` 与正常 workspace 入口：最终退出 0。20 个隔离八成员 Cargo workspace 的元数据/tree/判定与明确标记的图变异分别记录，合成 0.0.0 禁包不冒充真实发布库。旧失败日志保留；Windows host 宏路径与目标过滤 metadata 差异已修，不忽略 libc。
- `cargo clippy --workspace --all-targets --locked --offline -vv -- -D warnings`：退出 0；实际成员参数在 lint-actual-members.*。`check_lints.py --self-test` 也由主 Agent 复跑退出 0，违规必须同时匹配预期诊断，不能仅凭 exit 101 判成功。
- `check_safety.py --self-test`：最终 45 项预期一致，退出 0；正常扫描退出 0。独立审查的六项作用域/清单身份反例重验全部拒绝，汇总脚本退出 0。之前发现和修复的漏洞、额度中断与补复核均在独立记录中如实保留。
- `cargo test -p aline-app-core --locked --offline`：主 Agent 实跑 5 passed；`cargo test -p aline-ui --locked --offline`：1 passed。worker 另实际执行 release、app 入口和 Clippy，原记录见 catalog-report，不把无窗口运行说成原生窗口/输入验证。
- 统一入口最小无缓存反例：退出 1，未安装或下载。第一次完整入口因 `rustfmt --print-config current` 缺必要 PATH 退出 1；三条 Cargo 检查和各守卫当次实际退出 0，但总体未通过。读取锁定 rustfmt 的 `--help` 后补入文件路径参数（current 输出到 stdout，不改源文件），修复后完整入口重跑退出 0。
- 最终统一入口 `python3 scripts/check_workspace.py --output-dir …/workspace-gates/final`：登记组件查询、版本、格式有效配置、`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、依赖/lint/安全正常入口与自测全部退出 0。实际工具目录由已安装 toolchain 的 `rustup which` 加入子进程 PATH，避免仅声明版本却运行其它二进制。
- rustfmt 有效配置实际为 `max_width = 100`、`edition = "2024"`，fmt 日志无忽略选项警告。没有 nightly 选项或全组 pedantic。
- 文档回填第一次 `git diff --check` 退出 2，指出证据入口日期/范围两行的 Markdown 尾空白；删除这两处尾空白后重跑，未扩展格式化其它文档。

## 修改文件与状态所有者复核（人工）

正式工程包含 root Cargo.toml/lock、rust-toolchain.toml、rustfmt.toml、app 与七 crate 的 manifest/最小源码。新增工程脚本为 check_workspace/check_engineering/check_lints/check_safety、必要依赖模型/策略/正反例和词法辅助；构建缓存仅加入 gitignore，没有删除缓存文件。证据为本目录及第 1 组既有准入目录，不改历史日志来消除失败。

当前有业务公开面的引擎只有 app-core：Locale、Translator、CatalogError 经 lib.rs 显式导出；实现位于 i18n 叶模块，坏 catalog 构造器和解析辅助私有。实例独占 locale 和两个不可变查询表，set_locale 不影响其它实例；没有全局 locale/线程/锁/执行器。UTF-8 与 TOML 原错误保留 source 链，schema 原因为枚举，字符串负载只携 key/资源名，Display 是技术诊断，不直接成为 UI 文案。

UI 的 application_name(Locale) 实际调用 t("app.name") 并返回 GPUI SharedString；app 的最小消费只返回 ExitCode，没有窗口、真实输入、保存或平台服务。该显式 locale 仅为 M0 编译消费，不宣布产品默认语言策略。Rust-embed 已准入 debug-embed 的编译嵌入由 Cow::Borrowed 测试实际验证；app-core 无运行期文件读路径。其余四引擎和 plugin 仍是职责骨架，没有额外公开写句柄、注册器或后端。

source/app-core 没有反向业务依赖。五引擎完整保守闭包分别为 source 1、buffer 1、markdown 6、edit 7、app-core 40 个包，均无 GPUI/禁用库；根禁止直接借用 GPUI 例外。完整 all-target metadata 约束身份和边，各目标 tree 保留本机 host 宏/构建选包，filtered metadata 只作子图交叉证据。正式 GPUI 子图仅有已准入 toml 引起 toml_datetime serde feature 统一；精确禁库身份/来源/checksum/全部入边不变，七个具体指纹已独立复核，没有扩大 §3.2 例外。

自有 unsafe 例外为零；无生产 expect/panic/unreachable/assert 豁免，无引擎 unwrap 豁免，无未消费的主题、插件、图片、导出或动态宿主依赖。宏/别名/隐式索引和分配的语义盲区不由词法扫描冒充证明，后续领域输入路径仍需逐处审查与坏输入/预算测试。

## CI、平台与未执行项

CI 仅创建配置：self-hosted macOS ARM64、已安装的登记工具组件、预热隔离缓存变量 ALINE_M0_CARGO_HOME。checkout 精确来源和参数见 ci-source-review.json；没有配置远端 runner、推送或执行远端作业，**CI 未执行/未验证**。无缓存或无工具链时入口拒绝，不自动安装。源实现、源码/绘制映射、补丁/历史和 DocCore 尚未实现；M1 窗口/只读显示、M2 输入/保存闭环均未执行。

目标图解析不能证明 Intel Mac、Windows 10、Linux 构建或支持。真实窗口、IME、Metal/字体/性能、权限/替换竞争、完整 AC 和 V-01～V-09 均未验证，不改原编号或口径。下一步仍按顺序进入第 3 组；第 4 组按 [已批准行尾定义](../source-representation-decision.md)验证，不把分析演算当 source 成绩。
