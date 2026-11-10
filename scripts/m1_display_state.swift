// 采样环境检查工具：只读取会话/显示器/窗口状态，不接触被测量进程。
// check 输出 aline-m1-display-state/v1；activate 输出 aline-m1-window-activation/v1。
import AppKit
import CoreGraphics
import Foundation

let stateSchema = "aline-m1-display-state/v1"
let activationSchema = "aline-m1-window-activation/v1"

func emit(_ payload: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
}

func boolValue(_ value: Any?) -> Bool {
    if let value = value as? Bool { return value }
    if let value = value as? NSNumber { return value.boolValue }
    return false
}

func ownerPid(_ window: [String: Any]) -> Int? {
    let key = kCGWindowOwnerPID as String
    if let value = window[key] as? Int { return value }
    if let value = window[key] as? Int32 { return Int(value) }
    if let value = window[key] as? NSNumber { return value.intValue }
    return nil
}

func onscreenWindows(pid: Int) -> (all: Int, layer0: Int, nonzero: Bool) {
    let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
    let info = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] ?? []
    var all = 0
    var layer0 = 0
    var nonzero = false
    for window in info where ownerPid(window) == pid {
        all += 1
        if (window[kCGWindowLayer as String] as? Int) == 0 { layer0 += 1 }
        if let bounds = window[kCGWindowBounds as String] as? [String: Any],
           let width = bounds["Width"] as? Double,
           let height = bounds["Height"] as? Double,
           width >= 1, height >= 1 {
            nonzero = true
        }
    }
    return (all, layer0, nonzero)
}

func displayState() -> [String: Any] {
    let session = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
    let display = CGMainDisplayID()
    return [
        "schema": stateSchema,
        "screen_locked": boolValue(session["CGSSessionScreenIsLocked"]),
        "on_console": boolValue(session["kCGSSessionOnConsoleKey"]),
        "login_done": boolValue(session["kCGSessionLoginDoneKey"]),
        "display_asleep": CGDisplayIsAsleep(display) != 0,
        "display_active": CGDisplayIsActive(display) != 0,
        "display_online": CGDisplayIsOnline(display) != 0,
        "frontmost_pid": Int(NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0),
    ]
}

let arguments = CommandLine.arguments
let command = arguments.count > 1 ? arguments[1] : "check"

switch command {
case "check":
    emit(displayState())
case "activate":
    guard arguments.count == 4,
          let pid = Int32(arguments[2]),
          let timeout = Int(arguments[3]) else {
        FileHandle.standardError.write(Data("usage: m1_display_state activate <pid> <timeout_ms>\n".utf8))
        exit(2)
    }
    let target = Int(pid)
    guard let application = NSRunningApplication(processIdentifier: pid) else {
        emit(["schema": activationSchema, "pid": target, "found": false,
              "frontmost": false, "onscreen_windows": 0, "layer0_windows": 0,
              "has_nonzero_bounds": false])
        exit(2)
    }
    application.activate(options: [.activateAllWindows])
    let deadline = Date().addingTimeInterval(Double(max(timeout, 0)) / 1000.0)
    var frontmost = false
    while true {
        if NSWorkspace.shared.frontmostApplication?.processIdentifier == pid {
            frontmost = true
            break
        }
        if Date() >= deadline { break }
        Thread.sleep(forTimeInterval: 0.02)
    }
    let windows = onscreenWindows(pid: target)
    let ready = frontmost && windows.layer0 >= 1 && windows.nonzero
    emit(["schema": activationSchema, "pid": target, "found": true,
          "frontmost": frontmost, "onscreen_windows": windows.all,
          "layer0_windows": windows.layer0, "has_nonzero_bounds": windows.nonzero,
          "activation_policy": Int(application.activationPolicy.rawValue),
          "hidden": application.isHidden])
    exit(ready ? 0 : 3)
default:
    FileHandle.standardError.write(Data("unknown command\n".utf8))
    exit(2)
}
