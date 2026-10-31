//! 组 5 外部金样：正逆补丁、after 选区、行索引与固定种子往返。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, BufferError, CheckedBatch, HistoryBudget, LineIndex, LineIndexBudget,
    OperationSource, PatchBudget, SelectionSpec, StringBuffer, TextBudget,
};

const FORWARD_GOLDENS: &str = include_str!(
    "../../../docs/validation/m0-engine-foundation/group-5/step-5.1/goldens/forward-inverse.tsv"
);
const LINE_GOLDENS: &str = include_str!(
    "../../../docs/validation/m0-engine-foundation/group-5/step-5.1/goldens/line-index.tsv"
);

const TEXT_BUDGET: usize = 4096;
const PATCH_BUDGET: usize = 64;

type PatchList = Vec<(Range<usize>, String)>;

#[derive(Debug)]
struct ForwardCase {
    case_id: String,
    before: String,
    patches: PatchList,
    after: String,
    inverse: PatchList,
    before_anchor: usize,
    before_head: usize,
    after_anchor: usize,
    after_head: usize,
    changed: bool,
}

#[derive(Debug)]
struct LineCase {
    case_id: String,
    text: String,
    max_lines: usize,
    line_count: usize,
    ranges: Vec<Range<usize>>,
    offsets: Vec<(usize, usize)>,
    reject: bool,
}

#[test]
fn forward_inverse_patch_goldens_round_trip_after_coordinates() -> Result<(), Box<dyn Error>> {
    let cases = parse_forward_goldens()?;
    assert_eq!(cases.len(), 134, "金样总数必须与固定种子记录一致");
    for case in cases {
        let before_selection = SelectionSpec::new(case.before_anchor, case.before_head);
        let mut buffer = StringBuffer::new(
            case.before.clone(),
            before_selection,
            TextBudget::new(TEXT_BUDGET),
            LineIndexBudget::new(1024),
            HistoryBudget::new(256, 1 << 20),
        )?;
        let batch = CheckedBatch::new(
            buffer.snapshot(),
            case.patches.clone(),
            PatchBudget::new(PATCH_BUDGET, TEXT_BUDGET, TEXT_BUDGET),
        )?;
        let outcome = buffer.apply(
            &batch,
            SelectionSpec::new(case.after_anchor, case.after_head),
            OperationSource::Replace,
            1000,
            TextBudget::new(TEXT_BUDGET),
        )?;
        if !case.changed {
            assert!(
                matches!(outcome, ApplyOutcome::Unchanged),
                "{}: 无变化批次必须返回 Unchanged",
                case.case_id
            );
            assert_eq!(
                text_of(&buffer)?,
                case.before,
                "{}: 文本不得变化",
                case.case_id
            );
            assert_eq!(
                selection_spec(&buffer),
                before_selection,
                "{}: 无变化批次不得移动选区",
                case.case_id
            );
            assert_eq!(
                buffer.revision().value(),
                0,
                "{}: 不得推进代次",
                case.case_id
            );
            assert_eq!(
                buffer.history_node_count(),
                0,
                "{}: 不得产生历史",
                case.case_id
            );
            continue;
        }
        let ApplyOutcome::Changed(applied) = outcome else {
            return Err(format!("{}: 期望 Changed", case.case_id).into());
        };
        assert_eq!(
            applied.inverse_patches(),
            case.inverse.as_slice(),
            "{}",
            case.case_id
        );
        assert_eq!(
            selection_spec(&buffer),
            SelectionSpec::new(case.after_anchor, case.after_head),
            "{}: after 选区",
            case.case_id
        );
        assert_eq!(
            text_of(&buffer)?,
            case.after,
            "{}: after 文本",
            case.case_id
        );
        assert_eq!(buffer.revision().value(), 1, "{}: 单步代次", case.case_id);
        assert_eq!(buffer.history_node_count(), 1, "{}: 单步历史", case.case_id);

        let step = buffer
            .history_step(0)
            .ok_or_else(|| format!("{}: 缺少历史步骤", case.case_id))?;
        let expected_forward = effective_patches(&case.before, &case.patches);
        assert_eq!(
            step.get().forward_patches(),
            expected_forward.as_slice(),
            "{}: 历史正补丁只保留实际变化",
            case.case_id
        );
        assert_eq!(
            step.get().inverse_patches(),
            case.inverse.as_slice(),
            "{}: 历史逆补丁",
            case.case_id
        );

        let mut restored = StringBuffer::new(
            case.after.clone(),
            SelectionSpec::new(case.after_anchor, case.after_head),
            TextBudget::new(TEXT_BUDGET),
            LineIndexBudget::new(1024),
            HistoryBudget::new(256, 1 << 20),
        )?;
        let inverse_batch = CheckedBatch::new(
            restored.snapshot(),
            case.inverse.clone(),
            PatchBudget::new(PATCH_BUDGET, TEXT_BUDGET, TEXT_BUDGET),
        )?;
        let inverse_outcome = restored.apply(
            &inverse_batch,
            SelectionSpec::new(case.before_anchor, case.before_head),
            OperationSource::Command,
            1001,
            TextBudget::new(TEXT_BUDGET),
        )?;
        assert!(
            matches!(inverse_outcome, ApplyOutcome::Changed(_)),
            "{}: 逆补丁必须产生变化",
            case.case_id
        );
        assert_eq!(
            text_of(&restored)?,
            case.before,
            "{}: 逆补丁往返",
            case.case_id
        );
        assert_eq!(
            selection_spec(&restored),
            SelectionSpec::new(case.before_anchor, case.before_head),
            "{}: 逆补丁选区",
            case.case_id
        );
    }
    Ok(())
}

