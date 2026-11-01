# R03 / F-04 独立复核

结论：**APPROVE_WITH_CONCERNS**。建议合并受检坐标实现；下述报告勘误应在 R14 最终证据汇总前完成。未发现需要修改实现的正确性、安全或兼容性缺陷。

复核者为本轮独立 reviewer，未参与 R03 实现；未修改实现、原始日志或总状态。受检实现提交 `128954eb1a68b572bd9732a7c8817082ad19c64a`，实际运行 HEAD 为 `96024867f7ae949d7328232b05d93ed5bc6033d9`。本次 `reuse.rs` 与 `tests/reuse.rs` 相对实现提交的 diff 为空；当前输入 SHA-256、初始工作树状态见 [identity.json](review-evidence/identity.json)、[status.stdout.log](review-evidence/status.stdout.log)。其它批次已有改动未触碰。

已先使用 CodeGraph，继而核对提交差异、真实消费者与原始证据；依据 `CODE_STANDARDS.md`、工程治理规格、Rust S-5-01～06、引擎 §7、整改任务 §3/4 R03 与 brief。检查范围为坐标构造、快照身份、复用配对、重定位及公开消费边界，不代表其它批次或 M0 出口通过。

1. **身份和 UTF-8 闭环通过。** `reuse.rs:94–156,236–258` 保留两个 `SrcRange`，校验旧身份、闭区间块范围、新身份，受检算术后通过目标 `src_offset` 生成出口；不存在旧裸转换兼容入口。`coordinates.rs:18–40,71–106` 的身份与字段封装，以及 `text.rs:169–216,279–324` 的不可变快照/受检构造，使半字符或越界旧端点无法从外部安全构造。配对只在同类、源码字节相等块之间形成，故端点平移保持 UTF-8；目标再次校验。失败路径没有写入、副作用、clamp、线程或全局状态。
2. **消费者与兼容边界通过。** 正式 `crates/`、`app/` 中 `translate` 的消费者仅 `tests/reuse.rs`，`lib.rs:30` 显式导出错误类型；旧范围/入口签名变更为有意收紧，既有测试逐一适配，无生产调用遗漏。`SrcRange::byte_range()` 是已存在的观察/表示边界，不提供裸整数重定位替代路径。`ReuseOutcome`、`ReusedBlock` 的 Eq/PartialEq 保留。
3. **当前独立验证通过。** [run_review.py](review-evidence/run_review.py) 与逐命令 `.command.json`、原始 stdout/stderr 保存实际 argv/cwd/起止时间/exit 和环境。固定 1.98.1、offline；初始独立夹具 lock 单列，后续 Cargo 显式 locked/offline。markdown 全部 **127 测试通过**（reuse 23），Clippy all-targets 通过。独立外部正例使用 `甲😀z` 手工常量，覆盖中间合法端点/半字符、两侧块外、最大值、同文不同身份、clone、范围复验、出口身份、闭区间末端与错误优先顺序。5 个编译负例分别拒绝 usize 入口（E0308）、旧签名（E0061）、DrawnOffset（E0308）、两个裸范围访问器（E0308）、伪造 SrcOffset（E0451）；均核对具体诊断，未以任意非零视为成功。原通用 public-api 的 8 例证据也已核对；本次专用外部负例补足其不直接覆盖 ReusedBlock 的范围。

唯一问题（低 / P3，证据说明）：[report.md](report.md) 第 73 行声称“新增 5 条……全部是手写常量，并在修复前树上实测失败”。触发条件是据此判断五条均有先行失败；实际 [pre-fix.command.txt](pre-fix.command.txt) 只运行 F-04 单测，[pre-fix.txt](pre-fix.txt) 为 `1 failed / 18 filtered out`，不证明其余四条先行运行；`tests/reuse.rs:140–144` 的逐字节预期还通过范围算术生成，并非全部字面常量。影响是扩大了先行失败与独立预期的证据范围。建议将该句改为“F-04 单例在修复前失败；其余四条为新增契约回归，修复后通过”，如实说明逐字节期望的推导；保留原日志。本次独立外部常量探针证明当前行为，不能追认旧运行。

未复跑全 workspace 或平台 UI；当前结论限 R03 纯逻辑和实际 macOS 工具链。原始先行测试的当时工作树源码未另存，现存日志只支持上述单例失败，不扩大其历史证明范围。
