#!/bin/sh
# 组 5 低层边界人工审查辅助：只读扫描源码、清单与所有权声明。
set -eu
ROOT=${1:-/Users/xian/Project/editor/Aline}
cd "$ROOT" || exit 2

echo '# 1. I/O 与 unsafe/libc 扫描（空结果表示无命中）'
rg -n 'std::fs|std::io|File::|OpenOptions|read_to_|write_all|unsafe|libc' \
  crates/aline-buffer/src crates/aline-buffer/Cargo.toml || true

echo '# 2. 解析依赖/符号扫描（doc 注释中的 markdown 字样不是解析器）'
rg -n 'pulldown|comrak|markdown|serde' \
  crates/aline-buffer/src crates/aline-buffer/Cargo.toml || true

echo '# 3. 哈希/指纹扫描'
rg -n 'DefaultHasher|std::hash|Hasher|sha[0-9]|md5|blake' \
  crates/aline-buffer/src crates/aline-buffer/Cargo.toml || true

echo '# 4. 依赖清单'
sed -n '1,80p' crates/aline-buffer/Cargo.toml
cargo tree -p aline-buffer --edges normal --locked --offline

echo '# 5. StringBuffer 唯一持有与 History Arc 共享点'
rg -n 'pub struct StringBuffer|snapshot: TextSnapshot|selection: SrcSelection|line_index: LineIndex|history: History' \
  crates/aline-buffer/src/string_buffer.rs
rg -n 'pub struct HistoryStep|pub struct HistoryStepHandle|steps: Vec<Arc<HistoryStep>>|forward: Arc|inverse: Arc|Arc::clone' \
  crates/aline-buffer/src/history.rs crates/aline-buffer/src/string_buffer.rs

echo '# 6. apply/undo/redo/reload 中发布点顺序（先构造后赋值）'
rg -n 'fn apply|fn undo|fn redo|fn reload|prepare_record|commit_record|commit_undo|commit_redo|self\.snapshot =|self\.history\.commit' \
  crates/aline-buffer/src/string_buffer.rs
