#!/usr/bin/env python3
"""生成确定性的 M1 解析/滚动输入；大样本只写入本地 evidence 目录。"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path

SEED = 0xA11E_2026
SIZES = {
    "32KiB": 32 * 1024,
    "128KiB": 128 * 1024,
    "1MiB": 1024 * 1024,
    "4MiB": 4 * 1024 * 1024,
    "5MiB": 5 * 1024 * 1024,
}
PROFILE_SIZE = 128 * 1024


def _patterns(seed: int) -> dict[str, bytes]:
    rng = random.Random(seed)
    words = ["alpha", "beta", "gamma", "delta", "epsilon"]
    short_blocks = "".join(
        f"## Section {index:05d}\n\n{rng.choice(words)} {index:05d}.\n\n"
        for index in range(10000)
    )
    return {
        "many-short-blocks": short_blocks.encode(),
        "single-giant-paragraph": ("single paragraph word " * 100000).encode(),
        "long-line": ("0123456789abcdef" * 100000).encode(),
        "giant-table": (
            "| left | right |\n| --- | --- |\n"
            + "| repeated cell value | another repeated value |\n" * 10000
        ).encode(),
        "dense-entities": ("&amp;&lt;&gt;&quot;&#39; &#x1F642; " * 20000).encode(),
    }


def _fixed_size(content: bytes, size: int) -> bytes:
    if len(content) >= size:
        return content[:size]
    return content + (b"x" * (size - len(content)))


def generate(output_dir: Path) -> list[dict[str, object]]:
    output_dir.mkdir(parents=True, exist_ok=True)
    patterns = _patterns(SEED)
    cases: list[tuple[str, int, bytes]] = []
    profiles = list(patterns)
    for index, (size_name, size) in enumerate(SIZES.items()):
        profile = profiles[index % len(profiles)]
        cases.append((f"size-{size_name}-{profile}", size, patterns[profile]))
    for profile, content in patterns.items():
        cases.append((f"complexity-{profile}", PROFILE_SIZE, content))

    manifest: list[dict[str, object]] = []
    for name, size, content in cases:
        data = _fixed_size(content, size)
        path = output_dir / f"{name}.md"
        path.write_bytes(data)
        manifest.append(
            {
                "name": name,
                "size_bytes": len(data),
                "seed": SEED,
                "sha256": hashlib.sha256(data).hexdigest(),
                "first_32_bytes_hex": data[:32].hex(),
                "last_32_bytes_hex": data[-32:].hex(),
            }
        )
    (output_dir / "expected.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    manifest = generate(args.output_dir)
    print(f"generated {len(manifest)} deterministic samples (seed={SEED})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
