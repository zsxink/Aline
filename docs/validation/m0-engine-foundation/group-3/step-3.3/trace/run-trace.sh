#!/bin/zsh
# 任务 3.3 接口基础记录的命令与输出留证；只读扫描与既有测试，不修改代码。
set -u
ROOT=/Users/xian/Project/editor/Aline
OUT=$ROOT/docs/validation/m0-engine-foundation/group-3/step-3.3/trace

export CARGO_HOME=${CARGO_HOME:-$HOME/Library/Caches/aline-m0/cargo-home}
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=$ROOT/target

run() {
  local label=$1
  shift
  local start=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  "$@" >"$OUT/$label.stdout.log" 2>"$OUT/$label.stderr.log"
  local code=$?
  local end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '%s\t%s\t%s\texit=%s\n' "$start" "$end" "$*" "$code" >>"$OUT/commands.log"
  print -r -- "$label: exit=$code"
}

: >"$OUT/commands.log"

run environment zsh -c 'date -u; uname -a; rustc -Vv; cargo -V; printenv CARGO_HOME'
run git-state zsh -c "git -C $ROOT rev-parse HEAD; git -C $ROOT status --porcelain"

run public-surface rg -n -e '^\s*pub (struct|enum|fn|const fn|type|use|mod)' "$ROOT/crates" "$ROOT/app" -g '*.rs'

run mapping-consumers-all rg -n -e 'source_at|drawn_at|\.aligned\(|source_from_utf16|utf16_from_source|drawn_offset|utf16_offset|drawn_range|utf16_range' "$ROOT/crates" "$ROOT/app" -g '*.rs'
grep -v '/src/mapping/' "$OUT/mapping-consumers-all.stdout.log" | grep -v '/tests/' >"$OUT/mapping-consumers-outside-mapping.log"

run utf16-all rg -n -e 'utf16|Utf16|encode_utf16|len_utf16' "$ROOT/crates" "$ROOT/app" -g '*.rs'
grep -v '/src/mapping/' "$OUT/utf16-all.stdout.log" | grep -v '/tests/' >"$OUT/utf16-outside-mapping.log"

run coordinate-arithmetic-all rg -n -e '- *[a-zA-Z_]+\.(start|end)' "$ROOT/crates" "$ROOT/app" -g '*.rs'
grep -v '/src/mapping/' "$OUT/coordinate-arithmetic-all.stdout.log" | grep -v '/crates/aline-buffer/src/' >"$OUT/coordinate-arithmetic-outside-buffer-mapping.log"

run subtraction-calls-all rg -n -e 'checked_sub|saturating_sub|wrapping_sub' "$ROOT/crates" "$ROOT/app" -g '*.rs'
grep -v '/src/mapping/' "$OUT/subtraction-calls-all.stdout.log" | grep -v '/crates/aline-buffer/src/' >"$OUT/subtraction-calls-outside-buffer-mapping.log"

run range-field-use rg -n -e '\.(start|end)' "$ROOT/crates/aline-ui" "$ROOT/crates/aline-edit" "$ROOT/crates/aline-plugin" "$ROOT/crates/aline-app-core" "$ROOT/app" -g '*.rs'

run consumer-types rg -n -e 'TextSnapshot|StorageSnapshot|CheckedBatch|InputIdentity' "$ROOT/crates" "$ROOT/app" -g '*.rs'

run workspace-tests-list cargo test --workspace --locked --offline -- --list
run external-consumer cargo test --manifest-path "$ROOT/docs/validation/m0-engine-foundation/group-3/review-public-consumer/Cargo.toml" --locked --offline
