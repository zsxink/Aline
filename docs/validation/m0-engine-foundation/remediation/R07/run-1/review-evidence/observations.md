# R07 独立复核观察

复核者未参与实现或原期望编写。CodeGraph 已先用于定位测试与映射 API；随后审阅 `f87d2f0` 差异及 R07 brief、report、handoff、原始日志。当前相关测试与该提交一致；身份与命令见 `input-identity.txt`、`commands.jsonl`。

## 独立计算

`independent.swift` 独立输入字面字符串，用 Swift 的 Unicode scalar 与 Character 枚举计算字节边界，并额外输出 UTF-16 标量边界。没有读取或调用 Aline。原始输出见 `independent.stdout.txt`，九组绘制常量全部相符；这是当前复算证据，不能证明原作者历史创作来源。Swift 平台分段只作交叉检查，不构成 Aline 新依赖或三平台验收。

- 家庭 emoji：UTF-8 标量 `[0,4,7,11,14,18]`，UTF-16 标量 `[0,2,3,5,6,8]`，字素字节 `[0,18]`；可见 UTF-16 代理对合法边界也不等于字素边界。
- 原始 `a\r\nb`：标量 `[0,1,2,3,4]`，字素 `[0,1,3,4]`；现有测试测 `a\nb`，两者均 `[0,1,2,3]`，未直接测原始 CRLF 中点排除或 source_at 的 Aligned 来源。
- 候选 `AéB` 与连续两面旗帜亦独立输出；现有复合簇夹具只有单簇，未验证多簇环境、RI 奇偶或中间簇起点导航。brief 未要求完整 Unicode 分段，故列边界，不升格为本批阻断缺陷。

## 变异证据

M1 的接受集合差异、M2/M3 的差集差异均能从现有代码复算，失败日志内容可信。但三次均改单个手写常量，证明相互约束，而非字素算法识别能力；同时改错 grapheme 和 diff 可保持自洽。M3 的实际修改是向 `[0,18]` 加入簇内标量边界 7，并非文字所称“删掉”。

`self-proof.txt` 恢复指纹 `090515…` 与最终文件 `c8e78c…` 不同，失败行号亦有变化；无变异脚本或中间完整文件，无法重建历史中间态。没有重新写生产/测试文件进行变异，不将这些旧日志追认为最终提交的独立变异重跑。

## 范围核对

提交对 Rust 仅追加 `tests/mapping.rs` 293 行，无原断言删改；没有 `src/**`、manifest、lockfile 或新依赖。映射层仍只接受标量边界，字素轨迹函数仅遍历手写集合；按 brief 允许这一纯数据验收，不能据此关闭引擎导航或整个“重复文字与字素”scenario。既有 UTF-16 代理对测试保留并随 17 条 mapping 测试实跑通过。

## 依据文字勘误

[UAX #29 §3.1.1](https://www.unicode.org/reports/tr29/tr29-49.html#Grapheme_Cluster_Boundary_Rules) 的 GB999 为 `Any ÷ Any`；GB9 负责 ZWJ 前不断，GB11 负责合格 ZWJ 后接 ExtPict 不断，原家庭簇期望结果仍正确。

本次仅复跑针对性 mapping 测试，没有重跑 workspace/Clippy/格式门禁或真实 UI/IME；没有修改代码、测试、任务状态或他人工作树改动。
