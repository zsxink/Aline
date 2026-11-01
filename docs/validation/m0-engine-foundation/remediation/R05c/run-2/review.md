# R05c 独立复核

**APPROVE：建议合并 R05c，D1/D2 关闭；未发现新的可操作缺陷。** 未参与实现及原始取证。CodeGraph 定位并核对 diff，SHA `e8f4372f7495`。

- D1：`scripts/check_lints.py:682` 校验真实元组路径。独立八成员夹具的两种混合形态均 exit 1，正常策略均 0。八条独立 target 的 Cargo 对照证明：混合配置 check/clippy 均 0 且警告忽略键；真实 clippy/unsafe 策略对违规源码分别 101，driver 参数与诊断吻合。
- D2：第 1034 行按完整源路径匹配定位。含空格目录完整 self-test exit 0；表路径单项含空格与冒号也通过。25 parser、12 table、5 member、原 Rust 正反例与八成员继承通过；无关 `[package] "a.b"` 仍接受。原检查未弱化；本轮扩展职责连贯，无 API/依赖变化。
- 独立复跑 9 个变异均 exit 1，命中指定 case 和具体断言，无 traceback。AST、正常守卫、diff 检查通过。
- 原失败件哈希未变；原缓存控制的意外 0 与预期 101 均保留，八条 fresh 证据已核对。

命令、环境、日志与夹具见 [review-evidence](review-evidence/)。额外完整自测的冒号目录被 Cargo/macOS 路径分隔限制阻断，失败日志保留；空格修复结论不变。残留仍为 TOML 子集、深度上限 100、RecursionError 兜底缺专门断言、既有自测 Cargo 仅显式 offline；本轮未跑全量语料、workspace 全门禁或其它平台，不代表 M0 出口通过。
