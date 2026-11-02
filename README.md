# Aline

Typora 式 Markdown 编辑器。Rust + GPUI，不使用 WebView，目标平台 macOS / Windows / Linux。

> **当前阶段：M0（引擎基座）。尚无可运行的界面。**
> 工作区目前只建立编译边界、纯逻辑 crate 与工程门禁，应用窗口与平台服务留待后续阶段。这里没有可下载试用的产品，请不要按「装上就能写 Markdown」的预期使用。

## 技术路线

来源文本是唯一事实来源，渲染不反向持有文档状态：

1. 源文本解析为展示树，同时携带「绘制文本 ↔ 源码字节」双向映射
2. 按块虚拟化渲染，同一时刻最多一个块处于「打开编辑」状态
3. 提交时对源文本做单区间补丁，单步撤销
4. 未编辑的字节原样写回

这条路线决定了绝大多数约束：不引入第二份文档状态、不做显示层反向同步、编辑器不持有平台服务。

## 工作区结构

| Crate | 职责 | 显式不做 |
|---|---|---|
| `app` | M0 薄入口，建立编译边界 | 不创建窗口或平台服务 |
| `crates/aline-app-core` | 显式配置与 catalog 的纯模型 | 不持有 GPUI 或全局语言状态 |
| `crates/aline-source` | 文件字节与存储元数据的受检边界 | 独立于文档缓冲 |
| `crates/aline-buffer` | 源码补丁、坐标、行索引与历史的纯逻辑 | 不解析、不写盘 |
| `crates/aline-markdown` | 从不可变源码派生语义与映射 | 不持有显示或插件状态 |
| `crates/aline-edit` | 只读快照到受检变更计划的边界 | 不写真实文档 |
| `crates/aline-ui` | 文档状态与跨层事务的所有者 | M0 只实现无窗口状态协议 |
| `crates/aline-plugin` | M0 主链编译边界 | 具体提供者留待后续阶段 |

`aline-edit` 的失败语义是最能说明这套设计的一条：绑定工作版本生成计划 → 复验期望片段 → 构造候选文本并重新解析 → 在候选上核对命令语义与目标选区，**任何一步失败都在真实写入之前返回拒绝**。

## 质量门禁

`scripts/` 下是顺序执行的检查，每条独立保留结果，即使前一条失败也不伪造后续结果或整体通过：

```sh
python3 scripts/check_workspace.py    # 顺序运行 M0 本地及 CI 门禁
python3 scripts/check_lints.py        # workspace lint 声明与成员继承
python3 scripts/check_engineering.py  # 依赖准入、锁文件、场景矩阵
python3 scripts/check_safety.py       # 词法级安全约束
python3 scripts/check_public_api.py   # 公开 API 边界
```

每个脚本都支持 `--help`。设计上宁可抛错也不静默忽略——例如 `check_lints.py` 内置 TOML 1.0 子集解析器，逐行扫描会把多行字符串里的伪 `[lints]` 表当成真实表，所以无法安全解释的构造一律抛 `CheckError`。

依赖准入门禁带 `--scenario`（macos-arm / macos-intel / windows-x64 / linux-x64 / linux-arm / all-features），按目标平台分别核对。

## 构建

```sh
cargo build
cargo test
```

Rust edition 2024，工具链锁定见 `rust-toolchain.toml`。UI 层依赖 `gpui-pre`（Zed 的 GPUI 预发布线，版本锁定）。

## 文档

入口是 [AGENTS.md](AGENTS.md)，它同时是 AI 编码代理的工作指引。其余按用途分流：

| 想知道 | 读 |
|---|---|
| 产品范围与非目标 | `docs/v1-prd.md` |
| 引擎契约 | `docs/engine-design.md` |
| 架构蓝图与里程碑 | `docs/v1-architecture.md` |
| 工程基线（模块大小、状态所有权、Rust 约束） | `openspec/CODE_STANDARDS.md` |
| 工程治理规格与验收场景 | `openspec/specs/engineering-governance/spec.md` |
| 文档权威关系与阅读顺序 | `docs/README.md` |

## 关于本仓库

Aline 是独立开发的项目，规格、门禁与验收口径由人定义，实现过程有 AI 编码代理参与（见 `AGENTS.md` 与 `CLAUDE.md`）。

仓库保留代码、规格、可执行门禁、可复现测试及独立夹具/金样。运行输出统一放不追踪的 `logs/validation/`，临时交接放不追踪的 `tmp/`；不提交 Agent 逐轮报告。复跑入口见 `scripts/`、各 crate 测试和[验证计划](docs/v1-validation-plan.md)，提交边界见[开发流程 §5](docs/development-process.md#5-仓库产物与提交边界)。

## 状态与边界

- M0 出口未闭合，V-01～V-09 均未整项通过；工程与无头基础分项不能替代完整验收，见 `docs/v1-validation-plan.md`
- 无可用界面、无发布产物、无插件运行时
- CI 见 `.github/workflows/m0.yml`，与 `scripts/` 本地门禁同源

如果只是想看这个项目的方法论而不是代码，从 [docs/development-process.md](docs/development-process.md) 与 `openspec/` 开始更快。
