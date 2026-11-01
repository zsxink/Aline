# R07b：字素边界机制与依赖准入提案

状态：**用户已于 2026-11-01 批准方案 1；待 OpenSpec design 独立复核与实现。** 当前尚未增加依赖边或 helper，F-10 未关闭。

## 缺口与依据

R07/run-1 的手写字素集合和 scalar 有效偏移对照可核对样本，但 `grapheme_move_trajectory` 只遍历传入集合，没有从文本计算字素边界或调用 Aline 移动机制。独立 reviewer 因此确认测试增量可接受，但不能关闭 F-10 整体或声称实现了字素导航；详见 [R07 review](../run-1/review.md)。OpenSpec 的「重复文字与字素」Scenario 要求字素移动边界与标量边界分开验证；引擎 §7.3 规定移动/删除不得拆开组合字符和 emoji 序列，并建议映射服务提供字素边界适配。

## 依赖事实

- 当前锁文件含 `unicode-segmentation 1.13.3`，校验和 `c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8`。`cargo tree -i unicode-segmentation --locked --offline -e all` 的完整路径为 `unicode-segmentation → convert_case → derive_more-impl → derive_more → gpui-pre → aline-ui → aline`；现有五个引擎没有这条依赖边。把它添加到 `aline-markdown` 仍是新引擎依赖边，不能以 lockfile 已出现为准入。
- 锁定包源码元数据：版本 1.13.3、MSRV 1.85、MIT OR Apache-2.0、无运行时依赖、无 unsafe；包说明支持 Unicode 17.0.0。源码证据在本机 Cargo registry；上游 [Cargo.toml](https://github.com/unicode-rs/unicode-segmentation/blob/master/Cargo.toml) 与 [README/changelog](https://github.com/unicode-rs/unicode-segmentation/blob/master/README.md) 可复查。
- 当前 [UAX #29 Revision 49](https://www.unicode.org/reports/tr29/tr29-49.html) 对应 Unicode 18.0，并更改 GB9c。具体金样为 UTF-8 字节 `U+094D U+0915`：Unicode 17 / UAX #29 Rev.47 GB9c 边界 `[0, 3, 6]`；Unicode 18 / Rev.49 GB9c 边界 `[0, 6]`。因此 1.13.3 / Unicode 17 基线会把这两个码点间的光标位置视为合法，而 Unicode 18 不会；不得写成符合最新版 UAX，也不得用一般 Indic 样本掩盖这一差异。
- 适用准入规则：[engine-design §18.2](../../../../../engine-design.md#182-依赖准入与未解决实现风险)、[architecture §3.2](../../../../../v1-architecture.md#32-栈与依赖的状态)、[development-process §2–3](../../../../../development-process.md#2-openspec-变更流程)：当前分段依赖未准入；依赖/跨层契约须先更新 change 并独立审查，再实施。

## 建议的最小实现范围

仅向 `aline-markdown` 增加 `unicode-segmentation = 1.13.3` 的直接边；不把该 crate 暴露为公共类型、不另建 crate、不依赖 GPUI、不在 buffer 重做一份分段器。映射层基于现有 `Projection` 文本提供受检的前进/后退/边界查询，输入输出使用绑定投影身份的 `DrawnOffset`；Source 使用 `Projection::identity`，Visual 使用既有展示投影。需要返回源码位置时仍经 `source_at`，禁止手算偏移。删除范围必须另行验证完整来源及保护规则，不能把 grapheme 移动端点当成安全删除证明。

移动操作只要求光标起点已是字素边界；利用切片边界查询相邻簇，不为每键重建全文索引或重解析 Markdown。`GraphemeCursor` 对长 RI 序列可能在多个短簇间反向回扫；因此预算覆盖所有 chunk 主动扫描和 `PreContext` 回扫，不能只限制单个簇长度。方案边界定为每次查询最多扫描 4096 UTF-8 字节，chunk 至多 256 字节；跨块上下文通过 `PreContext` 提供，按每次实际交给 cursor 的字节累计计费（重复扫描也重复计费）。达到预算仍未确定相邻边界时返回类型化 `ScanBudgetExceeded`，不得截断输入、猜测边界或返回看似成功的偏移。该预算适用于相邻边界查询；构造并缓存全文分段索引不在本提案范围。

实现验收必须以独立手写字节偏移期望覆盖：长 RI 序列（总上下文超过预算）、GB9c Indic（包含上述 Unicode17 金样）、超长组合簇、合法边界和非字素边界起点。断言长 RI 与长簇在预算内拒绝且不返回偏移，短 RI/Indic 查询返回版本基线期望；还需测定常规查询的扫描上限。不能仅以库函数成功/失败作为预期。

## 用户裁决选项

1. **已批准的最小准入**：采用精确锁定的 `unicode-segmentation 1.13.3` / Unicode 17.0 作为 M0 字素算法基线，并接受上述 U+094D U+0915 边界 `[0,3,6]` 与 Unicode18 `[0,6]` 的差异、4096 字节查询预算及耗尽拒绝语义；不得声称最新 UAX 一致。此批准以 OpenSpec design/spec/task 更新并独立复核为实施前置。V-07 收口前须以该双版本金样显式核对产品要求；若 V-07 要求 Unicode18 行为或当前 UAX 一致，必须先重新准入 Unicode18 数据版本并更新此金样，不能沿用本批准。即使 V-07 接受 Unicode17，本依赖基线到新的 Unicode 主版本时也须重新审查准入和金样。
2. **未选：暂不准入**。该选项作为备选历史保留；用户已选择方案1。

无论已选方案，都不代表 V-07、系统字素/IME、真实 UI/三平台导航验收通过；这些按既有验证计划单独收口。独立 review OpenSpec design 前仍不添加 `aline-markdown` 依赖边、不修改正式设计/主规范，也不实现相应 helper。
