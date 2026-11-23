// 读取当前显示器身份、point 尺寸与 backing scale，只取几何，不读屏幕内容。
// 输出 JSON：{"screens":[{frame:{x,y,w,h},backing_scale,is_builtin,name}...]}
import AppKit
import CoreGraphics
import Foundation

func displayID(_ screen: NSScreen) -> CGDirectDisplayID? {
    guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else {
        return nil
    }
    return CGDirectDisplayID(number.uint32Value)
}

let payload: [String: Any] = [
    "schema": "aline-display-scales/v1",
    "screens": NSScreen.screens.enumerated().map { (index, screen) in
        let id = displayID(screen)
        return [
            "index": index,
            "display_id": id.map { NSNumber(value: $0) } ?? NSNull(),
            "frame": [
                "x": Double(screen.frame.origin.x),
                "y": Double(screen.frame.origin.y),
                "w": Double(screen.frame.width),
                "h": Double(screen.frame.height),
            ],
            "backing_scale": Double(screen.backingScaleFactor),
            "is_builtin": id.map { CGDisplayIsBuiltin($0) != 0 } ?? false,
            "name": screen.localizedName,
        ] as [String: Any]
    },
]
let data = try! JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data("\n".utf8))
