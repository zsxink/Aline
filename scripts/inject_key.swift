// 物理键盘按键注入器：把合成键盘事件投递到系统 HID 事件流，由当前前台应用
// 的输入上下文处理（与真实键盘走同一条平台通道，不是应用内部模拟）。
//
// 用法：inject_key <virtualKey> [<virtualKey> ...]
//   每个参数是十进制 macOS 虚拟键码（如 n=45、i=34、数字 2=19、空格=49）。
//   依次投递 keyDown/keyUp，键间留出足够间隔让输入法处理候选。
//
// 造这个工具的原因：orca computer 的 press-key 按 pid 定向投递，窗口失活时
// 仍会把按键送进目标进程，输入法看不到候选窗而把 marked 原文提交；HID 事件
// 只到前台应用，调用方必须先确认被测实例在前台。
import CoreGraphics
import Foundation

let keyCodes = CommandLine.arguments.dropFirst().compactMap { UInt16($0) }
guard !keyCodes.isEmpty else {
    FileHandle.standardError.write("usage: inject_key virtualKey [...]\n".data(using: .utf8)!)
    exit(2)
}
let source = CGEventSource(stateID: .hidSystemState)
for keyCode in keyCodes {
    guard let down = CGEvent(keyboardEventSource: source,
                             virtualKey: CGKeyCode(keyCode),
                             keyDown: true),
          let up = CGEvent(keyboardEventSource: source,
                           virtualKey: CGKeyCode(keyCode),
                           keyDown: false) else {
        FileHandle.standardError.write("failed to create event for \(keyCode)\n".data(using: .utf8)!)
        exit(1)
    }
    down.flags = []
    up.flags = []
    down.post(tap: .cghidEventTap)
    usleep(25_000)
    up.post(tap: .cghidEventTap)
    usleep(40_000)
    print("posted key \(keyCode)")
}
