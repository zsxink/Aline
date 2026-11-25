//! V-04/V-06 主题包归档探针：量候选解包库（`zip` 8.6.0）与 JSON 解析对恶意/
//! 越界输入的实际行为，而不是文档承诺。
//!
//! 主题包规范要求“ZIP64 是否支持由 V-04 的解包库及计数溢出实验决定”，并要求
//! 在解压过程中持续执行预算。本探针回答五件事：
//!
//! 1. 目录预检能看到什么：条目名、是否目录、是否链接、声明大小、压缩方法、
//!    是否加密，以及库自带的路径封装（`enclosed_name`）是否拦下穿越路径。
//! 2. 中央目录声明数与库实际暴露数是否一致——库用 `IndexMap` 按原始文件名
//!    去重，重复条目会被静默合并，预检不能只信 `len()`。
//! 3. 声明大小是否可信：读取时用 `take(limit)` 限流，报告实际读到的字节数，
//!    证明“声明 1 GiB、实际几百字节”的包会不会被无界展开。
//! 4. 解压落盘能否在执行过程中持续计预算并中止。
//! 5. JSON 的字节/深度/成员/数组/字符串实际形状。
//!
//! 子命令：
//! - `list <file>...`：只读目录，不读任何条目内容。
//! - `read <limit> <file>...`：逐条目读取，每条最多读 `limit` 字节。
//! - `extract <total-limit> <dest> <file>...`：解压到 `dest`，逐条持续计预算，
//!   触顶即中止并报告已写字节。
//! - `json <file>...`：用 `serde_json` 解析并报告形状。
//!
//! `extract` 只写调用方给定的临时目录；探针不选择落点。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

use zip::ZipArchive;

/// EOCD（End of Central Directory）签名；用于读回“中央目录声明了多少条”。
///
/// `zip` 8.6.0 不公开该计数，但重复条目会被它静默合并，预检必须能看出
/// “声明 3 条、实际 2 条”这类包，所以这里自己读 22 字节定长记录。
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const EOCD_MIN_BYTES: u64 = 22;

