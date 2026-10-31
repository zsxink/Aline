#!/bin/zsh
# 任务 3.3 提交前的三条直接门禁；只读检查，不修改代码或文档。
set -u
ROOT=/Users/xian/Project/editor/Aline
OUT=$ROOT/docs/validation/m0-engine-foundation/group-3/step-3.3/gates

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
run fmt cargo fmt --all --check
run clippy cargo clippy --workspace --all-targets -- -D warnings
run test cargo test --workspace --locked --offline
