//! W3 buffer：本地字符串/历史 oracle 驱动补丁、撤销、重载和失败；不触碰磁盘。

mod support;
use support as fuzz;

use aline_buffer::{
    Affinity, AnchorTransform, ApplyOutcome, CheckedBatch, HistoryBudget, LineIndexBudget,
    OperationSource, PatchBudget, SelectionSpec, StringBuffer, TextBudget, transform_anchor,
};
use fuzz::{Config, Run, TestResult, boundaries};

const LIMIT: usize = 1 << 20;

#[derive(Clone)]
struct Entry {
    before: String,
    after: String,
    before_selection: SelectionSpec,
    after_selection: SelectionSpec,
}

#[test]
fn buffer_fuzz_preserves_bytes_history_and_rejections() -> TestResult {
    let config = Config::read()?;
    for seed_index in 0..config.seeds {
        let mut run = Run::new(config.seed + u64::try_from(seed_index)?, config.trace);
        let result = (|| -> TestResult {
            let mut expected = String::from("中文\r\nA👩‍💻e\u{301}\rZ");
            let mut selection = SelectionSpec::new(0, 0);
            let mut buffer = StringBuffer::new(
                expected.clone(),
                selection,
                TextBudget::new(LIMIT),
                LineIndexBudget::new(LIMIT),
                HistoryBudget::new(200_001, LIMIT * 32),
            )?;
            let mut history: Vec<Entry> = Vec::new();
            let mut cursor = 0;
            let mut revision = 0;
            let mut saved_id = None;
            for step in 0..config.steps {
                let operation = run.below(9)?;
                run.record(step, format!("operation={operation} source={expected:?}"));
                match operation {
                    0..=3 => {
                        let points = boundaries(&expected);
                        let a = run.below(points.len())?;
                        let b = run.below(points.len())?;
                        let src_range = points[a.min(b)]..points[a.max(b)];
                        let replacements = ["", "中", "\r\n", "🙂", "e\u{301}", "**x**", "\t"];
                        let replacement = replacements[run.below(replacements.len())?];
                        run.record(step, format!("replace {src_range:?} {replacement:?}"));
                        let mut after = expected.clone();
                        after.replace_range(src_range.clone(), replacement);
                        let after_selection = SelectionSpec::new(
                            src_range.start + replacement.len(),
                            src_range.start,
                        );
                        let batch = CheckedBatch::new(
                            buffer.snapshot(),
                            vec![(src_range.clone(), replacement.to_owned())],
                            PatchBudget::new(16, LIMIT, LIMIT),
                        )?;
                        let anchor = buffer.snapshot().src_offset(src_range.start)?;
                        let transformed =
                            transform_anchor(buffer.snapshot(), &anchor, &batch, Affinity::After)?;
                        run.equal(
                            &transformed,
                            &AnchorTransform::Valid {
                                byte: src_range.start + replacement.len(),
                            },
                            "插入/替换起点 affinity",
                            expected.as_bytes(),
                        )?;
                        let outcome = buffer.apply(
                            &batch,
                            after_selection,
                            OperationSource::Command,
                            u64::try_from(step)?,
                            TextBudget::new(LIMIT),
                        )?;
                        if after != expected {
                            run.check(
                                matches!(outcome, ApplyOutcome::Changed(_)),
                                "实际变更需发布",
                                expected.as_bytes(),
                            )?;
                            history.truncate(cursor);
                            history.push(Entry {
                                before: expected.clone(),
                                after: after.clone(),
                                before_selection: selection,
                                after_selection,
                            });
                            cursor += 1;
                            revision += 1;
                            selection = after_selection;
                            expected = after;
                        } else {
                            run.check(
                                matches!(outcome, ApplyOutcome::Unchanged),
                                "空变更不得发布",
                                expected.as_bytes(),
                            )?;
                        }
                    }
                    4 if cursor > 0 => {
                        buffer.undo(TextBudget::new(LIMIT))?;
                        cursor -= 1;
                        let entry = history.get(cursor).ok_or("缺少 oracle 历史")?;
                        expected = entry.before.clone();
                        selection = entry.before_selection;
                        revision += 1;
                    }
                    5 if cursor < history.len() => {
                        buffer.redo(TextBudget::new(LIMIT))?;
                        let entry = history.get(cursor).ok_or("缺少 oracle redo")?;
                        expected = entry.after.clone();
                        selection = entry.after_selection;
                        cursor += 1;
                        revision += 1;
                    }
                    6 => {
                        buffer.mark_saved();
                        saved_id = Some(buffer.snapshot().id().clone());
                    }
                    7 => {
                        let after = format!("# reload {step}\n{expected}");
                        let target = SelectionSpec::new(0, 0);
                        buffer.reload(
                            after.clone(),
                            target,
                            u64::try_from(step)?,
                            TextBudget::new(LIMIT),
                        )?;
                        history.truncate(cursor);
                        history.push(Entry {
                            before: expected.clone(),
                            after: after.clone(),
                            before_selection: selection,
                            after_selection: target,
                        });
                        cursor += 1;
                        revision += 1;
                        expected = after;
                        selection = target;
                    }
                    _ => {
                        run.check(
                            CheckedBatch::new(
                                buffer.snapshot(),
                                vec![
                                    (0..0, "X".into()),
                                    (expected.len() + 1..expected.len() + 1, "Y".into()),
                                ],
                                PatchBudget::new(16, LIMIT, LIMIT),
                            )
                            .is_err(),
                            "末项越界整批拒绝",
                            expected.as_bytes(),
                        )?;
                    }
                }
                let actual = buffer.text(TextBudget::new(LIMIT))?;
                let mut observed = actual.as_bytes().to_vec();
                if config.inject {
                    observed.pop();
                }
                if let Err(error) = run.equal(
                    &observed,
                    &expected.as_bytes().to_vec(),
                    "源码字节",
                    expected.as_bytes(),
                ) {
                    if config.inject {
                        let minimum = fuzz::minimize(expected.as_bytes().to_vec(), |bytes| {
                            std::str::from_utf8(bytes).is_ok_and(|source| {
                                let mut observed = source.as_bytes().to_vec();
                                observed.pop();
                                observed != bytes
                            })
                        });
                        return Err(fuzz::invalid(format!(
                            "{error}\n最小注入观测片段（历史重放仍需上述操作序列）={minimum:?}"
                        )));
                    }
                    // 有状态错误需要按完整操作序列最小化；不能把任意一个字节冒称真实缺陷输入。
                    return Err(error);
                }
                run.equal(
                    &buffer.revision().value(),
                    &revision,
                    "revision",
                    expected.as_bytes(),
                )?;
                run.equal(
                    &(
                        buffer.selection().anchor().byte_offset(),
                        buffer.selection().head().byte_offset(),
                    ),
                    &(selection.anchor(), selection.head()),
                    "选区",
                    expected.as_bytes(),
                )?;
                run.equal(
                    &(
                        buffer.undo_depth(),
                        buffer.redo_depth(),
                        buffer.history_node_count(),
                    ),
                    &(cursor, history.len() - cursor, history.len()),
                    "历史",
                    expected.as_bytes(),
                )?;
                run.equal(
                    &buffer.is_at_save_point(),
                    &saved_id
                        .as_ref()
                        .is_some_and(|id| id == buffer.snapshot().id()),
                    "保存点身份",
                    expected.as_bytes(),
                )?;
                // 非破坏性裁剪同样复验当前文本和历史；破坏性裁剪由独立历史金样覆盖。
                let trim = buffer.trim_history(history.len());
                run.equal(
                    &trim.removed(),
                    &0,
                    "足额裁剪不得删历史",
                    expected.as_bytes(),
                )?;
            }
            Ok(())
        })();
        result
            .map_err(|error| run.failure(&error.to_string(), b"input is recorded in operations"))?;
    }
    Ok(())
}
