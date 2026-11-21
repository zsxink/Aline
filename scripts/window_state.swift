// 目标进程窗口快照：输出被测应用的窗口几何、标题与标准窗口按钮位置，
// 供真实窗口取证在不依赖 Orca 的情况下换算画面坐标与命中关闭按钮。
//
// 用法：window_state <目标 pid>
//   输出 JSON：{"ok":bool,"window":{x,y,width,height,title,
//     close_button:{x,y}|null,zoom_button:{x,y}|null},"windows":[...]}
//   window 取该 pid 层级 0、上屏、面积最大的窗口（主文档窗口）；
//   按钮位置经 AXUIElement（kAXCloseButtonAttribute 等）读取，屏幕坐标
//   左上原点。只读取被测进程自己的窗口，不读取其他应用内容。
//
// 依赖：调用方宿主需已获 macOS 辅助功能（读 AX）授权；窗口几何本身
// 不需要屏幕录制权限，标题在无权限时可能为空。
import ApplicationServices
import CoreGraphics
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 2, let target = Int32(arguments[1]) else {
    FileHandle.standardError.write("usage: window_state pid\n".data(using: .utf8)!)
    exit(2)
}

struct WindowInfo {
    var bounds: [String: Double]
    var title: String
    var number: Int
}

var candidates: [WindowInfo] = []
if let list = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
    for info in list {
        guard let pid = info[kCGWindowOwnerPID as String] as? Int32, pid == target,
              let layer = info[kCGWindowLayer as String] as? Int, layer == 0,
              let raw = info[kCGWindowBounds as String] as? [String: Any] else { continue }
        let bounds = [
            "x": (raw["X"] as? Double) ?? 0,
            "y": (raw["Y"] as? Double) ?? 0,
            "width": (raw["Width"] as? Double) ?? 0,
            "height": (raw["Height"] as? Double) ?? 0,
        ]
        candidates.append(WindowInfo(
            bounds: bounds,
            title: (info[kCGWindowName as String] as? String) ?? "",
            number: (info[kCGWindowNumber as String] as? Int) ?? -1))
    }
}
candidates.sort { ($0.bounds["width"]! * $0.bounds["height"]!) > ($1.bounds["width"]! * $1.bounds["height"]!) }

func axButton(_ window: AXUIElement, _ attribute: String) -> [String: Double]? {
    var button: CFTypeRef?
    guard AXUIElementCopyAttributeValue(window, attribute as CFString, &button) == .success,
          let button, CFGetTypeID(button) == AXUIElementGetTypeID() else { return nil }
    let element = button as! AXUIElement
    var positionValue: CFTypeRef?
    var sizeValue: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, kAXPositionAttribute as CFString, &positionValue) == .success,
          AXUIElementCopyAttributeValue(element, kAXSizeAttribute as CFString, &sizeValue) == .success,
          let positionValue, let sizeValue else { return nil }
    var position = CGPoint.zero
    var size = CGSize.zero
    guard AXValueGetValue(positionValue as! AXValue, .cgPoint, &position),
          AXValueGetValue(sizeValue as! AXValue, .cgSize, &size) else { return nil }
    return ["x": Double(position.x + size.width / 2),
            "y": Double(position.y + size.height / 2)]
}

var output: [String: Any] = ["ok": false, "windows": candidates.map { $0.bounds }]
if let main = candidates.first {
    var window: [String: Any] = [
        "x": main.bounds["x"]!, "y": main.bounds["y"]!,
        "width": main.bounds["width"]!, "height": main.bounds["height"]!,
        "title": main.title, "number": main.number,
    ]
    // AX 窗口按标题/几何匹配 CGWindowList 的主窗口；标题缺失时按位置尺寸匹配。
    let app = AXUIElementCreateApplication(target)
    var windowsValue: CFTypeRef?
    if AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &windowsValue) == .success,
       let windows = windowsValue as? [AXUIElement] {
        for element in windows {
            var positionValue: CFTypeRef?
            var sizeValue: CFTypeRef?
            var position = CGPoint.zero
            var size = CGSize.zero
            if AXUIElementCopyAttributeValue(element, kAXPositionAttribute as CFString, &positionValue) == .success,
               AXUIElementCopyAttributeValue(element, kAXSizeAttribute as CFString, &sizeValue) == .success,
               let positionValue, let sizeValue,
               AXValueGetValue(positionValue as! AXValue, .cgPoint, &position),
               AXValueGetValue(sizeValue as! AXValue, .cgSize, &size) {
                let sameOrigin = abs(Double(position.x) - main.bounds["x"]!) < 2
                    && abs(Double(position.y) - main.bounds["y"]!) < 2
                let sameSize = abs(Double(size.width) - main.bounds["width"]!) < 2
                    && abs(Double(size.height) - main.bounds["height"]!) < 2
                if sameOrigin && sameSize {
                    if let close = axButton(element, kAXCloseButtonAttribute as String) {
                        window["close_button"] = close
                    }
                    if let zoom = axButton(element, kAXZoomButtonAttribute as String) {
                        window["zoom_button"] = zoom
                    }
                    var titleValue: CFTypeRef?
                    if AXUIElementCopyAttributeValue(element, kAXTitleAttribute as CFString, &titleValue) == .success,
                       let title = titleValue as? String, !title.isEmpty {
                        window["title"] = title
                    }
                    break
                }
            }
        }
    }
    output["window"] = window
    output["ok"] = true
}
let data = try! JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
print("")