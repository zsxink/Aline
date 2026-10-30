# 任务 3.2：tuple 边界与统一映射分型

日期：2026-10-31。状态：实现并验证受检 tuple 构造、恒等映射与实际编译失败夹具，等待主 Agent 勾选；不代表补丁发布、历史、完整 Markdown 映射或 DocCore 完成。

## 已实现

CheckedBatch 私有持有原形 `Vec<(Range<usize>, String)>`、before SnapshotId/revision 和受检 output_len，仅提供不可变切片。构造检查全部端点、UTF-8、顺序、重叠、同点多插入、数量/插入/输出预算及长度溢出；validate 不凭同文本放行另一个快照或版本。该类型不 apply、写盘或持历史。

Affinity 位于 buffer 坐标层；markdown 的 Mapped<T> 携来源 SrcRange、精确性和方向。Projection 的 source_at/drawn_at/source_from_utf16/utf16_from_source/aligned 对恒等投影真实转换并校验身份，局部差及 UTF-16 索引只在映射叶模块内。查询使用一次构造的边界索引，不逐次物化或重扫全文。mapped 精确性目前只产生 Exact，Aligned/Hidden/Synthetic 的实际来源关系与复杂解析留第 6 组。

`前甲😀后` 的源码边界为 0/3/6/10/13；局部源码 3..10 投影 `甲😀` 的绘制边界为 0/3/7，UTF-16 为 0/1/3，局部结尾对应全文 UTF-16 4。外部测试直接使用这些手算金样，验证双向转换、来源范围、反向选区及对齐幂等；半 UTF-8、代理对中间、越界、局部范围外、错投影及同文本新快照拒绝。UI 信封另验证错文档、草稿版本及同版本新 SnapshotId 的拒绝。

## 实际编译正反例

`python3 scripts/check_public_api.py --self-test --output-dir <目录>` 生成独立 Cargo workspace。先 `cargo generate-lockfile --offline`，再 `cargo check --locked --offline`；不下载，所有夹具显式 unsafe_code=forbid。

| 夹具 | 预期实际结果 |
|---|---|
| 正常三坐标/StorageRange/tuple 只读消费 | exit 0 |
| DrawnOffset 当 SrcOffset | exit 101，E0308 及两个类型名 |
| DrawnOffset 当 Utf16Offset | exit 101，E0308 及两个类型名 |
| SrcRange 当 StorageRange | exit 101，E0308 及两个类型名 |
| 直接填 SrcOffset 私有字段 | exit 101，E0451/private |
| 从 Arc 伪造 SnapshotId | exit 101，E0423/private |
| 从 Arc 伪造 ProjectionId | exit 101，E0423/private |
| 经 patches() 修改字符串 | exit 101，E0596/cannot borrow |

反例必须匹配具体诊断，lock/依赖编译失败不算边界通过；没有运行非法程序。源文件、原始输出、每条完整命令/环境/UTC 时间/退出码及 summary 在 step-3.2-reviewed/compile-fixtures/。初版夹具未显式声明 unsafe forbid，独立 reviewer 指出后补齐并实际重跑；旧证据保留，不以旧版冒充最终配置。

## 实际检查与范围

先写 checked_batch 与 identity_mapping 外部金样，初次因类型/错误分支未实现以 E0432/E0599、exit 101 失败，见 step-3.2/initial.*。实现后 buffer、markdown 定向 tests、编译正反例、全 workspace Clippy 和安全扫描首轮通过，见 step-3.2-first/。

最后补局部投影范围外和 UI 同版本新快照拒绝，fmt --check、全 workspace Clippy/test 实际退出 0，见 step-3.2-final/。新增编译检查器与安全扫描、CodeGraph 的最终记录在 step-3.2-reviewed/。依赖未改，3.1 已执行的正式七组合完整闭包结果继续适用，阶段整合仍由主 Agent 再核。

该检查器需要接入统一入口；主 Agent 已收到接线说明。完整 parse/rewrite/HTML/实体、apply/history、plan/validate 和 DocCore 仍为后续任务，未造占位成功方法；真实 UI/IME/后台调度/保存/其他平台及 V-01～V-09 未验证。