fn declared_entry_count(path: &str) -> Option<u16> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length < EOCD_MIN_BYTES {
        return None;
    }
    // 注释最长 65535 字节，从尾部向前找签名。
    let window = length.min(EOCD_MIN_BYTES + 65_535);
    let start = length - window;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buffer = vec![0_u8; window as usize];
    file.read_exact(&mut buffer).ok()?;
    let signature = EOCD_SIGNATURE.to_le_bytes();
    let offset = buffer.windows(4).rposition(|bytes| bytes == signature)?;
    let count_at = offset.checked_add(10)?;
    let bytes = buffer.get(count_at..count_at + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn list_one(path: &str) -> String {
    let started = Instant::now();
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let mut entries = Vec::new();
    let mut declared_total: u64 = 0;
    let mut compressed_total: u64 = 0;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = match archive.by_index_raw(index) {
            Ok(entry) => entry,
            Err(error) => {
                entries.push(serde_json::json!({ "index": index, "error": error.to_string() }));
                continue;
            }
        };
        declared_total = declared_total.saturating_add(entry.size());
        compressed_total = compressed_total.saturating_add(entry.compressed_size());
        names.push(entry.name().to_string());
        entries.push(serde_json::json!({
            "name": entry.name(),
            "is_dir": entry.is_dir(),
            "is_symlink": entry.is_symlink(),
            "unix_mode": entry.unix_mode(),
            "declared_size": entry.size(),
            "compressed_size": entry.compressed_size(),
            "compression": format!("{:?}", entry.compression()),
            "encrypted": entry.encrypted(),
            // `enclosed_name` 是库自带的穿越防护：返回 None 表示它认为路径不安全。
            "enclosed_name": entry.enclosed_name().map(|p| p.to_string_lossy().into_owned()),
        }));
    }
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    let overlapping = archive.has_overlapping_files().unwrap_or(false);
    serde_json::json!({
        "ok": true,
        "entries": entries,
        "entry_count": archive.len(),
        "declared_entry_count": declared_entry_count(path),
        "unique_name_count": unique.len(),
        "has_overlapping_files": overlapping,
        "declared_total": declared_total,
        "compressed_total": compressed_total,
        // 目录预检的耗时：只读中央目录、不读任何条目内容。条目数预算就是
        // 拿这个数字量的，所以必须由探针自己报告，不能由调用方用墙钟外推。
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
    .to_string()
}

/// 逐条目限流读取。`take(limit)` 保证即使声明大小是天文数字也不会真读那么多，
/// 从而把“预算能否在读取过程中执行”变成可观测事实。
fn read_one(path: &str, limit: u64) -> String {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let count = archive.len();
    let mut rows = Vec::new();
    for index in 0..count {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                rows.push(serde_json::json!({ "index": index, "error": error.to_string() }));
                continue;
            }
        };
        let name = entry.name().to_string();
        let declared = entry.size();
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut read_total: u64 = 0;
        let mut failure = None;
        loop {
            let remaining = limit.saturating_sub(read_total);
            if remaining == 0 {
                break;
            }
            let want = remaining.min(buffer.len() as u64) as usize;
            match entry.read(&mut buffer[..want]) {
                Ok(0) => break,
                Ok(n) => read_total += n as u64,
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        rows.push(serde_json::json!({
            "name": name,
            "declared_size": declared,
            "read_bytes": read_total,
            "hit_limit": read_total >= limit,
            "error": failure,
        }));
    }
    serde_json::json!({ "ok": true, "limit": limit, "entries": rows }).to_string()
}

/// 受限解压：每条按 64 KiB 缓冲写盘，累计写入达到 `total_limit` 立即中止。
///
/// 这是“预算在解压过程中持续执行”的最小可观测实现：报告中止发生在第几条、
/// 已写多少字节，而不是解压完再追认总大小。
fn extract_one(path: &str, dest: &str, total_limit: u64) -> String {
    let started = Instant::now();
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let dest = Path::new(dest);
    let mut written_total: u64 = 0;
    let mut entries_written = 0_usize;
    let mut aborted_at: Option<usize> = None;
    let mut failure = None;
    let count = archive.len();
    for index in 0..count {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        };
        // 探针自己也不信任条目名：`enclosed_name` 为 None 即中止，绝不拼宿主路径。
        // 落盘文件按序号命名，只用于量写入量，不保留包内目录结构。
        if entry.enclosed_name().is_none() {
            failure = Some(format!("unsafe path: {}", entry.name()));
            break;
        }
        let target = dest.join(format!("entry-{index:05}.bin"));
        let mut sink = match File::create(&target) {
            Ok(sink) => sink,
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        };
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut entry_written: u64 = 0;
        let mut hit = false;
        loop {
            match entry.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = total_limit.saturating_sub(written_total);
                    if remaining == 0 {
                        hit = true;
                        break;
                    }
                    let take = (n as u64).min(remaining) as usize;
                    if let Err(error) = sink.write_all(&buffer[..take]) {
                        failure = Some(error.to_string());
                        break;
                    }
                    written_total += take as u64;
                    entry_written += take as u64;
                    if take < n {
                        hit = true;
                        break;
                    }
                }
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        if entry_written > 0 || entry.size() == 0 {
            entries_written += 1;
        }
        if hit {
            aborted_at = Some(index);
            break;
        }
        if failure.is_some() {
            break;
        }
    }
    serde_json::json!({
        "ok": failure.is_none(),
        "error": failure,
        "entry_count": count,
        "entries_written": entries_written,
        "written_bytes": written_total,
        "total_limit": total_limit,
        "aborted_at": aborted_at,
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
    .to_string()
}

/// 形状统计：深度、对象成员数、数组项数、字符串标量与字节上限。
///
/// 深度从顶层对象算 1；标量字符串按 Unicode 标量数计长度。
fn json_one(path: &str) -> String {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let started = Instant::now();
    let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    match parsed {
        Ok(value) => {
            let shape = shape_of(&value);
            serde_json::json!({
                "ok": true,
                "source_bytes": bytes.len(),
                "elapsed_ms": elapsed_ms,
                "depth": shape.depth,
                "max_members": shape.max_members,
                "max_array_items": shape.max_array_items,
                "max_string_scalars": shape.max_string_scalars,
                "max_string_bytes": shape.max_string_bytes,
            })
            .to_string()
        }
        Err(error) => serde_json::json!({
            "ok": false,
            "source_bytes": bytes.len(),
            "elapsed_ms": elapsed_ms,
            "error": error.to_string(),
        })
        .to_string(),
    }
}

#[derive(Default)]
struct Shape {
    depth: usize,
    max_members: usize,
    max_array_items: usize,
    max_string_scalars: usize,
    max_string_bytes: usize,
}

fn shape_of(value: &serde_json::Value) -> Shape {
    match value {
        serde_json::Value::Object(map) => {
            let mut shape = Shape {
                depth: 1,
                max_members: map.len(),
                ..Shape::default()
            };
            for child in map.values() {
                let inner = shape_of(child);
                shape.depth = shape.depth.max(inner.depth + 1);
                shape.max_members = shape.max_members.max(inner.max_members);
                shape.max_array_items = shape.max_array_items.max(inner.max_array_items);
                shape.max_string_scalars = shape.max_string_scalars.max(inner.max_string_scalars);
                shape.max_string_bytes = shape.max_string_bytes.max(inner.max_string_bytes);
            }
            shape
        }
        serde_json::Value::Array(items) => {
            let mut shape = Shape {
                depth: 1,
                max_array_items: items.len(),
                ..Shape::default()
            };
            for child in items {
                let inner = shape_of(child);
                shape.depth = shape.depth.max(inner.depth + 1);
                shape.max_members = shape.max_members.max(inner.max_members);
                shape.max_array_items = shape.max_array_items.max(inner.max_array_items);
                shape.max_string_scalars = shape.max_string_scalars.max(inner.max_string_scalars);
                shape.max_string_bytes = shape.max_string_bytes.max(inner.max_string_bytes);
            }
            shape
        }
        serde_json::Value::String(text) => Shape {
            depth: 1,
            max_string_scalars: text.chars().count(),
            max_string_bytes: text.len(),
            ..Shape::default()
        },
        _ => Shape {
            depth: 1,
            ..Shape::default()
        },
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    let rest: Vec<String> = arguments.collect();
    match command.as_str() {
        "list" => {
            for file in &rest {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": file,
                        "result": serde_json::from_str::<serde_json::Value>(
                            &list_one(file)).unwrap_or(serde_json::Value::Null),
                    })
                );
            }
        }
        "read" => {
            let (limit, files) = match rest.split_first() {
                Some((limit, files)) => (limit.parse::<u64>().unwrap_or(0), files),
                None => (0, &[][..]),
            };
            for file in files {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": file,
                        "result": serde_json::from_str::<serde_json::Value>(
                            &read_one(file, limit)).unwrap_or(serde_json::Value::Null),
                    })
                );
            }
        }
        "extract" => {
            let (limit, dest, files) = match rest.split_first() {
                Some((limit, rest)) => match rest.split_first() {
                    Some((dest, files)) => {
                        (limit.parse::<u64>().unwrap_or(0), dest.as_str(), files)
                    }
                    None => (0, "", &[][..]),
                },
                None => (0, "", &[][..]),
            };
            for file in files {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": file,
                        "result": serde_json::from_str::<serde_json::Value>(
                            &extract_one(file, dest, limit)).unwrap_or(serde_json::Value::Null),
                    })
                );
            }
        }
        "json" => {
            for file in &rest {
                println!(
                    "{}",
                    serde_json::json!({
                        "file": file,
                        "result": serde_json::from_str::<serde_json::Value>(
                            &json_one(file)).unwrap_or(serde_json::Value::Null),
                    })
                );
            }
        }
        other => {
            println!(
                "{}",
                serde_json::json!({ "error": format!("unknown command: {other}") })
            )
        }
    }
}
