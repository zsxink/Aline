# R08 / F-11 纯拒绝负例

## 范围与结果

只改 `crates/aline-edit/tests/refusals.rs`，未改生产代码或 UI。为既有拒绝测试补充表驱动场景：脚注引用空替换删除 1 例、非空粘贴意图 1 例；HTML 格式选区右端切入关闭标签 1 例、选区跨越整个元素 1 例。四例分别断言拒绝类型/精确范围、调用前后完整 State（源码、anchor/head、revision、undo/redo 深度、历史节点数）相等，并确认原 redo 分支仍可消费。

未发现实现缺陷。首次运行中的 SelectionMismatch 来自测试选区与格式命令范围不一致；随后失败显示 5..7 按当前闭区间触及规则首先碰到 `<b>` 的 2..5。根据源码字节位置改用 6..7，使右端穿过 `</b>` 起点而不触及前标签；得到 HtmlBoundary 6..10。此处只调整测试预期/输入，没有放宽生产校验。

## 独立预期与字节推导

夹具 `before [^1] after\n\n[^1]: note\n`：按 UTF-8 ASCII 字节手算，正文前缀 `before ` 为 0..7，引用 `[^1]` 为 7..11。删除用 7..11、expected `[^1]`、replacement 空串；粘贴替换意图用 0..11、expected `before [^1]`、replacement `BEFORE pasted`。两者都应拒绝 `ProtectedRange { FootnoteReference, 7..11 }`，状态与 redo 分支不变。

夹具 `a <b>x</b> c\n`：`<b>` 字节 2..5，内容 `x` 为 5..6，`</b>` 为 6..10。范围 6..7 右端切入关闭标签，预期 `HtmlBoundary { range: 6..10 }`；范围 1..11 覆盖整个元素及两侧标签，按解析保护范围顺序首先触及开标签，预期 `HtmlBoundary { range: 2..5 }`。各范围及返回错误都由测试直接断言，没有从运行输出生成预期。

## 验证

- `cargo test -p aline-edit --test refusals --locked --offline`：最终 16 passed，exit 0。
- `cargo clippy -p aline-edit --test refusals --locked --offline -- -D warnings`：exit 0。
- `cargo fmt --all --check`：最终 exit 0。
- `git diff --check`：exit 0。
- 每条命令实际 argv、cwd、UTC 起止、退出码与 stdout/stderr 均保存在同目录 `*.command.json`、`*.stdout.log`、`*.stderr.log`。首次测试与首次格式检查的非零尝试也原样保留。

身份：命令运行时 HEAD `96024867f7ae949d7328232b05d93ed5bc6033d9`；最终受测 `refusals.rs` SHA-256 `0abf16b447c5f73066008d71869a45be152ddc14f153927454ffa7241423a070`。环境 `CARGO_NET_OFFLINE=true`、`RUSTUP_TOOLCHAIN=1.98.1`。本批未运行 workspace 全量门禁，留 R14。