#[test]
fn line_index_goldens_match_handwritten_boundaries() -> Result<(), Box<dyn Error>> {
    let cases = parse_line_goldens()?;
    assert_eq!(cases.len(), 11, "行索引金样数量");
    for case in cases {
        let built = LineIndex::build(&case.text, LineIndexBudget::new(case.max_lines));
        if case.reject {
            assert!(
                matches!(built, Err(BufferError::LineCountExceeded { .. })),
                "{}: 必须按行数预算拒绝",
                case.case_id
            );
            continue;
        }
        let index = built?;
        assert_eq!(
            index.line_count(),
            case.line_count,
            "{}: 行数",
            case.case_id
        );
        for (line, expected) in case.ranges.iter().enumerate() {
            assert_eq!(
                index.line_range(line).as_ref(),
                Some(expected),
                "{}: 第 {line} 行内容范围",
                case.case_id
            );
        }
        for (offset, expected_line) in &case.offsets {
            assert_eq!(
                index.line_of_offset(*offset),
                Some(*expected_line),
                "{}: 偏移 {offset} 的行号",
                case.case_id
            );
        }
    }
    Ok(())
}

fn text_of(buffer: &StringBuffer) -> Result<String, Box<dyn Error>> {
    Ok(buffer.text(TextBudget::new(TEXT_BUDGET))?.into_owned())
}

fn selection_spec(buffer: &StringBuffer) -> SelectionSpec {
    SelectionSpec::new(
        buffer.selection().anchor().byte_offset(),
        buffer.selection().head().byte_offset(),
    )
}

fn effective_patches(before: &str, patches: &[(Range<usize>, String)]) -> PatchList {
    patches
        .iter()
        .filter(|(range, replacement)| before.get(range.clone()) != Some(replacement.as_str()))
        .cloned()
        .collect()
}

fn parse_forward_goldens() -> Result<Vec<ForwardCase>, Box<dyn Error>> {
    let mut lines = FORWARD_GOLDENS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let header = lines.next().ok_or_else(|| "缺少正逆补丁表头".to_string())?;
    if !header.starts_with("case_id\tbefore_hex\t") {
        return Err("正逆补丁表头不符合约定".to_string().into());
    }
    let mut cases = Vec::new();
    for line in lines {
        let fields = fields_of(line)?;
        if fields.len() != 10 {
            return Err(format!("{}: 字段数不是 10", line).into());
        }
        cases.push(ForwardCase {
            case_id: fields[0].to_string(),
            before: decode_utf8(fields[1])?,
            patches: parse_patches(fields[2])?,
            after: decode_utf8(fields[3])?,
            inverse: parse_patches(fields[4])?,
            before_anchor: fields[5].parse()?,
            before_head: fields[6].parse()?,
            after_anchor: fields[7].parse()?,
            after_head: fields[8].parse()?,
            changed: match fields[9] {
                "changed" => true,
                "unchanged" => false,
                other => return Err(format!("未知 expect 值: {other}").into()),
            },
        });
    }
    Ok(cases)
}

