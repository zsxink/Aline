# 第 1 组候选准入守卫证据

日期：2026-10-30。范围仅为任务 1.3 的隔离候选准入与正反例；不是第 2 组正式 workspace 工程守卫，不判断八成员构建、V-01 或 M0 出口。

## 实现与准入边界

`candidate_guard.py` 读取真实 Cargo metadata、`cargo tree --prefix none --format '{p}' -e normal,build,dev` 的选包输出、Cargo.lock 及成功的采集命令记录。默认拒绝未知组合、输入缺失/损坏/空 tree、非零采集退出码、身份或图结构不一致。明确拒绝实际包名 `gpui` 的替代来源（包括使用依赖别名隐藏名称，以及同时已存在合法 gpui-pre 时再引入 gpui），不把库别名 gpui 误认作实际包名。

`guard_policy.json` 对 10 个实际审计组合分别保存 GPUI 完整 metadata 子图的规范化指纹（包身份、来源、lock 归档校验和、features、全部边及实际选包状态），并逐项登记例外包的精确来源/校验和/入边/允许组合。临时候选根及其它合法节点不在子图指纹中，所以正式消费者换根或加入不影响子图的合规节点不会被整体 lock 字节变化误拒绝；GPUI 子图变化必须重审。

完整 GPUI metadata 子图的每个节点（包括未被 tree 选中的过近似节点）均先核对 lock.source 与 metadata.source 相等，registry 校验和必须为有效的 64 位小写十六进制，再进入指纹比较。不能只验证已选节点，也不能只将 lock 校验和值混入指纹而遗漏 lock 来源。

检查已选包之间的全部 metadata 边，且删除 GPUI 节点后再次检查根可达性：任何不经 GPUI 即可到达 regex/wasip2 等的路径均拒绝。五引擎 scope 及图中实际命名为五引擎的节点不能借用例外。不会用“找到一条合法最短路径”放行同包的其它入边。

metadata 中的未选包仍在结果中列出。已审 GPUI 子图的过近似节点已被指纹锁定；自有路径出现未经审查的 metadata/tree 差异直接拒绝。即使同一节点被选中，也不会据此自动放行未经登记的边；无法消除的过近似边采取保守拒绝，不能当作自动合规。

例外入边如下：

- `gpui-pre 0.3.9 → regex 1.13.1`。
- `regex 1.13.1 → regex-automata 0.4.18`。
- `regex 1.13.1 → regex-syntax 0.8.11`、`regex-automata 0.4.18 → regex-syntax 0.8.11`。
- GPUI **test-support** 既有 `proptest 1.12.0 → regex-syntax 0.8.11`，这条入边不是从 regex 直接发起；仍需 GPUI 支配、精确组合和完整子图一致。
- `getrandom 0.3.4 → wasip2 1.0.4+wasi-0.2.12` 仅限 `cfg(all(target_arch = "wasm32", target_os = "wasi", target_env = "p2"))`，仅 all-target 两组允许，所有已审桌面组合均不允许选入。

没有登记 metadata-only 的旧 `wasi 0.11.1` 为例外，也未放行 notify/tokio/Wasmtime/第二解析器。

## 执行入口

单个已审组合示例（从仓库根执行）：

```sh
python3 docs/validation/m0-engine-foundation/admission-2026-10-30/candidate_guard.py \
  --metadata docs/validation/m0-engine-foundation/admission-2026-10-30/metadata-macos-arm.json \
  --tree docs/validation/m0-engine-foundation/admission-2026-10-30/tree-macos-arm-packages.stdout.log \
  --lock docs/validation/m0-engine-foundation/admission-2026-10-30/candidate/Cargo.lock \
  --scenario macos-arm --scope app
```

默认读取 metadata 同名 `.command.json` 与 tree 同名 `.command.json`；可用 `--metadata-command` / `--tree-command` 显式指定。输入证据必须来自可信的本地采集过程，命令记录只是复核执行结果，不是抗恶意篡改签名。

完整正反例入口：

```sh
python3 docs/validation/m0-engine-foundation/admission-2026-10-30/test_candidate_guard.py
```

最终执行退出码为 0。原始逐条命令、退出码、stdout/stderr、独立预期见 `guard-results/*.command.json`、`*.stdout.log`、`*.stderr.log`；汇总见 `guard-results/summary.json`。合成输入变异的基线、变换标签、预期诊断、实际临时路径及 SHA-256 在 `guard-fixtures/*.json`；变异发生在独立临时目录，原始审计图未被修改或删除。末项直接重查原始图通过。

## 正反例及独立预期

最终共执行 **60 条命令**，其中 14 个候选检查正例、29 个候选检查反例、17 条实际 Cargo 图采集命令；结果全部与预期退出码和具体诊断一致。另对 7 个真实 Cargo 负例的 alias/kind/target/中间包形状，按夹具 manifest 独立核对成功。

正例包括：10 个留存真实 GPUI 组合（基础与 test-support all-target、各桌面目标组合），真实无 GPUI 本地最小工程，临时候选 root 改名为 app，加入额外合法节点，及变异后原始图恢复复查。

反例包括：

