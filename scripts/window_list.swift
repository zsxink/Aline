// 只读窗口枚举探针：按 CGWindowList 判断目标进程是否真的拥有窗口。
//
// 只用系统框架 CoreGraphics/Foundation，不写盘、不改被观察进程。
// 参数 1 为期望的 owner 名（大小写不敏感的子串匹配）。
// 退出码恒为 0；结论写在 stdout，由调用方读取。
//
// 2026-11-12 从 logs 证据目录迁入 scripts/，供 run_window_failure_probe.py
// 在复跑时编译；可执行代码与迁入前一致，仅新增本文件头注释，sha256 因此变化。

import CoreGraphics
import Foundation

let needle = CommandLine.arguments.count > 1
    ? CommandLine.arguments[1].lowercased()
    : "aline"

func windows(_ option: CGWindowListOption) -> [[String: Any]] {
    CGWindowListCopyWindowInfo(option, kCGNullWindowID) as? [[String: Any]] ?? []
}

func describe(_ window: [String: Any]) -> [String: Any] {
    [
        "owner": window[kCGWindowOwnerName as String] as? String ?? "",
        "pid": window[kCGWindowOwnerPID as String] ?? 0,
        "layer": window[kCGWindowLayer as String] ?? 0,
        "onscreen": window[kCGWindowIsOnscreen as String] ?? false,
        "bounds": window[kCGWindowBounds as String] ?? [:],
    ]
}

let onscreen = windows([.optionOnScreenOnly, .excludeDesktopElements])
    .filter { (($0[kCGWindowOwnerName as String] as? String ?? "").lowercased()).contains(needle) }
    .map(describe)
let all = windows([.optionAll])
    .filter { (($0[kCGWindowOwnerName as String] as? String ?? "").lowercased()).contains(needle) }
    .map(describe)

let record: [String: Any] = [
    "needle": needle,
    "onscreen_count": onscreen.count,
    "all_count": all.count,
    "onscreen": onscreen,
    "all": all,
]
let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
