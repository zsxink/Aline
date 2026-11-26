// 用法：read_text <image.png> [language ...]
// 用系统 Vision 文本识别读取图片文字，按识别结果输出 JSON：{"lines":[...]}
// 只读取传入图片，不访问其它文件；语言缺省为 zh-Hans 与 en-US。
import Foundation
import ImageIO
import Vision

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write("usage: read_text <image> [language ...]\n".data(using: .utf8)!)
    exit(2)
}
let url = URL(fileURLWithPath: args[1])
guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    FileHandle.standardError.write("cannot load image\n".data(using: .utf8)!)
    exit(3)
}
let languages = Array(args.dropFirst(2))
let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = false
if !languages.isEmpty {
    request.recognitionLanguages = languages
}
let handler = VNImageRequestHandler(cgImage: image, options: [:])
do {
    try handler.perform([request])
} catch {
    FileHandle.standardError.write("vision failed: \(error)\n".data(using: .utf8)!)
    exit(4)
}
let lines = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }
let payload: [String: Any] = ["lines": lines]
let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
