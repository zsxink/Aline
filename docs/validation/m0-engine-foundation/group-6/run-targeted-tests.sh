#!/bin/zsh
# 组 6 定向测试证据：只运行已存在的 aline-markdown 测试与 dump 示例，不改代码、文档或金样。
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
  printf '%s\t%s\t%s\texit=%s\n' "$start" "$end" "$*" "$rc" >>"$BASE/commands.log"
  print -r -- "$label: exit=$rc"
}

: >"$BASE/commands.log"
run environment "$BASE/evidence" zsh -c 'date -u; uname -a; rustc -Vv; cargo -V; printenv CARGO_HOME RUSTUP_TOOLCHAIN CARGO_NET_OFFLINE'
run git-state "$BASE/evidence" zsh -c "git -C $ROOT rev-parse HEAD; git -C $ROOT status --porcelain"
run tests-list "$BASE/evidence" cargo test -p aline-markdown --locked --offline -- --list
run spec "$BASE/step-6.1/targeted-tests" cargo test -p aline-markdown --locked --offline --test spec
run rewrite "$BASE/step-6.2/targeted-tests" cargo test -p aline-markdown --locked --offline --test rewrite
run dump-tree "$BASE/step-6.3" cargo run -q -p aline-markdown --locked --offline --example dump_tree -- crates/aline-markdown/tests/fixtures/m0/basic.md
run mapping "$BASE/step-6.4/targeted-tests" cargo test -p aline-markdown --locked --offline --test mapping
run html "$BASE/step-6.5/targeted-tests" cargo test -p aline-markdown --locked --offline --test html
run protection "$BASE/step-6.5/targeted-tests" cargo test -p aline-markdown --locked --offline --test protection
run hostile "$BASE/step-6.6/targeted-tests" cargo test -p aline-markdown --locked --offline --test hostile
run reuse "$BASE/step-6.7/targeted-tests" cargo test -p aline-markdown --locked --offline --test reuse
run full-crate "$BASE/step-6.8/targeted-tests" cargo test -p aline-markdown --locked --offline
run goldens-independent "$BASE/step-6.8" python3 -I "$BASE/verify_goldens.py" "$ROOT"
