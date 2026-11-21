// 物理鼠标点击注入器：把合成鼠标按下/抬起投递到系统 HID 事件流，命中屏幕
// 坐标处的最前窗口（与真实鼠标同一条平台通道，不是应用内部模拟）。
//
// 用法：inject_click <screenX> <screenY>
//   坐标为屏幕坐标（左上原点，与 list_windows/inject_scroll_wheel 一致）。
//   先投递 mouseMoved 让指针真实驻留，再 leftMouseDown/leftMouseUp。
//
// 与 inject_key.swift 同一动机：事件按屏幕坐标命中前台窗口，调用方必须
// 先确认被测实例在前台且目标点落在其窗口内；不依赖 Orca。
import CoreGraphics
import Foundation

let arguments = CommandLine.arguments.dropFirst()
guard arguments.count == 2, let x = Double(arguments.first!),
      let y = Double(arguments[arguments.index(after: arguments.startIndex)]) else {
    FileHandle.standardError.write("usage: inject_click screenX screenY\n".data(using: .utf8)!)
    exit(2)
}
let point = CGPoint(x: x, y: y)
let source = CGEventSource(stateID: .hidSystemState)
guard let move = CGEvent(mouseEventSource: source, mouseType: .mouseMoved,
                         mouseCursorPosition: point, mouseButton: .left),
      let down = CGEvent(mouseEventSource: source, mouseType: .leftMouseDown,
                         mouseCursorPosition: point, mouseButton: .left),
      let up = CGEvent(mouseEventSource: source, mouseType: .leftMouseUp,
                       mouseCursorPosition: point, mouseButton: .left) else {
    FileHandle.standardError.write("failed to create mouse events\n".data(using: .utf8)!)
    exit(1)
}
move.post(tap: .cghidEventTap)
usleep(60_000)
down.post(tap: .cghidEventTap)
usleep(50_000)
up.post(tap: .cghidEventTap)
usleep(40_000)
print("posted click at \(x),\(y)")