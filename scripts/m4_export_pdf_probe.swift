// V-02 PDF 产物回读：用 PDFKit 读取真实页数、MediaBox、文本与墨迹包围盒。
//
// 只用系统 PDFKit，不依赖候选 crate；输出为逐行 `key=value`，由
// `scripts/probe_m4_export.py` 解析。坐标一律为 PDF 用户空间（原点在左下），
// 与 MediaBox 同单位（pt）。失败时打印 `OPEN_FAIL` 并以非零退出，供探针失败关闭。

import Foundation
import PDFKit
import CoreGraphics
import AppKit

/// 光栅化整页后求“非白墨迹”的包围盒。
///
/// 探针把内容区边框画成最外层的闭合路径，文字与图片都在其内部，因此
/// **全部非白像素的包围盒就等于内容区包围盒**。这样四边边距成为可回读的
/// 几何事实：只靠文本墨迹无法证明右/下边距（末行天然是参差的）。
/// 返回 PDF 用户空间（原点左下）的 x,y,w,h；页面全白时返回 nil。
func inkBounds(of page: PDFPage) -> (Double, Double, Double, Double)? {
    let media = page.bounds(for: .mediaBox)
    let scale: CGFloat = 4.0
    let width = Int((media.width * scale).rounded())
    let height = Int((media.height * scale).rounded())
    guard width > 0, height > 0 else { return nil }
    guard let context = CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width * 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else { return nil }
    context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: CGFloat(width), height: CGFloat(height)))
    context.scaleBy(x: scale, y: scale)
    page.draw(with: .mediaBox, to: context)
    guard let data = context.data else { return nil }
    let buffer = data.bindMemory(to: UInt8.self, capacity: width * height * 4)

    var minX = width, minY = height, maxX = -1, maxY = -1
    for y in 0..<height {
        let row = y * width * 4
        for x in 0..<width {
            let offset = row + x * 4
            // 抗锯齿会留下极浅的灰；阈值 240 只把真正着墨的像素算作墨迹。
            if buffer[offset] < 240 || buffer[offset + 1] < 240 || buffer[offset + 2] < 240 {
                if x < minX { minX = x }
                if x > maxX { maxX = x }
                if y < minY { minY = y }
                if y > maxY { maxY = y }
            }
        }
    }
    if maxX < 0 { return nil }
    let x0 = Double(minX) / Double(scale)
    let x1 = Double(maxX + 1) / Double(scale)
    // 位图首行是图像顶部，PDF 用户空间原点在左下，故 y 需翻转。
    let top = Double(minY) / Double(scale)
    let bottom = Double(maxY + 1) / Double(scale)
    return (x0, media.height - bottom, x1 - x0, bottom - top)
}

func fail(_ message: String) -> Never {
    print("OPEN_FAIL \(message)")
    exit(1)
}

let args = CommandLine.arguments
guard args.count >= 2 else { fail("missing path") }
let path = args[1]

/// 可选：`<pdf> <页码> <out.png>` 把指定页渲染成 PNG，供人工复核。
/// 页码从 1 起，与 PDF 阅读器一致。缺省时只做回读，不渲染。
if args.count >= 4 {
    guard let pageNumber = Int(args[2]), pageNumber >= 1 else { fail("bad page \(args[2])") }
    guard let document = PDFDocument(url: URL(fileURLWithPath: path)) else {
        fail("cannot open \(path)")
    }
    guard pageNumber <= document.pageCount, let page = document.page(at: pageNumber - 1) else {
        fail("no page \(pageNumber)")
    }
    let media = page.bounds(for: .mediaBox)
    let scale: CGFloat = 2.0
    let width = Int((media.width * scale).rounded())
    let height = Int((media.height * scale).rounded())
    guard let context = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else { fail("no context") }
    context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: CGFloat(width), height: CGFloat(height)))
    context.scaleBy(x: scale, y: scale)
    page.draw(with: .mediaBox, to: context)
    guard let image = context.makeImage() else { fail("no image") }
    let bitmap = NSBitmapImageRep(cgImage: image)
    guard let data = bitmap.representation(using: .png, properties: [:]) else { fail("no png") }
    do {
        try data.write(to: URL(fileURLWithPath: args[3]))
    } catch {
        fail("cannot write \(args[3]): \(error.localizedDescription)")
    }
    print("RENDER_OK \(args[3]) page=\(pageNumber) size=\(width)x\(height)")
    exit(0)
}

guard let document = PDFDocument(url: URL(fileURLWithPath: path)) else {
    fail("cannot open \(path)")
}

print("pages=\(document.pageCount)")

for index in 0..<document.pageCount {
    guard let page = document.page(at: index) else {
        fail("missing page \(index + 1)")
    }
    let media = page.bounds(for: .mediaBox)
    print(String(format: "page.%d.media=%.3f,%.3f", index + 1, media.width, media.height))

    // 内容区包围盒：页边距是否按四边生效的可回读几何事实。
    if let ink = inkBounds(of: page) {
        print(String(format: "page.%d.ink=%.3f,%.3f,%.3f,%.3f", index + 1, ink.0, ink.1, ink.2, ink.3))
    } else {
        print("page.\(index + 1).ink=none")
    }

    let text = page.string ?? ""
    let escaped = text
        .replacingOccurrences(of: "\\", with: "\\\\")
        .replacingOccurrences(of: "\n", with: "\\n")
        .replacingOccurrences(of: "\r", with: "")
    print("page.\(index + 1).chars=\(text.count)")
    print("page.\(index + 1).text=\(escaped)")

    // 逐行墨迹包围盒：PDFKit 的 line selection 以 UTF-16 计数。
    var lineRects: [String] = []
    let ns = text as NSString
    var location = 0
    while location < ns.length {
        let remaining = NSRange(location: location, length: ns.length - location)
        let lineRange = ns.lineRange(for: remaining)
        let lineText = ns.substring(with: lineRange).trimmingCharacters(in: .whitespacesAndNewlines)
        if !lineText.isEmpty, let selection = page.selection(for: lineRange) {
            let rect = selection.bounds(for: page)
            if rect.width > 0 && rect.height > 0 {
                lineRects.append(String(format: "%.3f,%.3f,%.3f,%.3f",
                                        rect.origin.x, rect.origin.y, rect.width, rect.height))
            }
        }
        let next = lineRange.location + lineRange.length
        if next <= location { break }
        location = next
    }
    print("page.\(index + 1).lines=\(lineRects.count)")
    for (order, rect) in lineRects.enumerated() {
        print("page.\(index + 1).line.\(order)=\(rect)")
    }
}
