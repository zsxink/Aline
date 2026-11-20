// 前台窗口几何快照：只输出被测应用窗口与输入法候选窗的几何信息，不读取
// 窗口名称或任何窗口内容，避免把候选正文或其他应用信息写进证据。
//
// 用法：list_windows <目标 pid>
//   输出 JSON 数组，条目含 owner/pid/layer/bounds/alpha/onscreen。
//   收录条件：目标 pid 自己的窗口、层级 >= 1e9 的系统/输入法浮层，或
//   owner 名含“微信输入法”/wetype/inputmethod 的窗口。bounds 为屏幕坐标
//   （左上原点）。
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 2, let target = Int(args[1]) else {
    FileHandle.standardError.write("usage: list_windows pid\n".data(using: .utf8)!)
    exit(2)
}
var output: [[String: Any]] = []
if let list = CGWindowListCopyWindowInfo(
    [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
    for info in list {
        let pid = info[kCGWindowOwnerPID as String] as? Int ?? -1
        let owner = info[kCGWindowOwnerName as String] as? String ?? ""
        let layer = info[kCGWindowLayer as String] as? Int ?? 0
        let lowered = owner.lowercased()
        let candidate = layer >= 1_000_000_000
            || lowered.contains("wetype") || lowered.contains("inputmethod")
            || owner.contains("微信输入法")
        guard pid == target || (candidate && lowered != "window server") else { continue }
        var bounds: [String: Any] = [:]
        if let raw = info[kCGWindowBounds as String] as? [String: Any] {
            bounds = ["x": raw["X"] ?? 0, "y": raw["Y"] ?? 0,
                      "w": raw["Width"] ?? 0, "h": raw["Height"] ?? 0]
        }
        output.append([
            "owner": owner,
            "pid": pid,
            "layer": layer,
            "bounds": bounds,
            "alpha": info[kCGWindowAlpha as String] ?? 0,
            "onscreen": info[kCGWindowIsOnscreen as String] ?? false,
        ])
    }
}
guard let data = try? JSONSerialization.data(withJSONObject: output, options: [.sortedKeys]),
      let text = String(data: data, encoding: .utf8) else {
    FileHandle.standardError.write("failed to encode windows\n".data(using: .utf8)!)
    exit(1)
}
print(text)
