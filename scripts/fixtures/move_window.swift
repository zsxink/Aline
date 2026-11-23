// 用法：move_window <pid> <x> <y>
// 经辅助功能 API 把目标进程首个窗口的左上角移到屏幕坐标 (x,y)。
// 只设置位置，不读写窗口内容；失败返回非 0 与错误文本。
import ApplicationServices
import Foundation

let args = CommandLine.arguments
guard args.count == 4,
      let pid = Int32(args[1]),
      let x = Double(args[2]),
      let y = Double(args[3]) else {
    FileHandle.standardError.write("usage: move_window <pid> <x> <y>\n".data(using: .utf8)!)
    exit(2)
}
let app = AXUIElementCreateApplication(pid)
var windows: CFTypeRef?
let status = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &windows)
guard status == .success,
      let list = windows as? [AXUIElement],
      let window = list.first else {
    FileHandle.standardError.write("AX windows unavailable: \(status.rawValue)\n".data(using: .utf8)!)
    exit(3)
}
var point = CGPoint(x: x, y: y)
guard let value = AXValueCreate(.cgPoint, &point) else { exit(4) }
let setStatus = AXUIElementSetAttributeValue(window, kAXPositionAttribute as CFString, value)
print("set_position status=\(setStatus.rawValue) target=(\(x),\(y))")
exit(setStatus == .success ? 0 : 5)
