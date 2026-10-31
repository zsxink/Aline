#!/usr/bin/env python3
"""Read-only relative-link checker for the Aline docs tree.

Extracts markdown inline link targets of the form ](target) from every .md file
under docs/, resolves relative targets against the containing file's directory,
and reports whether they exist. External URLs (scheme:), pure anchors and
absolute filesystem paths are skipped and counted separately.
"""
import os
import re
import sys

ROOT = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
DOCS = os.path.join(ROOT, "docs")
LINK = re.compile(r"\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
FENCE = re.compile(r"```.*?```|~~~.*?~~~", re.S)
CODE = re.compile(r"`[^`\n]*`")


def strip_code(text):
    text = FENCE.sub("", text)
    return CODE.sub("", text)

total = 0
checked = 0
missing = []
skipped_external = 0
skipped_anchor = 0

for dirpath, _dirnames, filenames in os.walk(DOCS):
    for name in sorted(filenames):
        if not name.endswith(".md"):
            continue
        path = os.path.join(dirpath, name)
        with open(path, encoding="utf-8") as handle:
            text = strip_code(handle.read())
        for match in LINK.finditer(text):
            total += 1
            target = match.group(1)
            if target.startswith("#"):
                skipped_anchor += 1
                continue
            if "://" in target or target.startswith("mailto:"):
                skipped_external += 1
                continue
            if target.startswith("/"):
                skipped_external += 1
                continue
            clean = target.split("#", 1)[0].split("?", 1)[0]
            if not clean:
                skipped_anchor += 1
                continue
            checked += 1
            resolved = os.path.normpath(os.path.join(dirpath, clean))
            if not os.path.exists(resolved):
                missing.append((os.path.relpath(path, ROOT), target))

print(f"docs_md_files_scanned={sum(1 for d,_,fs in os.walk(DOCS) for f in fs if f.endswith('.md'))}")
print(f"link_targets_total={total}")
print(f"relative_checked={checked}")
print(f"skipped_external_or_absolute={skipped_external}")
print(f"skipped_anchor_only={skipped_anchor}")
print(f"missing_count={len(missing)}")
for source, target in missing:
    print(f"MISSING\t{source}\t{target}")

group_links = []
for dirpath, _dirnames, filenames in os.walk(DOCS):
    for name in filenames:
        if not name.endswith(".md"):
            continue
        path = os.path.join(dirpath, name)
        with open(path, encoding="utf-8") as handle:
            text = strip_code(handle.read())
        for match in LINK.finditer(text):
            target = match.group(1)
            if "group-6" in target or "group-7" in target:
                resolved = os.path.normpath(os.path.join(dirpath, target.split("#", 1)[0]))
                group_links.append(
                    (os.path.relpath(path, ROOT), target, os.path.exists(resolved))
                )
print(f"group_links={len(group_links)}")
for source, target, exists in group_links:
    print(f"GROUP_LINK\t{'OK' if exists else 'MISSING'}\t{source}\t{target}")