- 真实 Cargo 本地解析的 7 个最小违规图：别名 regex、Windows 目标 notify、build tokio、dev Wasmtime、feature 激活 WASI、中间包引入 comrak，以及别名隐藏的实际 gpui 包；metadata/tree 均解析成功，随后守卫明确拒绝。其诊断保留真实 alias、kind、target 与包身份。实际 gpui 包分别按 app/engine scope 拒绝，另有“既有合法 gpui-pre 再加原始 gpui”的合成输入反例。
- 已有合法 GPUI regex 路径之外再加自有直接入边；root scope 为 engine 借用例外；实际名为 aline-buffer 的中间节点依赖 GPUI。
- 已审边 kind/target 漂移、未审 feature、新 scenario、版本/来源/校验和漂移（包括同步改 metadata 与 lock 的 GPUI 源漂移）。
- test-support 真实 metadata 中未选中的 `wasi 0.11.1+wasi-snapshot-preview1`，仅改变 lock 来源或改为无效校验和仍须拒绝；两例均先独立确认该节点没有出现在实际 tree 中。
- 缺失/损坏 metadata、缺失/空 tree、遗漏已选包、未经审查的自有 metadata-only 禁库、真实 Cargo 采集失败。

独立预期来自现行架构 §3.2 精确边界、明确构造的 manifest/输入变异及 Cargo 输入错误，不用守卫实际输出生成预期。第二解析器反例使用 comrak 这一已知解析器名；这不构成对所有未知包语义的自动识别。

`guard-fixtures/packages/` 中命名 notify/regex/tokio/wasmtime/wasi/comrak/gpui 的 **0.0.0 本地包全部是明确合成夹具**，用于验证 Cargo 真实别名/kind/target 数据与守卫检测，不是同名真实发布库，不声称下载或兼容验证这些库。没有安装工具链、引入禁库到 Aline 正式工程或建立八成员 workspace。夹具自有 Rust 配置 `unsafe_code = "forbid"`，不使用 unwrap。

## 开发失败与修复

第一版脚本有一处 Python 括号语法错误，尚未进入测试，修正后执行正反例。第一次完整运行中，遗漏 regex 选包负例确实退出 1，但测试预期关键词写成“子图”，实际在更早的 root 可达性检查拒绝；已按独立“选包不一致应拒绝”的预期修正诊断断言。初次不通过汇总保存在 `guard-results/summary.initial.json`，没有将其当成通过。随后增加 GPUI 来源漂移、嵌套引擎、kind 漂移及缺失 metadata 等边界，53 条执行全部符合预期，汇总留存在 `summary.before-raw-gpui.json`。

主 Agent 复核随后指出：原版只识别 gpui-pre，缺少对实际包名 gpui 的替代来源拒绝。已补充明确拒绝和真实 Cargo 别名图的 app/engine 检查、既有合法 gpui-pre 旁加原始 gpui 反例；58 条执行全部符合预期，汇总保存在 `summary.before-unselected-lock.json`。

独立 reviewer 随后发现：完整 GPUI 子图的未选节点只从 lock 取校验和，尚未核对 lock 来源与 metadata 来源相等；只改变 metadata-only wasi 节点的 lock 来源可能绕过原指纹。已将完整子图的来源一致性及校验和合法性检查前置，补充未选 wasi 节点的来源漂移与无效校验和反例；最终 60 条执行全部符合预期。

收尾 JSON 扫描第一次将故意失败的 `negative-cargo-failed.metadata.json` 空 stdout 当成正常 JSON，出现 JSONDecodeError；该文件保留真实 Cargo 失败输出，其 command 记录退出 101，不能改写成伪造成功 JSON。按实际非零采集记录排除这一原始失败输出后，其余所有归属本任务的 JSON 与两个 Python 源文件语法检查通过。`git diff --check` 和 `codegraph sync` 实际退出 0。

## 未验证与后续责任

- 这是受限的候选审计辅助 CLI，不是通用 Cargo workspace 依赖守卫。第 2 组仍须实现内部方向、所有成员继承、GPUI 经 dev/build/target/features 泄漏等正式门禁及对应正反例。
- 当前解析 Cargo.lock 的 package 标量身份，不重做 Cargo 解析器；同名同版本不同来源导致 tree 名称歧义时拒绝。版本/来源/锁定校验和来自已验证 registry 归档与实际 lock；本工具不重新下载或逐文件审计所有第三方源码。
- 最短路径不是授权依据，所有入边及 GPUI 支配性才是；但对 Cargo metadata 的过近似边采取保守拒绝，可能要求重新采集或人工分析，不能靠忽略边修补通过。
- guard_policy 是本地可信审核清单，不带签名；不能把修改后的政策文件或伪造的采集记录当成供应链真实性证明。
- 已知第二 Markdown 解析器包名可拦截；未知名称/自有复制解析器需人工审查。构建脚本运行期行为、线程/全局状态、许可证法律判断和其它未知包用途不由本工具证明。
- 只明确识别现行约束涉及的实际包名 gpui/gpui-pre，不声称自动识别所有未知 GUI 库或改名复制实现；未知库用途仍需人工准入。
- 没有编译或运行实际 Windows/Linux/Intel Mac 应用、创建窗口或验证原生 GPUI 后端；没有正式五引擎实现或 M0 出口通过结论。
