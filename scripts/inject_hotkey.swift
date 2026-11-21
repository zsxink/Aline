// 组合键物理按键注入器：把带修饰键的合成键盘事件投递到系统 HID 事件流，
// 由当前前台应用处理（与真实键盘同一条平台通道，不是应用内部模拟）。
//
// 用法：inject_hotkey <cmd|shift|alt|ctrl>[+...] <virtualKey>
//   例：inject_hotkey cmd 8   → Cmd+O（o=31、c=8、w=13、s=1、f=3、z=6）
//       inject_hotkey cmd+shift 6 → Cmd+Shift+Z
//   修饰键按下后投递主键 keyDown/keyUp，再逆序松开修饰键。
//
// 与 inject_key.swift 同一动机：事件只到前台应用，调用方必须先确认被测
// 实例在前台；不依赖 Orca 或 System Events 的按键许可。
import CoreGraphics
import Foundation

let arguments = CommandLine.arguments.dropFirst()
guard arguments.count == 2, let keyCode = UInt16(arguments[arguments.index(after: arguments.startIndex)]) else {
    FileHandle.standardError.write("usage: inject_hotkey cmd[+shift+alt+ctrl] virtualKey\n".data(using: .utf8)!)
    exit(2)
}
var flags: CGEventFlags = []
for modifier in arguments.first!.lowercased().split(separator: "+") {
    switch modifier {
    case "cmd": flags.insert(.maskCommand)
    case "shift": flags.insert(.maskShift)
    case "alt": flags.insert(.maskAlternate)
    case "ctrl": flags.insert(.maskControl)
    default:
        FileHandle.standardError.write("unknown modifier \(modifier)\n".data(using: .utf8)!)
        exit(2)
    }
}
let source = CGEventSource(stateID: .hidSystemState)
guard let down = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(keyCode), keyDown: true),
      let up = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(keyCode), keyDown: false) else {
    FileHandle.standardError.write("failed to create event for \(keyCode)\n".data(using: .utf8)!)
    exit(1)
}
down.flags = flags
up.flags = flags
down.post(tap: .cghidEventTap)
usleep(30_000)
up.post(tap: .cghidEventTap)
usleep(50_000)
print("posted hotkey \(arguments.first!) + \(keyCode)")