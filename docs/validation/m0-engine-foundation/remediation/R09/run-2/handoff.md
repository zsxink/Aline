# R09 run-2 source-fix handoff

- Scope: added `dump_structure_checks_ignore_sensitive_words_in_drawn_text` in `crates/aline-markdown/tests/diagnostics.rs` only.
- Coverage: budget fallback body includes `Table`, `CodeBlock`, and `diagnostic`; block and diagnostic assertions are restricted to their dump line families. Mermaid code body includes `diagnostic`; the dump contains it as drawn text while the diagnostic line family stays empty and the block family reports `CodeBlock`.
- Existing structure negative intent remains in the node-budget test (`Table` / `CodeBlock` absent from block rows) and Mermaid test (no diagnostic rows).
- Verification: final `cargo test -p aline-markdown --test diagnostics --locked --offline` passed (6/6); `cargo fmt --all --check` and `git diff --check` passed. Full argv, cwd, UTC timestamps, exit codes, stdout, and stderr are recorded in `source-fix-evidence/commands.jsonl` and adjacent logs. An initial fmt check failed before applying rustfmt and is retained as entries 02–03.
- Only the diagnostics test file was modified for source changes; the shared working tree may contain unrelated ongoing work.
