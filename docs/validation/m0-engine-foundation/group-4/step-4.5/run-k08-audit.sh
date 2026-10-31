#!/bin/sh
# 4.5 K-08 只读人工复核脚本：核对源码偏移与磁盘/存储偏移未混用。
# 只读取源码、文档与 git 状态；不写仓库文件（输出由调用方重定向）。
set -u

ROOT=$(git rev-parse --show-toplevel) || exit 1
cd "$ROOT" || exit 1

section() {
    printf '\n===== %s =====\n' "$1"
}

run_expect() {
    label=$1
    expect=$2
    shift 2
    printf '\n$ %s\n' "$*"
    "$@"
    code=$?
    printf '[exit=%s; %s]\n' "$code" "$label (expected $expect)"
}

printf 'utc_start=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
printf 'pwd=%s\n' "$(pwd)"
printf 'head=%s\n' "$(git rev-parse --short HEAD)"
printf 'toolchain=%s\n' "$(rustc --version 2>/dev/null || echo unavailable)"

section "1. source/disk offset identifiers in aline-source"
run_expect "no src_off/disk_off identifiers" "no hits" \
    rg -n "src_off|source_off|disk_off|storage_off" crates/aline-source/src

section "2. all offset/range identifiers in aline-source"
run_expect "storage range and local write cursor only" "informational" \
    rg -n "StorageRange|SourceRange|offset" crates/aline-source/src

section "3. consumers outside aline-source"
run_expect "no external consumer yet" "no hits" \
    rg -n "aline_source|SourceBaseline|StorageSnapshot" crates --glob '!crates/aline-source/**'

section "4. StorageRange contract"
printf '\n$ sed -n 118,135p crates/aline-source/src/storage.rs\n'
sed -n '118,135p' crates/aline-source/src/storage.rs
printf '\n$ sed -n 186,214p crates/aline-source/src/storage.rs\n'
sed -n '186,214p' crates/aline-source/src/storage.rs

section "5. decode boundary"
printf '\n$ sed -n 117,167p crates/aline-source/src/decode.rs\n'
sed -n '117,167p' crates/aline-source/src/decode.rs

section "6. encode boundary"
printf '\n$ sed -n 84,134p crates/aline-source/src/encode.rs\n'
sed -n '84,134p' crates/aline-source/src/encode.rs

section "7. atomic write/publish path"
printf '\n$ sed -n 426,501p crates/aline-source/src/atomic.rs\n'
sed -n '426,501p' crates/aline-source/src/atomic.rs
printf '\n$ sed -n 252,360p crates/aline-source/src/atomic.rs\n'
sed -n '252,360p' crates/aline-source/src/atomic.rs

section "8. worked offset example: BOM + 甲 CRLF 乙"
printf '\n$ python3 -I -c <offset computation>\n'
python3 -I -c '
s = "甲\r\n乙"
b = s.encode()
d = b"\xef\xbb\xbf" + b
print("source_len", len(b))
print("source_offset_of_乙", b.index("乙".encode()))
print("disk_len", len(d))
print("disk_offset_of_乙", d.index("乙".encode()))
print("difference", d.index("乙".encode()) - b.index("乙".encode()))
'

section "9. docs backfill diff (additions only)"
printf '\n$ git diff --stat -- docs/engine-design.md docs/v1-architecture.md docs/v1-validation-plan.md\n'
git diff --stat -- docs/engine-design.md docs/v1-architecture.md docs/v1-validation-plan.md

section "10. no code or evidence mutation in this audit"
printf '\n$ git status --porcelain -- crates/\n'
git status --porcelain -- crates/
printf '[exit=%s]\n' "$?"

printf '\nutc_end=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
