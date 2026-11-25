// V-02 DOCX 渲染回读：用 Quick Look 把 .docx 渲染成缩略图，证明产物能被
// macOS 的 Word 兼容渲染路径打开（不是 Word 本身，标签见验证计划）。
//
// 输出：成功打印 `RENDER_OK <path>` 并退出 0；失败打印 `RENDER_FAIL <原因>` 并退出 1。
// Quick Look 对无扩展名或损坏包会拒绝，因此该检查只证明“可被系统渲染管线打开”。

import Foundation
import QuickLookThumbnailing
import AppKit

func fail(_ message: String) -> Never {
    print("RENDER_FAIL \(message)")
    exit(1)
}

let args = CommandLine.arguments
guard args.count >= 3 else { fail("usage: <docx> <output.png>") }
let source = URL(fileURLWithPath: args[1])
let destination = URL(fileURLWithPath: args[2])

guard FileManager.default.fileExists(atPath: source.path) else { fail("missing \(source.path)") }

let request = QLThumbnailGenerator.Request(
    fileAt: source,
    size: CGSize(width: 1024, height: 1024),
    scale: 2.0,
    representationTypes: .all
)

let semaphore = DispatchSemaphore(value: 0)
var failure: String?

QLThumbnailGenerator.shared.generateBestRepresentation(for: request) { representation, error in
    defer { semaphore.signal() }
    if let error = error {
        failure = "thumbnail error: \(error.localizedDescription)"
        return
    }
    guard let representation = representation else {
        failure = "no representation"
        return
    }
    let bitmap = NSBitmapImageRep(cgImage: representation.cgImage)
    guard let data = bitmap.representation(using: .png, properties: [:]) else {
        failure = "cannot encode png"
        return
    }
    do {
        try data.write(to: destination)
    } catch {
        failure = "cannot write \(destination.path): \(error.localizedDescription)"
    }
}

if semaphore.wait(timeout: .now() + 60) == .timedOut {
    fail("thumbnail timeout")
}
if let failure = failure { fail(failure) }
print("RENDER_OK \(destination.path)")
