//! 组 6.6 探针：二分出扫描步数预算阈值（最小可通过值），并记录超限诊断。
use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{ParseBudget, ParseConfig, parse};

fn main() {
    let mut source = String::new();
    for _ in 0..40 {
        source.push_str("<b>x</b>");
    }
    source.push('\n');
    let snapshot = match TextSnapshot::new(source.clone(), TextBudget::new(1 << 20)) {
        Ok(s) => s,
        Err(e) => { println!("ERROR {e:?}"); return; }
    };
    let mut low = 0usize;
    let mut high = 1024usize;
    while low < high {
        let middle = low.saturating_add(high.saturating_sub(low) / 2);
        let ok = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0().with_max_scan_steps(middle))
            .map(|p| p.diagnostics().is_empty())
            .unwrap_or(false);
        if ok { high = middle; } else { low = middle.saturating_add(1); }
    }
    println!("source_bytes {}", source.len());
    println!("scan_step_threshold {}", low);
    let at = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0().with_max_scan_steps(low))
        .map(|p| p.diagnostics().len());
    let below = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0().with_max_scan_steps(low.saturating_sub(1)))
        .map(|p| p.diagnostics().len());
    println!("diagnostics_at_threshold {:?}", at);
    println!("diagnostics_below {:?}", below);
}
