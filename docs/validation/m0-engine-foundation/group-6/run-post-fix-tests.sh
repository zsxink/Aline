#!/bin/zsh
# 组 6 修复后定向测试证据：不改代码/文档/金样，只运行测试与示例。
# 与 run-targeted-tests.sh 的差别：不截断既有 commands.log，证据写入独立目录与独立日志。
set -eu
ROOT=/Users/xian/Project/editor/Aline
BASE=$ROOT/docs/validation/m0-engine-foundation/group-6

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
  local rc=0
  "$@" >"$outdir/$label.stdout.log" 2>"$outdir/$label.stderr.log" || rc=$?
  local end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  printf '%s\t%s\t%s\texit=%s\n' "$start" "$end" "$*" "$rc" >>"$BASE/commands-post-fix.log"
  print -r -- "$label: exit=$rc"
}

: >"$BASE/commands-post-fix.log"
run git-state "$BASE/step-6.8/evidence-post-fix" zsh -c "git -C $ROOT rev-parse HEAD; git -C $ROOT status --porcelain"
run tests-list "$BASE/step-6.8/evidence-post-fix" cargo test -p aline-markdown --locked --offline -- --list
run spec "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test spec
run rewrite "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test rewrite
run mapping "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test mapping
run html "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test html
run protection "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test protection
run hostile "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test hostile
run reuse "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test reuse
run diagnostics "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline --test diagnostics
run dump-tree "$BASE/step-6.8/targeted-tests-post-fix" cargo run -q -p aline-markdown --locked --offline --example dump_tree -- crates/aline-markdown/tests/fixtures/m0/basic.md
run full-crate "$BASE/step-6.8/targeted-tests-post-fix" cargo test -p aline-markdown --locked --offline
run goldens-independent "$BASE/step-6.8" python3 -I "$BASE/verify_goldens.py" "$ROOT"