fn parse_line_goldens() -> Result<Vec<LineCase>, Box<dyn Error>> {
    let mut lines = LINE_GOLDENS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let header = lines.next().ok_or_else(|| "缺少行索引表头".to_string())?;
    if !header.starts_with("case_id\ttext_hex\t") {
        return Err("行索引表头不符合约定".to_string().into());
    }
    let mut cases = Vec::new();
    for line in lines {
        let fields = fields_of(line)?;
        if fields.len() != 7 {
            return Err(format!("{}: 字段数不是 7", line).into());
        }
        let reject = match fields[6] {
            "ok" => false,
            "line_count_exceeded" => true,
            other => return Err(format!("未知 expect 值: {other}").into()),
        };
        cases.push(LineCase {
            case_id: fields[0].to_string(),
            text: decode_utf8(fields[1])?,
            max_lines: fields[2].parse()?,
            line_count: if reject { 0 } else { fields[3].parse()? },
            ranges: if reject {
                Vec::new()
            } else {
                parse_ranges(fields[4])?
            },
            offsets: if reject {
                Vec::new()
            } else {
                parse_offsets(fields[5])?
            },
            reject,
        });
    }
    Ok(cases)
}

fn fields_of(line: &str) -> Result<Vec<&str>, Box<dyn Error>> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.is_empty() || fields[0].is_empty() {
        return Err("空字段行".to_string().into());
    }
    Ok(fields)
}

fn decode_utf8(hex: &str) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(decode_hex(hex)?)?)
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    if !hex.len().is_multiple_of(2) {
        return Err(format!("十六进制长度不是偶数: {hex}").into());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let (pairs, _remainder) = hex.as_bytes().as_chunks::<2>();
    for pair in pairs {
        let digits = std::str::from_utf8(pair.as_slice())?;
        bytes.push(u8::from_str_radix(digits, 16)?);
    }
    Ok(bytes)
}

fn parse_patches(field: &str) -> Result<PatchList, Box<dyn Error>> {
    if field == "-" {
        return Ok(Vec::new());
    }
    let mut patches = Vec::new();
    for item in field.split(',') {
        let mut parts = item.split(':');
        let start: usize = parts
            .next()
            .ok_or_else(|| "补丁缺少起点".to_string())?
            .parse()?;
        let end: usize = parts
            .next()
            .ok_or_else(|| "补丁缺少终点".to_string())?
            .parse()?;
        let replacement = parts.next().ok_or_else(|| "补丁缺少替换文本".to_string())?;
        if parts.next().is_some() {
            return Err(format!("补丁字段多余: {item}").into());
        }
        patches.push((start..end, decode_utf8(replacement)?));
    }
    Ok(patches)
}

fn parse_ranges(field: &str) -> Result<Vec<Range<usize>>, Box<dyn Error>> {
    if field == "-" {
        return Ok(Vec::new());
    }
    let mut ranges = Vec::new();
    for item in field.split(',') {
        let mut parts = item.split(':');
        let start: usize = parts
            .next()
            .ok_or_else(|| "范围缺少起点".to_string())?
            .parse()?;
        let end: usize = parts
            .next()
            .ok_or_else(|| "范围缺少终点".to_string())?
            .parse()?;
        if parts.next().is_some() {
            return Err(format!("范围字段多余: {item}").into());
        }
        ranges.push(start..end);
    }
    Ok(ranges)
}

fn parse_offsets(field: &str) -> Result<Vec<(usize, usize)>, Box<dyn Error>> {
    if field == "-" {
        return Ok(Vec::new());
    }
    let mut offsets = Vec::new();
    for item in field.split(',') {
        let mut parts = item.split(':');
        let offset: usize = parts
            .next()
            .ok_or_else(|| "偏移缺少字节位置".to_string())?
            .parse()?;
        let line: usize = parts
            .next()
            .ok_or_else(|| "偏移缺少行号".to_string())?
            .parse()?;
        if parts.next().is_some() {
            return Err(format!("偏移字段多余: {item}").into());
        }
        offsets.push((offset, line));
    }
    Ok(offsets)
}
