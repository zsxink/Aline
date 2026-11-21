# W4 Recovery V1 goldens

These are hand-frozen wire inputs for `qmd-w4-persistence-recovery` task 1.2. The `.manifest.hex` files decode to the exact binary bytes described by Decision 0 in `openspec/changes/qmd-w4-persistence-recovery/design.md`; production parsing must not regenerate these expectations.

## `snapshot-v1`

- Header: `ALINEREC`, version 1, kind 0 (snapshot), flags 0, body length 104 bytes, body CRC-32 `0xa72a5ca6`.
- Stable document ID 1, generation 2, parent generation 1, path generation 2.
- No active composition; Unix path bytes are exactly `notes.md`.
- Reversed selection: `anchor=3`, `head=0`.
- `source.utf8` is exactly `abc`; CRC-32/ISO-HDLC is `0x352441c2`.
- `logical-saved.utf8` is `a\nb`; `baseline.bytes` is `a\r\nb`, so the fixture proves logical and storage baselines differ.
- Zero resources.
- Expected: accepted as complete generation 2 if all three sibling files match; source file remains untouched by recovery.

## Rejection inputs

- `truncated.manifest.hex`: one byte shorter than the declared header plus body length; reject without opening data paths or cleaning the generation.
- `bad-crc.manifest.hex`: valid length and payload with a corrupted body CRC; reject without opening data paths or cleaning the generation.
- `unknown-version.manifest.hex`: version 99 with otherwise valid body; reject as unsupported, retain original bytes and disable writes/cleanup for this identity.
- `oversized-source.manifest.hex`: valid framing declares a source larger than the fixed 10 MiB V1 cap; reject from the manifest before opening the source file.

## `unlinked-snapshot-v1`

- Header body length is 96 bytes and body CRC-32 is `0x52231356`.
- Stable document ID 2, generation 1, parent generation 0, path generation 0.
- `path_kind=0`, `path_len_bytes=0`, collapsed selection, source `abc`; logical and storage baseline files are present and empty.
- Expected: accepted as a recoverable untitled document when its source sibling matches.

The `.hex` format is whitespace-separated-by-line hex for review only; a test helper may decode it. It is not itself a recovery format.
