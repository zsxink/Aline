// 像素级滚轮事件注入器：把合成滚轮事件投递到系统事件流，走的是应用真实
// NSEvent 输入通道（不是应用内部模拟）。
//
// 用法：inject_scroll_wheel <screenX> <screenY> <deltaX> <deltaY>
//   screenX/screenY 为屏幕点坐标（不是像素、不含标题栏偏移）；事件先移动到
//   该点，再以像素单位投递 wheelCount=2 的滚轮事件：wheel1=纵向、wheel2=横向。
//   正 deltaY 向上、负 deltaY 向下；正 deltaX 向右、负 deltaX 向左（与
//   NSEvent.scrollingDeltaX/Y 的方向一致）。
//
// 造这个工具的原因：orca computer 的水平滚动在 wheel1 槽位留下未初始化值，
// 会夹带真实纵向滚动；这里把两个轴都写成显式值，保证是纯水平输入。
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 5,
      let x = Double(args[1]), let y = Double(args[2]),
      let dx = Double(args[3]), let dy = Double(args[4]) else {
    FileHandle.standardError.write("usage: inject_scroll_wheel x y dx dy\n".data(using: .utf8)!)
    exit(2)
}
CGWarpMouseCursorPosition(CGPoint(x: x, y: y))
usleep(120_000)
guard let event = CGEvent(scrollWheelEvent2Source: nil,
                          units: .pixel,
                          wheelCount: 2,
                          wheel1: Int32(dy),
                          wheel2: Int32(dx),
                          wheel3: 0) else {
    FileHandle.standardError.write("failed to create event\n".data(using: .utf8)!)
    exit(1)
}
event.location = CGPoint(x: x, y: y)
event.post(tap: .cghidEventTap)
print("posted dx=\(dx) dy=\(dy) at \(x),\(y)")
