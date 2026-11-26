// 用法：resize_window <pid> <width> <height>
// 经辅助功能 API 把目标进程首个窗口的尺寸设为 (width,height) 逻辑点。
// 只设置尺寸，不读写窗口内容；失败返回非 0 与错误文本。
import ApplicationServices
import Foundation

let args = CommandLine.arguments
guard args.count == 4,
      let pid = Int32(args[1]),
      let width = Double(args[2]),
      let height = Double(args[3]) else {
    FileHandle.standardError.write("usage: resize_window <pid> <width> <height>\n".data(using: .utf8)!)
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
var size = CGSize(width: width, height: height)
guard let value = AXValueCreate(.cgSize, &size) else { exit(4) }
let setStatus = AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value)
print("set_size status=\(setStatus.rawValue) target=(\(width),\(height))")
exit(setStatus == .success ? 0 : 5)
