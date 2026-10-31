#!/usr/bin/env bash
# 人工写入通道审查（可复现重跑脚本）。
# 复核 F3-3：原日志只有结果、没有命令；本脚本逐条记录命令、输出与退出码。
# rg 无匹配时退出码为 1（表示“未找到”），不是错误。
set -u

cd "$(dirname "$0")/../../../../../../" || exit 1

log() { printf '%s\n' "$*"; }
run() {
  local cmd="$1"
  log "### \$ $cmd"
  local out
  out=$(eval "$cmd" 2>&1)
  local code=$?
  if [ -n "$out" ]; then printf '%s\n' "$out" | sed 's/^/    /'; fi
  log "    [exit=$code]"
  log ""
}

log "# 人工写入通道审查（可复现重跑）"
log "# UTC: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "# HEAD: $(git rev-parse HEAD)"
log ""

run 'rg -n "&mut StringBuffer|StringBuffer::apply|\.apply\(|\.reload\(|\.undo\(|\.redo\(" crates/aline-edit/src'
run 'rg -n "CheckedBatch::new" crates/aline-edit/src'
run 'rg -n "std::fs|OpenOptions|fs::write|File::create" crates/aline-edit/src crates/aline-buffer/src crates/aline-markdown/src crates/aline-ui/src'
run 'rg -n "^pub fn |^pub struct |^pub enum |^pub trait " crates/aline-edit/src/lib.rs'
run 'rg -n "aline-buffer|aline-markdown|aline-source|aline-plugin|gpui" crates/aline-edit/Cargo.toml'
run 'rg -n "fn .*&mut self" crates/aline-edit/src'
