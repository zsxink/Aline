// 同进程启动判别信号探针：实际创建一个 NSWindow，再读取四个候选信号。
//
// 只读系统状态、不写盘、不改被观察进程；结论写在 stdout 的最后一行 JSON。
// 2026-11-12 从 logs 证据目录迁入 scripts/，供 run_signal_probe.py 复跑时编译；
// 可执行代码与迁入前一致，仅新增本文件头注释，sha256 因此变化。

import AppKit
import CoreGraphics
import Foundation

func displayCount() -> Int {
    var count: UInt32 = 0
    if CGGetActiveDisplayList(0, nil, &count) != .success { return -1 }
    return Int(count)
}

func emit(_ payload: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)

let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 320, height: 200),
    styleMask: [.titled, .closable],
    backing: .buffered,
    defer: false
)
window.title = "signal-probe"
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)

let deadline = Date().addingTimeInterval(1.5)
while Date() < deadline {
    RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
}

emit([
    "cg_active_display_count": displayCount(),
    "nsscreen_count": NSScreen.screens.count,
    "window_number": window.windowNumber,
    "occlusion_visible": window.occlusionState.contains(.visible),
])
