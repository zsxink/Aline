#!/bin/zsh
# 组 7 定向测试与门禁证据：只读取/运行现有代码，不改代码、文档或金样。
set -eu
ROOT=/Users/xian/Project/editor/Aline
BASE=$ROOT/docs/validation/m0-engine-foundation/group-7

export CARGO_HOME=${CARGO_HOME:-/Users/xian/Library/Caches/aline-m0/cargo-home}
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR=$ROOT/target
export PYTHONDONTWRITEBYTECODE=1

run() {
  local label=$1
  local outdir=$2
  shift 2
  mkdir -p "$outdir"
  local start=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  local rc=0
  "$@" >"$outdir/$label.stdout.log" 2>"$outdir/$label.stderr.log" || rc=$?
  local end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '%s\t%s\t%s\t%s\n' "$start" "$end" "$rc" "$*" >>"$BASE/commands.log"
  print -r -- "$label: exit=$rc"
}

run environment "$BASE/step-7.4" zsh -c 'date -u; uname -a; rustc -Vv; cargo -V; printenv CARGO_HOME RUSTUP_TOOLCHAIN CARGO_NET_OFFLINE CARGO_TARGET_DIR'
run git-state "$BASE/step-7.4" zsh -c "git -C $ROOT rev-parse HEAD; git -C $ROOT status --porcelain"
run tests-list "$BASE/step-7.4" cargo test -p aline-edit --locked --offline -- --list
run goldens "$BASE/step-7.1/targeted-tests" cargo test -p aline-edit --locked --offline --test goldens
run validate-reread "$BASE/step-7.2/targeted-tests" cargo test -p aline-edit --locked --offline --test goldens golden_validate
run consume "$BASE/step-7.2/targeted-tests" cargo test -p aline-edit --locked --offline --test goldens golden_consume
run refusals "$BASE/step-7.3/targeted-tests" cargo test -p aline-edit --locked --offline --test refusals
run protection "$BASE/step-7.3/targeted-tests" cargo test -p aline-edit --locked --offline --test protection
run full-crate "$BASE/step-7.4/targeted-tests" cargo test -p aline-edit --locked --offline
run workspace-gates "$BASE/step-7.4" python3 -I "$ROOT/scripts/check_workspace.py" \
  --cargo-home "$CARGO_HOME" \
  --output-dir "$BASE/step-7.4/workspace-gates"
