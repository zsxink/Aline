# 任务 3.1 独立代码审查（2026-10-31）

当前实际基础代码通过独立审查，未发现未解决的已确认缺陷。主 Agent 仍须结合正式成员测试、工程守卫及文档回填决定任务勾选。本记录不判任务 3.2/3.3 或 M0 完成。

## 所审边界

- buffer：TextSnapshot、SnapshotId、DocumentRevision、SrcOffset/Range/Selection、Whole/单 Overlay backing、TextBudget。
- markdown：恒等 Projection 的独立 ProjectionId、受检绘制/UTF-16 offset/range 与受预算索引；转换方法属于 3.2，完整解析/来源映射属于 6 组。
- source：自有原始 StorageSnapshot、BaselineId、DiskVersion、StorageRange/Budget；不把原始字节视图当作已解码文件或实际磁盘写入事实。
- UI：DocumentId、DraftGeneration、WorkingVersion 与 InputIdentity 信封；不实现 DocCore、真实输入或文件协调器。

身份使用私有 Arc token 与 Arc::ptr_eq；新对象相同内容或相同版本不能冒充同一实例，clone 保持身份。Arc 是不可变共享的具体实现，不创建线程、锁或全局注册表；相较初版建议的 Rc，它使冻结数据可满足 Send+Sync 的静态消费要求。实际跨线程调度未执行。

源码/存储/绘制/UTF-16 有不同类型和所属身份。Whole 与一层 Overlay 以私有 backing 区分，构造先验身份、UTF-8、长度与预算；新草稿保留旧不可变视图并创建新身份，显式物化保留同一逻辑输入身份。低层公开面未返回可写真实 Buffer；实际唯一写所有者及历史重绑定仍待组 5/8 实现，未声称通过。

## 独立实际消费

独立夹具 `review-public-consumer/` 是隔离 Cargo workspace，不属于正式八成员，不构成新增产品 crate。

完整实际命令在 `review-public-consumer-first.command.json`：`cargo test --manifest-path <本目录>/review-public-consumer/Cargo.toml --offline`，退出码 **0**，**3 passed**；原始 stdout/stderr 与 hash 均保留。首次命令生成隔离 lockfile，未用不存在的 lock 声称执行 --locked。

独立预期为夹具中手工标定的 UTF-8 字节：`A甲B尾` 替换 1..4 为 `😀\r\n` 后精确等于 `A😀\r\nB尾`，合法边界为 0/1/5/6/7/8/11。没有调用被测实现生成预期字符串或边界表。

已实际验证：

- prefix/draft/suffix 跨段切片及上述每个 UTF-8 边界；不改 CR/LF。
- Arc::make_mut 改变外部草稿后，旧快照仍逐字相同；新草稿身份变化、旧范围拒绝。
- materialize 前后身份相同且原范围仍可用；叠套 Overlay 拒绝。
- 显式恒等 Projection 的 UTF-8/UTF-16 边界、代理对中间拒绝、最小索引预算失败。
- 相同 bytes 的不同基线不能互借 StorageRange，相同快照/版本的不同文档不能互借 InputIdentity。
- TextSnapshot、SrcRange、Projection、StorageSnapshot、InputIdentity 的 Send+Sync 泛型约束在外部消费方实际编译成功；没有启动线程。

## 尚未验证

本记录未执行 tuple 批次、source_at/drawn_at/aligned/UTF-16 转换、compile-fail 类型混传、业务解析、字节编码、Undo/Redo、草稿组状态机或 DocCore 写入口。它们属于后续指定任务；未实现方法不以占位成功结果代替。当前五引擎职责仍为无 GPUI 的纯逻辑，正式依赖守卫结果由主 Agent/实现者相应命令记录补足。

只覆盖本机 macOS aarch64；其它平台、真实输入/窗口/保存及 V-01～V-09 均未验证。
