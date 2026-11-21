// 输入源选择器：在系统输入源列表中按 ID 选择输入源，走公开 TIS API。
//
// 用法：select_input_source <input-source-id>
//   打印被选中输入源的 ID 与 TISSelectInputSource 返回码；找不到时以 2 退出。
// 只切换当前输入源，不改动启用列表与用户配置；取证结束后按记录的 ID 切回。
import Carbon
import Foundation

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write("usage: select_input_source <input-source-id>\n".data(using: .utf8)!)
    exit(2)
}
let wanted = args[1]

func sourceID(_ source: TISInputSource) -> String? {
    guard let pointer = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) else {
        return nil
    }
    return Unmanaged<CFString>.fromOpaque(pointer).takeUnretainedValue() as String
}

// 与 im-select 一致：默认列表（includeAllInstalled=false）里的对象才可直接
// 选中；全量列表中的未启用对象可能返回 paramErr(-50)。
guard let raw = TISCreateInputSourceList(nil, false)?.takeRetainedValue() else {
    FileHandle.standardError.write("failed to list input sources\n".data(using: .utf8)!)
    exit(1)
}
let sources = raw as! [TISInputSource]
for source in sources where sourceID(source) == wanted {
    let status = TISSelectInputSource(source)
    print("selected \(wanted) status=\(status)")
    exit(status == noErr ? 0 : 1)
}
let available = sources.compactMap(sourceID).sorted().joined(separator: "\n")
FileHandle.standardError.write("input source not found: \(wanted)\navailable:\n\(available)\n".data(using: .utf8)!)
exit(2)
