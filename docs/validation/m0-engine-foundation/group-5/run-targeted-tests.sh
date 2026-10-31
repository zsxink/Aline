#!/bin/zsh
# 组 5 定向测试证据：只运行已提交的 buffer 测试，不改代码、文档或金样。
set -u
ROOT=/Users/xian/Project/editor/Aline
BASE=$ROOT/docs/validation/m0-engine-foundation/group-5

export CARGO_HOME=${CARGO_HOME:-/Users/xian/Library/Caches/aline-m0/cargo-home}
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=$ROOT/target

run() {
  local label=$1
  local outdir=$2
  shift 2
  mkdir -p "$outdir"
  local start=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  "$@" >"$outdir/$label.stdout.log" 2>"$outdir/$label.stderr.log"
  local rc=$?
  local end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '%s\t%s\t%s\texit=%s\n' "$start" "$end" "$*" "$rc" >>"$BASE/commands.log"
  print -r -- "$label: exit=$rc"
}

: >"$BASE/commands.log"
run environment "$BASE/evidence" zsh -c 'date -u; uname -a; rustc -Vv; cargo -V; printenv CARGO_HOME RUSTUP_TOOLCHAIN CARGO_NET_OFFLINE'
run git-state "$BASE/evidence" zsh -c "git -C $ROOT rev-parse HEAD; git -C $ROOT status --porcelain"
run tests-list "$BASE/evidence" cargo test -p aline-buffer --locked --offline -- --list
run patch "$BASE/step-5.1/targeted-tests" cargo test -p aline-buffer --locked --offline patch
run rejections "$BASE/step-5.2/targeted-tests" cargo test -p aline-buffer --locked --offline --test rejections
run anchors "$BASE/step-5.3/targeted-tests" cargo test -p aline-buffer --locked --offline anchors
run history "$BASE/step-5.4/targeted-tests" cargo test -p aline-buffer --locked --offline --test history
run full-crate "$BASE/step-5.4/targeted-tests" cargo test -p aline-buffer --locked --offline
run audit "$BASE/step-5.3" sh "$BASE/step-5.3/run-audit.sh" "$ROOT"
