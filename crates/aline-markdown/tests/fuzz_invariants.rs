//! W3 Markdown：语法种子和逐步源码变更；对公共全量结果检查分区、映射和复用候选。

mod support;
use support as fuzz;

use aline_buffer::{Affinity, TextBudget, TextSnapshot};
use aline_markdown::{MappingPrecision, ParseBudget, ParseConfig, Parsed, parse, reuse};
use fuzz::{Config, Run, TestResult, boundaries, invalid};

const LIMIT: usize = 1 << 20;
const CHUNKS: &[&str] = &[
    "# 中文🙂\n\n",
    "**粗体** &amp; &#x4e2d;\n\n",
    "---\ntitle: 中文\n---\n\n",
    "> [!NOTE]\n> 引用\n\n",
    "- 一\n  - 二\n\n",
    "```rust\nlet a = 1;\n```\n\n",
    "| 甲 | b |\n| :- | -: |\n| 中 | a\\|b |\n\n",
    "[^Note] 引用\n\n[^note]: 定义\n\n",
    "<span>中文 &amp; emoji 👩‍💻</span>\n\n",
    "[toc]\n\n",
    "a\r\nb\rc\n\n",
    "$$x$$\n\n",
];

fn parsed(text: &str) -> TestResult<Parsed> {
    let snapshot = TextSnapshot::new(text.to_owned(), TextBudget::new(LIMIT))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

fn property(text: &str) -> TestResult<Parsed> {
    let parsed = parsed(text)?;
    let again = parse(parsed.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
    if parsed.dump_tree() != again.dump_tree() {
        return Err(invalid("同快照全量解析不稳定"));
    }
    let mut ranges = parsed
        .blocks()
        .iter()
        .map(|block| block.range().byte_range())
        .chain(parsed.gaps().iter().map(|gap| gap.range().byte_range()))
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut covered = 0;
    for range in ranges {
        if range.start != covered || text.get(range.clone()).is_none() {
            return Err(invalid("块+Gap 未精确分区"));
        }
        covered = range.end;
    }
    if covered != text.len() {
        return Err(invalid("块+Gap 未覆盖全文"));
    }
    let projection = parsed.projection();
    let mut drawn_covered = 0;
    for segment in projection.segments() {
        let source = segment.source_range().byte_range();
        let drawn = segment.drawn_range();
        if text.get(source.clone()).is_none()
            || projection.drawn_text().get(drawn.clone()).is_none()
        {
            return Err(invalid("映射端点不是有效字符边界"));
        }
        if !drawn.is_empty() {
            if drawn.start != drawn_covered {
                return Err(invalid("非空绘制映射未连续覆盖"));
            }
            drawn_covered = drawn.end;
        }
        if segment.precision() == MappingPrecision::Exact {
            // 只对精确段内点要求双射；实体、隐藏及公共端点有 affinity 语义。
            for local_off in boundaries(
                projection
                    .drawn_text()
                    .get(drawn.clone())
                    .ok_or("绘制片段缺失")?,
            ) {
                let drawn_off = drawn.start + local_off;
                if drawn_off == drawn.start || drawn_off == drawn.end {
                    continue;
                }
                let drawn_point = projection.drawn_offset(drawn_off)?;
                for affinity in [Affinity::Before, Affinity::After] {
                    let mapped = projection.source_at(&drawn_point, affinity)?;
                    let reverse = projection.drawn_at(mapped.value(), affinity)?;
                    if reverse.value().byte_offset() != drawn_off {
                        return Err(invalid("精确段内点不互逆"));
                    }
                }
            }
        }
    }
    if drawn_covered != projection.drawn_text().len() {
        return Err(invalid("映射遗漏绘制文本"));
    }
    for drawn_off in boundaries(projection.drawn_text()) {
        for affinity in [Affinity::Before, Affinity::After] {
            let mapped = projection.source_at(&projection.drawn_offset(drawn_off)?, affinity)?;
            parsed.snapshot().validate_offset(mapped.value())?;
        }
    }
    for protected in parsed.protected_ranges() {
        parsed.validate_range(protected.range())?;
    }
    let source = parsed.snapshot().slice(
        &parsed.snapshot().src_range(0..text.len())?,
        TextBudget::new(LIMIT),
    )?;
    if source.as_bytes() != text.as_bytes() {
        return Err(invalid("解析改变源码字节"));
    }
    Ok(parsed)
}

#[test]
fn markdown_fuzz_keeps_partition_mapping_and_full_parse_reference() -> TestResult {
    let config = Config::read()?;
    for seed_index in 0..config.seeds {
        let mut run = Run::new(config.seed + u64::try_from(seed_index)?, config.trace);
        let result = (|| -> TestResult {
            let mut text = String::new();
            for _ in 0..4 {
                text.push_str(CHUNKS[run.below(CHUNKS.len())?]);
            }
            run.record(0, format!("initial source={text:?}"));
            let mut previous = property(&text)?;
            for step in 0..config.steps {
                let points = boundaries(&text);
                let a = run.below(points.len())?;
                let b = run.below(points.len())?;
                let source = points[a.min(b)]..points[a.max(b)];
                let replacement = CHUNKS[run.below(CHUNKS.len())?];
                run.record(step, format!("replace {source:?} {replacement:?}"));
                text.replace_range(source, replacement);
                let current = match property(&text) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let failed_property = error.to_string();
                        let minimum = fuzz::minimize(text.as_bytes().to_vec(), |bytes| {
                            std::str::from_utf8(bytes).is_ok_and(|candidate| {
                                property(candidate)
                                    .is_err_and(|error| error.to_string() == failed_property)
                            })
                        });
                        return Err(run.failure(
                            &format!("{error}; 单字节删除最小输入={minimum:?}"),
                            text.as_bytes(),
                        ));
                    }
                };
                let candidates = reuse(&previous, &current);
                if candidates.refusal().is_some() {
                    run.check(candidates.is_empty(), "拒绝不得保留候选", text.as_bytes())?;
                }
                for pair in candidates.reused() {
                    let old_range = pair.old_range().byte_range();
                    let new_range = pair.new_range().byte_range();
                    let old_bytes = previous
                        .snapshot()
                        .slice(pair.old_range(), TextBudget::new(LIMIT))?;
                    let new_bytes = current
                        .snapshot()
                        .slice(pair.new_range(), TextBudget::new(LIMIT))?;
                    run.equal(
                        &old_bytes.as_bytes(),
                        &new_bytes.as_bytes(),
                        "复用块字节必须相同",
                        text.as_bytes(),
                    )?;
                    let old_block = previous
                        .blocks()
                        .get(pair.old_index())
                        .ok_or("旧复用块缺失")?;
                    let new_block = current
                        .blocks()
                        .get(pair.new_index())
                        .ok_or("新复用块缺失")?;
                    run.equal(
                        &old_block.kind(),
                        &new_block.kind(),
                        "复用块种类",
                        text.as_bytes(),
                    )?;
                    let old_projection = previous.projection();
                    let new_projection = current.projection();
                    let drawn_base = |parsed: &Parsed, src_off| -> TestResult<usize> {
                        Ok(parsed
                            .projection()
                            .drawn_at(&parsed.snapshot().src_offset(src_off)?, Affinity::Before)?
                            .value()
                            .byte_offset())
                    };
                    let old_drawn_base = drawn_base(&previous, old_range.start)?;
                    let new_drawn_base = drawn_base(&current, new_range.start)?;
                    // 使用全量新投影作权威参考，逐个有效源码端点验证重定位后的映射。
                    // 数值相减仅产生测试签名，不作为生产坐标转换或导航输入。
                    for local_off in boundaries(&old_bytes) {
                        if local_off == 0 || local_off == old_bytes.len() {
                            continue;
                        }
                        let old_source = previous
                            .snapshot()
                            .src_offset(old_range.start + local_off)?;
                        let new_source = pair.translate(current.snapshot(), &old_source)?;
                        for affinity in [Affinity::Before, Affinity::After] {
                            let old_drawn = old_projection.drawn_at(&old_source, affinity)?;
                            let new_drawn = new_projection.drawn_at(&new_source, affinity)?;
                            run.equal(
                                &old_drawn.precision(),
                                &new_drawn.precision(),
                                "复用映射精确性",
                                text.as_bytes(),
                            )?;
                            // Hidden 无可见位置；全局相邻表格的 Synthetic 锚点可以改变
                            // 隐藏区映射落点，不能把它冒称块内部绘制坐标等价。
                            if old_drawn.precision() == MappingPrecision::Hidden {
                                continue;
                            }
                            run.equal(
                                &(
                                    old_drawn.value().byte_offset().checked_sub(old_drawn_base),
                                    old_drawn.precision(),
                                ),
                                &(
                                    new_drawn.value().byte_offset().checked_sub(new_drawn_base),
                                    new_drawn.precision(),
                                ),
                                "复用映射与全量新投影一致",
                                text.as_bytes(),
                            )?;
                        }
                    }
                    let protected_signature = |parsed: &Parsed, range: &std::ops::Range<usize>| {
                        parsed
                            .protected_ranges()
                            .iter()
                            .filter_map(|entry| {
                                let bytes = entry.range().byte_range();
                                (bytes.start >= range.start && bytes.end <= range.end).then(|| {
                                    (
                                        bytes.start - range.start..bytes.end - range.start,
                                        entry.kind(),
                                    )
                                })
                            })
                            .collect::<Vec<_>>()
                    };
                    run.equal(
                        &protected_signature(&previous, &old_range),
                        &protected_signature(&current, &new_range),
                        "复用保护范围",
                        text.as_bytes(),
                    )?;
                    let style_signature = |parsed: &Parsed, range: &std::ops::Range<usize>| {
                        parsed
                            .styled_ranges()
                            .iter()
                            .filter_map(|entry| {
                                let bytes = entry.range().byte_range();
                                (bytes.start >= range.start && bytes.end <= range.end).then(|| {
                                    (
                                        bytes.start - range.start..bytes.end - range.start,
                                        entry.style(),
                                    )
                                })
                            })
                            .collect::<Vec<_>>()
                    };
                    run.equal(
                        &style_signature(&previous, &old_range),
                        &style_signature(&current, &new_range),
                        "复用样式范围",
                        text.as_bytes(),
                    )?;
                    for (old_off, new_off) in [
                        (old_range.start, new_range.start),
                        (old_range.end, new_range.end),
                    ] {
                        let old_point = previous.snapshot().src_offset(old_off)?;
                        let translated = pair.translate(current.snapshot(), &old_point)?;
                        run.equal(
                            &translated.byte_offset(),
                            &new_off,
                            "复用端点重定位",
                            text.as_bytes(),
                        )?;
                    }
                }
                let observed = current.projection().drawn_text();
                let expected = parse(current.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
                let mut observed_bytes = observed.as_bytes().to_vec();
                if config.inject {
                    observed_bytes.pop();
                }
                if let Err(error) = run.equal(
                    &observed_bytes,
                    &expected.projection().drawn_text().as_bytes().to_vec(),
                    "全量展示差分",
                    text.as_bytes(),
                ) {
                    if config.inject {
                        let minimum = fuzz::minimize(text.as_bytes().to_vec(), |bytes| {
                            std::str::from_utf8(bytes).is_ok_and(|candidate| {
                                parsed(candidate).is_ok_and(|candidate| {
                                    let expected = candidate.projection().drawn_text().as_bytes();
                                    let mut observed = expected.to_vec();
                                    observed.pop();
                                    observed != expected
                                })
                            })
                        });
                        return Err(fuzz::invalid(format!("{error}\n最小注入输入={minimum:?}")));
                    }
                    return Err(error);
                }
                previous = current;
            }
            Ok(())
        })();
        result
            .map_err(|error| run.failure(&error.to_string(), b"input is recorded in operations"))?;
    }
    Ok(())
}
