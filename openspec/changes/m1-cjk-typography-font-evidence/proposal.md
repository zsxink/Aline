# Proposal

## Why

QMD-01.6 已要求先测量中文排版再决定是否修改上游；现有 V-04 字体激活/失败证据不能回答全角标点避头尾、合成斜体与 CJK 标点宽度。本变更是 QMD-01 W2 的独立立项，落实 M1 的测量前置，不代替 M1 核心出口。

## What Changes

- 增加独立中文排版夹具，覆盖全角开闭标点、中英/emoji/组合字符与正常/粗体/斜体/粗斜体；测量正文实际宽度 320/480/768 px 的 Visual/Source 六个组合。
- 增加只接受完整真实观测的证据核对入口，缺截图/换行/行高/回退/运行身份时失败；生成夹具和空观测模板不算测量通过。
- 核对 renderer 当前换行路径，记录实测能力及最小失败输入；只有实测后才能作上游/vendor/Aline 布局/已知限制的选择。M1 已知限制是否可接受仍须具体裁决。
- 复用现有字体注册与布局代次机制及 M1 任务 6.4 的历史证据；不重新把历史结果写成当前测试。

### Non-Goals

不 vendor/fork/升级 GPUI，不引入字体或新依赖，不预折行、不改变源码映射，不改变字体平滑、视觉/schema 或冻结 14° 等参考数值；三平台完整字体与主题包矩阵归 M4/M6。

## Capabilities

### New Capabilities

无。

### Modified Capabilities

- `workspace/engineering-gates`：增加 W2 中文排版分项证据的完整性与真实平台边界。

## Impact

仅新增 `scripts/fixtures/m1-cjk-typography.md`、`scripts/check_m1_cjk_typography.py` 及本 change。所有运行产物经 `scripts/evidence_paths.py` 写入 `logs/validation/m1-single-window-readonly/<run-id>/w2/`。遵守 CODE_STANDARDS §2～6、架构 R-10/11/17、Rust S-14 与 UI §3.2；基线为开发流程 §3.1、决定 §25 QMD-01.6、V-04、quiet-md 调研 W2（仅研究线索）。不改变生产代码或依赖图。

### Risks and Evidence

源码测量或 JSON 完整性不能证明真实字形；截图只能证明所列平台/尺寸当时可见结果，无法替代字体实际 run 的记录。验收需要六组合真实截图、实际内容宽度/缩放/字体/行高、行首行尾与回退观察、合成斜体与平滑结论；缺证据或能力缺口时保留任务未完成，不擅定已知限制。
