# 任务 3.1：公共坐标与身份 home

日期：2026-10-31。状态：已落地基础类型与实际外部消费测试，等待主 Agent 复核勾选；没有实现组 4～8 的解码、补丁发布、解析、编辑计划或 DocCore。

## 实际方案

- buffer：SnapshotId、DocumentRevision、SrcOffset/SrcRange/SrcSelection、TextSnapshot 与 TextBudget。坐标字段私有，创建时校验 UTF-8/范围，消费时复验实例身份。方向选区保留 anchor/head。Whole 持共享不可变 String，SingleOverlay 只持完整基线、受检替换范围及一个共享块草稿，禁止叠套。构造和读取不复制基线全文；跨片段 slice 只分配请求范围，materialize 仅在显式受预算消费点分配连续全文且保留身份。
- markdown：ProjectionId、DrawnOffset/DrawnRange、Utf16Offset/Utf16Range、Projection/MappingBudget。恒等投影显式复制所选源码片段并一次建立受预算的 UTF-8/UTF-16 边界索引，查询用二分检查，不每次重扫/物化全文。source_at/drawn_at/aligned 的实际转换留 3.2，复杂 Inline/实体/HTML 映射留第 6 组。
- source：BaselineId、DiskVersion、StorageRange、StorageSnapshot/StorageBudget。只接自有原始字节视图，不依赖 buffer/UI；视图并不冒充已解码文件或成功写出。真实内容证据和新基线更新留第 4 组。
- UI document：DocumentId、DraftGeneration、WorkingVersion、具体 InputIdentity。文档、工作版本、快照身份分开检查；草稿推进/提交不重置代次。没有通用消息框架、真实平台输入或文件协调器。

根据主 Agent 和独立 reviewer 的实现前复核，原建议 Rc 统一采用标准库 Arc：所有对象仍不可变共享，身份比较使用 Arc::ptr_eq，clone 保留身份，新实例不借助全局计数/路径/文本。公开草稿参数是 Arc<String>，避免将未来已确认的宿主后台输入签名锁成 !Send；没有 spawn、锁或运行期全局。外部泛型 T: Send+Sync 编译消费者只证明类型边界，不是跨线程或平台运行验证。

## 独立预期与实际命令

先写 crates/aline-buffer/tests/public_boundaries.rs 的独立输入和字节预期，初次 cargo test 因公共类型尚不存在以 E0432/exit 101 失败；原始日志在 initial-public-boundaries.*，不是通过记录。随后实现对应机制。

外部测试分别位于四个 crate 的 tests/public_boundaries.rs。主要金样：`甲😀Z` 的标量边界为 0/3/7/8；`甲-old-尾` 的 4..7 替换为 `😀` 得三个片段 `甲-`、`😀`、`-尾`，整体 12 字节；投影 `前甲😀后` 的源码 3..10 是 `甲😀`，绘制字节 0/3/7 与 UTF-16 单位 0/1/3。预期由手工 UTF-8/UTF-16 字节计算，不从实现输出回写。

实际命令/环境/UTC 时间/退出码/完整输出在 step-3.1/、step-3.1-fixed/、step-3.1-final/。四个 crate 的测试通过后，首次 Clippy 发现负例 `3..2` 触发默认 reversed_empty_ranges lint；改用显式 Range 字段表达被测坏输入，不放行 lint，修复后 Clippy 退出 0。实际 workspace 依赖守卫退出 0，五引擎仍无 GPUI，七组合解析不代表其他平台构建。

最终 fmt --check、workspace Clippy、workspace test、源码安全扫描与 CodeGraph sync 的结果保存在 step-3.1-final/。没有更改依赖、Cargo manifest、任务勾选或规范；主 Agent 根据实际结果决定 3.1 完成。

## 未执行/未验证

decode/encode 与存储内容相等证据、tuple 批次构造、全局/局部坐标转换、编译失败分型夹具分别留 4、3.2；真实补丁/历史、完整 parse、plan/validate、DocCore 留 5～8。真实后台调度、窗口、IME、保存闭环、Windows/Linux/Intel Mac 及 V-01～V-09 均未执行，不能据本项判任何完整出口通过。
