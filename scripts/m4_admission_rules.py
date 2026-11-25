#!/usr/bin/env python3
"""M4 后端与平台适配准入的纯规则层：冻结候选身份、许可证、校验和、依赖边、
目标/features 与逐场景预期裁决。

本模块只含常量与纯函数，不读磁盘、不起进程、不编译 Rust，因此可以在没有
锁定的 Cargo 缓存、没有系统的环境里独立自测。真实探针
`scripts/probe_m4_admission.py` 用它把“准入期望”和“门禁实测”分开：身份、
校验和、许可证与边在这里冻结，实测只负责证明当前策略快照下门禁如何裁决。

**本模块不是准入结论。** 期望值来自 2026-11-26 在**未改动的工作区副本**上
对 `engineering_dependencies.validate()` 的原始测量；其中三项需要用户裁决
（见 `RULINGS_REQUIRED`），在这些项获批前不得据此开始生产接线，也不得把
本模块的 `verdict` 读成“已准入”。

冻结范围：
- `PACKAGES`：相对基线新增的包身份（名称@版本 → 校验和/许可证/rust-version）。
- `DIRECT`：要写进 workspace 清单的直接依赖行与消费方。
- `GROUPS`：逐组注入的直接依赖、相对基线的新增包、GPUI 受审子图漂移行、
  新增禁库入边、绕过 GPUI 受审根的原生目标路径，以及七场景预期裁决。
- `EMBEDDED_ASSETS`：候选 crate 内嵌、**不受 SPDX 元数据覆盖**的第三方素材；
  其许可证不在准入白名单内，必须单独裁决。
- `PLATFORM_ADAPTERS`：M4 平台适配所需能力与锁定 GPUI 的既有入口；本轮全部
  零新增依赖。

策略快照：`POLICY_SHA256` 对应 2026-11-26 的门禁策略文件。策略一旦变化，
预期裁决就要重新冻结，探针会把快照不一致写成证据而不是静默沿用。
"""

import re

# ---------------------------------------------------------------- 场景与裁决

# 与 `scripts/engineering_policy.json` 的七个场景一一对应。
SCENARIOS = (
    "all",
    "macos-arm",
    "macos-intel",
    "windows-x64",
    "linux-x64",
    "linux-arm",
    "all-features",
)

# 2026-11-26 策略快照；探针据此判断本表的期望是否仍然同源。
POLICY_SHA256 = {
    "scripts/engineering_policy.json":
        "4555fa107824479777fb3aed905012add29f3ec37fadb145e80902201b1af75a",
    "scripts/dependency_admission_policy.json":
        "9a80938c1c69991add885b61962f25413e96b94f327cfe61cd7b1f3c0279246c",
}

# 当前已批准的七场景 GPUI 受审子图指纹；新增依赖获批后由接线任务替换。
APPROVED_GPUI_FINGERPRINTS = {
    "all": "3a57cfcb0907ceec19418888259685bec9021706510b8f4acdb2c294559cf69d",
    "all-features": "3a57cfcb0907ceec19418888259685bec9021706510b8f4acdb2c294559cf69d",
    "macos-arm": "0fd9fbdc366bfc2e37b96f3c895e7f3bc7278c4284ea29269cc2f6773ada4815",
    "macos-intel": "a7cf9d9e5abfa209a8c54961daa64a4c426a6c4a1c16d122967083c839c20819",
    "windows-x64": "d417bf93757eb0bb96dbe012e5c2861a5deeb6e22a2b94d695d8d179bbbf2159",
    "linux-x64": "80bca853c98ef36a2f12a008c0baf1f6115442a3324d2b5a1ea266a6b6064c5b",
    "linux-arm": "639218209d396e02c15885620d2dd10630148ee435f1d76a42199412788dccb4",
}

# 裁决类别 → 门禁拒绝消息的稳定前缀。`pass` 不产生拒绝。
PASS = "pass"
GPUI_DRIFT = "gpui-drift"
NATIVE_PATH = "native-path"
GUARD_BYPASS = "guard-bypass"
ADMISSION_GROUP = "admission-group"

REJECTION_PREFIX = {
    GPUI_DRIFT: "正式 GPUI 子图身份/来源/features/边/选包漂移，须重新审计",
    NATIVE_PATH: "原生目标分支出现获批根以外路径: wasip2@1.0.4+wasi-0.2.12",
    GUARD_BYPASS: "禁库或 GPUI 例外旁路",
    ADMISSION_GROUP: "已选包未进入运行时准入分组: latex-rust@2.1.1",
}

VERDICTS = (PASS, GPUI_DRIFT, NATIVE_PATH, GUARD_BYPASS, ADMISSION_GROUP)

SOURCE = "registry+https://github.com/rust-lang/crates.io-index"

# 2026-11-26 待用户裁决项；获批前不得开始相应生产接线。
RULINGS_REQUIRED = (
    {
        "id": "export-closure",
        "question": "是否按本表精确身份、校验和、边与 features 登记 krilla/docx-rs/zip 闭包，"
                    "并采用探针重算的七场景 GPUI 指纹？",
        "candidates": ("krilla@0.8.2", "docx-rs@0.4.22", "zip@8.6.0"),
        "consequence": "不批准则 4.4 导出、5.2 主题包安装与主题 ZIP 导入无后端可用。",
    },
    {
        "id": "latex-rust-assets",
        "question": "是否准入内嵌 SIL OFL-1.1 字体与 LPPL-1.3c 色表的 latex-rust@2.1.1？",
        "candidates": ("latex-rust@2.1.1",),
        "consequence": "不批准则 4.2 公式无可锁定后端，须缩减 PRD 范围或另找候选。",
    },
    {
        "id": "regex-guard",
        "question": "是否为 mermaid-rs-renderer/syntect 扩展禁库守卫、新增精确入边并重定基线？",
        "candidates": ("mermaid-rs-renderer@0.3.1", "syntect@5.3.0"),
        "consequence": "不批准则 4.3 Mermaid 独立图像与 4.1/4.5 代码高亮无后端，须缩减 PRD 范围。",
    },
)

# ---------------------------------------------------------------- 新增包身份
#
# 只登记相对当前基线**新增**的包。基线已有、仅 features 变化的包（flate2、
# bytemuck、regex-automata、fontdb 等）保持同身份、同校验和、同许可证，漂移
# 只出现在 `GROUPS[*].gpui_drift`。
PACKAGES = {
    # 主题/资源 ZIP
    "zip@8.6.0": {
        "checksum": "2d04a6b5381502aa6087c94c669499eb1602eb9c5e8198e534de571f7154809b",
        "license": "MIT", "rust_version": "1.88",
        "role": "主题包与资源 ZIP 的受限读写",
    },
    "typed-path@0.12.4": {
        "checksum": "54ec2755506471f54baf5e4bb5499d8e510cc1caf271270e778a736bd2d7336c",
        "license": "MIT OR Apache-2.0", "rust_version": "1.65.0",
        "role": "zip 的路径类型依赖",
    },
    # PDF
    "krilla@0.8.2": {
        "checksum": "27da593198b20eeba65caeb73c2bbeec3e53ab08fa549898312ce81c4fce5e33",
        "license": "MIT OR Apache-2.0", "rust_version": "1.92",
        "role": "PDF 生成后端",
    },
    "pdf-writer@0.15.0": {
        "checksum": "f5e456864a7a304047bff84977dc6fb162bd956475d40ba50b2dcecaada7f753",
        "license": "MIT OR Apache-2.0", "rust_version": None,
        "role": "PDF 对象写出",
    },
    "xmp-writer@0.3.3": {
        "checksum": "9440ea3e5aeabb0ac63af70daf835274065238cdd0cec83418f417eae38bacee",
        "license": "MIT OR Apache-2.0", "rust_version": None,
        "role": "PDF XMP 元数据",
    },
    "subsetter@0.2.6": {
        "checksum": "38803281d1c23166c5ebcb455439a5d2afe711cc909cf88af72448c297756ad6",
        "license": "MIT OR Apache-2.0", "rust_version": "1.85",
        "role": "PDF 字体子集化",
    },
    "skrifa@0.42.1": {
        "checksum": "0c34617370ae968efb7161bb2beb517d9084659aae19e24b89e3db25b46e4564",
        "license": "MIT OR Apache-2.0", "rust_version": "1.85",
        "role": "字体整形与度量",
    },
    "read-fonts@0.39.2": {
        "checksum": "c4ed38b89c2c77ff968c524145ad65fb010f38af5c7a224b53b81d47ac2daa81",
        "license": "MIT OR Apache-2.0", "rust_version": "1.85",
        "role": "字体表读取",
    },
    "write-fonts@0.48.1": {
        "checksum": "cb731d4c4d93eacc69a1ad2f270f905788a98e4a3438267bcafbe08d3431c8d8",
        "license": "MIT OR Apache-2.0", "rust_version": "1.85",
        "role": "字体表写出",
    },
    "font-types@0.11.3": {
        "checksum": "5b38ad915f6dadd993ced50848a8291a543bd41ca62bc10740d5e64e2ab4cfd7",
        "license": "MIT OR Apache-2.0", "rust_version": "1.85",
        "role": "字体基础类型",
    },
    "tiny-skia-path@0.12.0": {
        "checksum": "edca365c3faccca67d06593c5980fa6c57687de727a03131735bb85f01fdeeb9",
        "license": "BSD-3-Clause", "rust_version": None,
        "role": "PDF 路径（与 tiny-skia 0.11 的 path 版本不同）",
    },
    "bytemuck_derive@1.12.1": {
        "checksum": "6a1f896587b6f2c069c73d2f0913e2d590c3990285cd2f0b6aa02b786b4c679c",
        "license": "Zlib OR Apache-2.0 OR MIT", "rust_version": "1.71",
        "role": "krilla 拉入的 bytemuck derive 宏",
    },
    # DOCX
    "docx-rs@0.4.22": {
        "checksum": "7fdf00e8af6d0b3e92d4bbf9b76f773d8b84ea80f310324ad16cbdc2e653e02c",
        "license": "MIT", "rust_version": None,
        "role": "DOCX 生成后端（不得启用 image feature）",
    },
    "quick-xml@0.41.0": {
        "checksum": "e660451e55124f798a69a5af3f49ccfbefbd41910eefd25caf2393e1f3473ec1",
        "license": "MIT", "rust_version": "1.79",
        "role": "DOCX XML 读写",
    },
    "zopfli@0.8.4": {
        "checksum": "aaf7fc5d30c28483d93805c4a5e12b05bbb52407fa67c5f8bd552374cd01fb11",
        "license": "Apache-2.0", "rust_version": "1.88.0",
        "role": "docx-rs 的 zip deflate 写端",
    },
    "simdutf8@0.1.5": {
        "checksum": "e3a9fe34e3e7a50316060351f37187a3f546bce95496156754b601a5fa71b76e",
        "license": "MIT OR Apache-2.0", "rust_version": None,
        "role": "UTF-8 校验",
    },
    "encoding_rs@0.8.44": {
        "checksum": "89253175e704ab869b04f9db224d38630e0211a53841a79181deae0889e169f5",
        "license": "(Apache-2.0 OR MIT) AND BSD-3-Clause", "rust_version": "1.88",
        "role": "quick-xml encoding feature 的编码转换",
    },
    "core_detect@1.0.0": {
        "checksum": "7f8f80099a98041a3d1622845c271458a2d73e688351bf3cb999266764b81d48",
        "license": "MIT/Apache-2.0", "rust_version": None,
        "role": "simdutf8 的运行时特性检测（slash 写法按 legacy 规则视为 OR）",
    },
    "multiversion_no_op@1.0.0": {
        "checksum": "743fb55ba31b18fb1ecef6bdc9aa2743314978ac084044301a7eee33fb99a20d",
        "license": "Apache-2.0 OR MIT", "rust_version": "1.56",
        "role": "multiversion 的非分发占位包",
    },
    # 语法高亮
    "syntect@5.3.0": {
        "checksum": "656b45c05d95a5704399aeef6bd0ddec7b2b3531b7c9e900abbf7c4d2190c925",
        "license": "MIT", "rust_version": None,
        "role": "代码语言高亮（regex-fancy，不启用 onig）",
    },
    "fancy-regex@0.16.2": {
        "checksum": "998b056554fbe42e03ae0e152895cd1a7e1002aec800fdc6635d20270260c46f",
        "license": "MIT", "rust_version": "1.66",
        "role": "syntect 的纯 Rust 正则引擎",
    },
    "bincode@1.3.3": {
        "checksum": "b1f45e9417d87227c7a56d22e471c6206462cba514c7590c09aff4cf6d1ddcad",
        "license": "MIT", "rust_version": None,
        "role": "syntect dump-load 语法包反序列化",
    },
    "bit-set@0.8.0": {
        "checksum": "08807e080ed7f9d5433fa9b275196cfc35414f66a0c79d864dc51a0d825231a3",
        "license": "Apache-2.0 OR MIT", "rust_version": None,
        "role": "fancy-regex 的位集合",
    },
    "bit-vec@0.8.0": {
        "checksum": "5e764a1d40d510daf35e07be9eb06e75770908c27d411ee6c92109c9840eaaf7",
        "license": "Apache-2.0 OR MIT", "rust_version": None,
        "role": "bit-set 的底层位向量",
    },
    "fnv@1.0.7": {
        "checksum": "3f9eec918d3f24069decb9af1554cad7c880e2da24a9afd88aca000531ab82c1",
        "license": "Apache-2.0 / MIT", "rust_version": None,
        "role": "syntect 的哈希（slash 写法按 legacy 规则视为 OR）",
    },
    # Mermaid
    "mermaid-rs-renderer@0.3.1": {
        "checksum": "6cb5b469222ce6b9896edf0a969cf68464f5a028ef394fd6643f00bd943df117",
        "license": "MIT", "rust_version": None,
        "role": "Mermaid 图类型解析与 SVG 输出（default-features=false，不启用 png/cli）",
    },
    "json5@1.3.8": {
        "checksum": "6c1d229cedc4e210dd2e2a6cdc59e36ce28e7338fb69367d0d2566e26de7d599",
        "license": "MIT", "rust_version": None,
        "role": "Mermaid 主题配置解析",
    },
    "ucd-trie@0.1.8": {
        "checksum": "8f20bdbfc9915686faa18fb5bbe8b4552778505defbf9f12289beefbc44092a6",
        "license": "MIT OR Apache-2.0", "rust_version": None,
        "role": "json5 的 Unicode 字符表",
    },
    # 公式
    "latex-rust@2.1.1": {
        "checksum": "f8094b939c8af51f9c7d08256ffa607b743e24d466d9a0ccc602dc748c911e54",
        "license": "MIT OR Apache-2.0", "rust_version": "1.76",
        "role": "公式排版（std，不启用 png/egui）；内嵌素材见 EMBEDDED_ASSETS",
    },
}

# 直接依赖行与消费方。`features` 是**声明**值；实际选中的 features 由 Cargo
# 统一决定，探针以 metadata 为准。
DIRECT = {
    "zip@8.6.0": {
        "line": 'zip = { version = "=8.6.0", default-features = false, '
                'features = ["deflate-flate2-zlib-rs"] }',
        "features": ("deflate-flate2-zlib-rs",),
        "consumer": "aline-plugin",
        "contract": "V-04/V-06 主题包；1.4 探针用 `deflate`（含 zopfli 写端），"
                    "读路径同为 flate2/zlib-rs",
        "status": "needs-approval",
    },
    "krilla@0.8.2": {
        "line": 'krilla = { version = "=0.8.2", default-features = false, '
                'features = ["raster-images", "simple-text"] }',
        "features": ("raster-images", "simple-text"),
        "consumer": "aline-plugin",
        "contract": "V-02 EXPORT-02～04；krilla 不提供换行/段落排版，须由 aline-plugin 补齐",
        "status": "needs-approval",
    },
    "docx-rs@0.4.22": {
        "line": 'docx-rs = { version = "=0.4.22", default-features = false }',
        "features": (),
        "consumer": "aline-plugin",
        "contract": "V-02 Word 样式与可编辑边界；4.4 前须确认 CT_PPr 段序缺陷的上游修复或规避。"
                    "**不得启用 image feature**：它会打开 docx-rs→image→ravif→rav1e→getrandom→wasip2 "
                    "的非 GPUI metadata 路径。图片改用 Pic::new_with_dimensions + tiny-skia 预编码 PNG。",
        "status": "needs-approval",
    },
    "usvg@0.46.0": {
        "line": 'usvg = { version = "=0.46.0", default-features = false }',
        "features": (),
        "consumer": "aline-ui",
        "contract": "V-03 SVG 安全边界：必须安装只允许 data: 的 ImageHrefResolver，"
                    "不能使用 SvgRenderer::new 的默认 Options",
        "status": "admissible-now",
    },
    "resvg@0.46.0": {
        "line": 'resvg = { version = "=0.46.0", default-features = false }',
        "features": (),
        "consumer": "aline-ui",
        "contract": "V-03 SVG/图表栅格化；与 mermaid 的 png feature（usvg/resvg 0.47）互斥使用",
        "status": "admissible-now",
    },
    "tiny-skia@0.11.4": {
        "line": 'tiny-skia = { version = "=0.11.4", default-features = false, '
                'features = ["std", "simd"] }',
        "features": ("std", "simd"),
        "consumer": "aline-ui",
        "contract": "V-03 像素缓冲与 PNG 预编码（Pixmap::encode_png）",
        "status": "admissible-now",
    },
    "latex-rust@2.1.1": {
        "line": 'latex-rust = { version = "=2.1.1", default-features = false, '
                'features = ["std"] }',
        "features": ("std",),
        "consumer": "aline-plugin",
        "contract": "V-03 七类公式语法；内嵌素材另需许可证裁决",
        "status": "needs-license-ruling",
    },
    "mermaid-rs-renderer@0.3.1": {
        "line": 'mermaid-rs-renderer = { version = "=0.3.1", default-features = false }',
        "features": (),
        "consumer": "aline-plugin",
        "contract": "V-03 23 类图类型；只取 SVG，栅格化交给 resvg 0.46。"
                    "不启用 png feature（会带入 usvg/resvg 0.47）与 cli feature",
        "status": "needs-guard-ruling",
    },
    "syntect@5.3.0": {
        "line": 'syntect = { version = "=5.3.0", default-features = false, '
                'features = ["parsing", "default-syntaxes", "default-themes", '
                '"dump-load", "regex-fancy"] }',
        "features": ("parsing", "default-syntaxes", "default-themes", "dump-load",
                     "regex-fancy"),
        "consumer": "aline-plugin",
        "contract": "PLUGIN-05/PL-02；不启用 onig（C 依赖）与 html/plist/yaml/dump-create",
        "status": "needs-guard-ruling",
    },
}

# 同一功能不得引入第二个库；每条候选都要写明为何不能复用基线包或已有候选。
NECESSITY = {
    "zip@8.6.0": "基线无 ZIP 读写；1.4 已按 zip 8.6.0 冻结条目/解压预算，换库会使契约失效。",
    "krilla@0.8.2": "V-02 实测四边边距、CJK 字体子集与产物大小均达标；printpdf 因 MPL/tokio 被排除。",
    "docx-rs@0.4.22": "V-02 实测 Word 样式/编号/表格/图片/脚注均可用；无第二个 DOCX 候选进入实测。",
    "usvg@0.46.0": "锁定 GPUI 已含 usvg 0.46；直依赖只为安装 data:-only resolver，不新增包。",
    "resvg@0.46.0": "锁定 GPUI 已含 resvg 0.46；直依赖用于 SVG/图表栅格化，不新增包。",
    "tiny-skia@0.11.4": "锁定 GPUI 已含 tiny-skia 0.11；直依赖用于 PNG 预编码与像素缓冲，不新增包。",
    "latex-rust@2.1.1": "V-03 七类公式在同一后端上实测通过；无第二个公式候选进入实测。",
    "mermaid-rs-renderer@0.3.1": "V-03 23 类图类型实测通过；§3.2 禁止借浏览器/WebView 补图表。",
    "syntect@5.3.0": "PLUGIN-05 需要语言识别与 scope 着色；无第二个高亮候选进入实测。",
}

# 直接依赖但**不新增包**：这些身份已在锁定 GPUI 的闭包里，直依赖只是为了让
# aline-ui 能直接调用其 API。探针用“该组新增包为空”证明它们没有改变依赖图。
BASELINE_DIRECT = {
    "usvg@0.46.0": "锁定 GPUI 的 SVG 渲染已引入 usvg 0.46。",
    "resvg@0.46.0": "锁定 GPUI 的 SVG 栅格化已引入 resvg 0.46。",
    "tiny-skia@0.11.4": "锁定 GPUI 的像素缓冲已引入 tiny-skia 0.11。",
}

# ---------------------------------------------------------------- 内嵌素材
#
# 这些文件随 crate 发布，但**不在 crate 的 SPDX 许可证元数据内**。现行
# 许可证白名单没有 OFL-1.1 / LPPL-1.3c，因此不能把它们当作“MIT OR Apache-2.0
# 已覆盖”。记录在此，供用户单独裁决。
EMBEDDED_ASSETS = (
    {
        "package": "latex-rust@2.1.1",
        "path": "fonts/stix-two-math/STIXTwoMath-Regular.otf",
        "bytes": 838652,
        "license": "OFL-1.1",
        "license_file": "fonts/stix-two-math/OFL.txt",
        "obligation": "分发时随附 OFL 文本；不得单独售卖字体；不得用保留字体名发布修改版。",
    },
    {
        "package": "latex-rust@2.1.1",
        "path": "data/dvipsnames.tsv",
        "bytes": 3212,
        "license": "LPPL-1.3c",
        "license_file": "NOTICE",
        "obligation": "分发时保留文件头与 NOTICE 的修改声明，并保留获取原始 graphics bundle 的指引。",
    },
)

# 内嵌素材许可证必须**不**被现行白名单覆盖，否则本表就失去了“待裁决”的理由。
# 探针自测拿它和白名单对照；若上游换了许可证，这条会先失败。
EMBEDDED_ASSET_LICENSES = ("OFL-1.1", "LPPL-1.3c")

# ---------------------------------------------------------------- 平台适配
#
# M4 的平台能力全部由锁定 GPUI 提供；本轮不新增任何依赖。行号是
# 2026-11-26 在 vendor/gpui-pre-0.3.9 与 gpui-pre-macos 0.3.9 源码中的位置。
PLATFORM_ADAPTERS = (
    {
        "capability": "打开 http/https/mailto 链接",
        "tasks": ("3.4",),
        "api": "App::open_url",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1809",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:831",
        "dependency": None,
    },
    {
        "capability": "在文件管理器中揭示文件",
        "tasks": ("3.4",),
        "api": "App::reveal_path",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1913",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:1023",
        "dependency": None,
    },
    {
        "capability": "用系统默认应用打开文件",
        "tasks": ("3.4",),
        "api": "App::open_with_system",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1918",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:1043",
        "dependency": None,
    },
    {
        "capability": "选择已存在文件/目录（导入主题、图片另存为）",
        "tasks": ("3.4", "5.2"),
        "api": "App::prompt_for_paths",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1891",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:898",
        "dependency": None,
    },
    {
        "capability": "选择新文件路径（导出目标、另存为）",
        "tasks": ("4.4", "5.2"),
        "api": "App::prompt_for_new_path",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1904",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:946",
        "dependency": None,
    },
    {
        "capability": "读取当前窗口外观",
        "tasks": ("5.4",),
        "api": "App::window_appearance",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1698",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:800",
        "dependency": None,
    },
    {
        "capability": "设置/跟随系统外观",
        "tasks": ("5.4",),
        "api": "App::set_window_appearance",
        "gpui": "vendor/gpui-pre-0.3.9/src/app.rs:1712",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:808",
        "dependency": None,
    },
    {
        "capability": "观察外观变化",
        "tasks": ("5.4",),
        "api": "Window::observe_window_appearance",
        "gpui": "vendor/gpui-pre-0.3.9/src/window.rs:2314",
        "implementation": "gpui-pre-macos-0.3.9/src/platform.rs:800",
        "dependency": None,
    },
)

# ---------------------------------------------------------------- 分组
#
# 每组是“把哪些直接依赖放进工作区副本”的独立实验。`verdict` 是门禁在七场景
# 下的首条拒绝类别；`pass` 表示七场景全部通过。`gpui_*_rows` 是相对**同一场景
# 基线**的 GPUI 受审子图行差异，不是相对 all-target metadata 的原始快照。
_PASS = {scenario: PASS for scenario in SCENARIOS}


def _same(verdict):
    return {scenario: verdict for scenario in SCENARIOS}


# 同一组的 GPUI 受审子图漂移在不同场景下不同：桌面 target tree 只选该目标真正
# 需要的包，`all` 会把各目标分支一起选上。`desktop_extra` 只在五个桌面场景生效。
def _drift(changed, desktop_extra=(), new=()):
    rows = {}
    for scenario in SCENARIOS:
        extra = () if scenario in ("all", "all-features") else tuple(desktop_extra)
        rows[scenario] = (tuple(sorted(set(new))),
                          tuple(sorted(set(changed) | set(extra))))
    return rows


GROUPS = {
    "baseline": {
        "workspace_deps": (),
        "plugin_deps": (),
        "ui_deps": (),
        "expected_verdicts": _PASS,
        "new_packages": (),
        "gpui_drift": _drift(()),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "svg-direct": {
        "workspace_deps": ("usvg@0.46.0", "resvg@0.46.0", "tiny-skia@0.11.4"),
        "plugin_deps": (),
        "ui_deps": ("usvg", "resvg", "tiny-skia"),
        "expected_verdicts": _PASS,
        "new_packages": (),
        "gpui_drift": _drift(()),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "zip": {
        "workspace_deps": ("zip@8.6.0",),
        "plugin_deps": ("zip",),
        "ui_deps": (),
        "expected_verdicts": _same(GPUI_DRIFT),
        "new_packages": ("typed-path@0.12.4", "zip@8.6.0"),
        "gpui_drift": _drift(("flate2@1.1.10", "zlib-rs@0.6.8")),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "krilla": {
        "workspace_deps": ("krilla@0.8.2",),
        "plugin_deps": ("krilla",),
        "ui_deps": (),
        "expected_verdicts": _same(GPUI_DRIFT),
        "new_packages": (
            "bytemuck_derive@1.12.1",
            "font-types@0.11.3",
            "krilla@0.8.2",
            "pdf-writer@0.15.0",
            "read-fonts@0.39.2",
            "skrifa@0.42.1",
            "subsetter@0.2.6",
            "tiny-skia-path@0.12.0",
            "write-fonts@0.48.1",
            "xmp-writer@0.3.3",
        ),
        "gpui_drift": _drift(
            (
                "bytemuck@1.25.2",
                "float-cmp@0.9.0",
                "smallvec@1.16.4",
                "stable_deref_trait@1.2.1",
                "yoke@0.8.4",
                "zerofrom@0.1.8",
            ),
            # 桌面 target tree 会选中 krilla 直接依赖的 bumpalo；`all` 已含该选包。
            desktop_extra=("bumpalo@3.20.3",),
            new=("bytemuck_derive@1.12.1",),
        ),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "docx-image": {
        "workspace_deps": (
            'docx-rs = { version = "=0.4.22", default-features = false, '
            'features = ["image"] }',
        ),
        "plugin_deps": ("docx-rs",),
        "ui_deps": (),
        "expected_verdicts": _same(NATIVE_PATH),
        "new_packages": (
            "core_detect@1.0.0",
            "docx-rs@0.4.22",
            "encoding_rs@0.8.44",
            "multiversion_no_op@1.0.0",
            "quick-xml@0.41.0",
            "simdutf8@0.1.5",
            "typed-path@0.12.4",
            "zip@8.6.0",
            "zopfli@0.8.4",
        ),
        "gpui_drift": _drift(
            ("flate2@1.1.10", "zlib-rs@0.6.8"),
            # 桌面 tree 选中 docx-rs→zip→zopfli→bumpalo 与 encoding_rs(build)→rustversion。
            desktop_extra=("bumpalo@3.20.3", "rustversion@1.0.23"),
        ),
        "forbidden_edges": (),
        "outside_paths": (
            {
                "package": "wasip2@1.0.4+wasi-0.2.12",
                "route_contains": (
                    "aline-plugin@0.1.0",
                    "docx-rs@0.4.22",
                    "image@0.25.10",
                    "ravif@0.13.0",
                    "rav1e@0.8.1",
                    "rand@0.9.5",
                    "getrandom@0.3.4",
                    "wasip2@1.0.4+wasi-0.2.12",
                ),
                "target_condition":
                    'cfg(all(target_arch = "wasm32", target_os = "wasi", '
                    'target_env = "p2"))',
                "selected_by_desktop_trees": False,
            },
        ),
    },
    "docx-plain": {
        "workspace_deps": (
            'docx-rs = { version = "=0.4.22", default-features = false }',
        ),
        "plugin_deps": ("docx-rs",),
        "ui_deps": (),
        "expected_verdicts": _same(GPUI_DRIFT),
        "new_packages": (
            "core_detect@1.0.0",
            "docx-rs@0.4.22",
            "encoding_rs@0.8.44",
            "multiversion_no_op@1.0.0",
            "quick-xml@0.41.0",
            "simdutf8@0.1.5",
            "typed-path@0.12.4",
            "zip@8.6.0",
            "zopfli@0.8.4",
        ),
        "gpui_drift": _drift(
            ("flate2@1.1.10", "zlib-rs@0.6.8"),
            # 桌面 tree 选中 docx-rs→zip→zopfli→bumpalo 与 encoding_rs(build)→rustversion。
            desktop_extra=("bumpalo@3.20.3", "rustversion@1.0.23"),
        ),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "syntect": {
        "workspace_deps": ("syntect@5.3.0",),
        "plugin_deps": ("syntect",),
        "ui_deps": (),
        "expected_verdicts": _same(GUARD_BYPASS),
        "new_packages": (
            "bincode@1.3.3",
            "bit-set@0.8.0",
            "bit-vec@0.8.0",
            "fancy-regex@0.16.2",
            "fnv@1.0.7",
            "syntect@5.3.0",
        ),
        "gpui_drift": _drift(("regex-automata@0.4.18",)),
        "forbidden_edges": (
            ("fancy-regex@0.16.2", "regex-automata@0.4.18"),
            ("fancy-regex@0.16.2", "regex-syntax@0.8.11"),
            ("syntect@5.3.0", "regex-syntax@0.8.11"),
        ),
        "outside_paths": (),
    },
    "mermaid": {
        "workspace_deps": ("mermaid-rs-renderer@0.3.1",),
        "plugin_deps": ("mermaid-rs-renderer",),
        "ui_deps": (),
        "expected_verdicts": _same(GUARD_BYPASS),
        "new_packages": (
            "json5@1.3.8",
            "mermaid-rs-renderer@0.3.1",
            "ucd-trie@0.1.8",
        ),
        "gpui_drift": _drift(("fontdb@0.23.0",)),
        "forbidden_edges": (
            ("mermaid-rs-renderer@0.3.1", "regex@1.13.1"),
        ),
        "outside_paths": (),
    },
    "latex": {
        "workspace_deps": ("latex-rust@2.1.1",),
        "plugin_deps": ("latex-rust",),
        "ui_deps": (),
        "expected_verdicts": _same(ADMISSION_GROUP),
        "new_packages": ("latex-rust@2.1.1",),
        "gpui_drift": _drift(()),
        "forbidden_edges": (),
        "outside_paths": (),
    },
    "full-plain": {
        "workspace_deps": (
            "krilla@0.8.2", "docx-rs@0.4.22", "zip@8.6.0", "latex-rust@2.1.1",
            "usvg@0.46.0", "resvg@0.46.0", "tiny-skia@0.11.4",
        ),
        "plugin_deps": ("krilla", "docx-rs", "zip", "latex-rust"),
        "ui_deps": ("usvg", "resvg", "tiny-skia"),
        "expected_verdicts": _same(GPUI_DRIFT),
        "new_packages": (
            "bytemuck_derive@1.12.1",
            "core_detect@1.0.0",
            "docx-rs@0.4.22",
            "encoding_rs@0.8.44",
            "font-types@0.11.3",
            "krilla@0.8.2",
            "latex-rust@2.1.1",
            "multiversion_no_op@1.0.0",
            "pdf-writer@0.15.0",
            "quick-xml@0.41.0",
            "read-fonts@0.39.2",
            "simdutf8@0.1.5",
            "skrifa@0.42.1",
            "subsetter@0.2.6",
            "tiny-skia-path@0.12.0",
            "typed-path@0.12.4",
            "write-fonts@0.48.1",
            "xmp-writer@0.3.3",
            "zip@8.6.0",
            "zopfli@0.8.4",
        ),
        "gpui_drift": _drift(
            (
                "bytemuck@1.25.2",
                "flate2@1.1.10",
                "float-cmp@0.9.0",
                "smallvec@1.16.4",
                "stable_deref_trait@1.2.1",
                "yoke@0.8.4",
                "zerofrom@0.1.8",
                "zlib-rs@0.6.8",
            ),
            desktop_extra=("bumpalo@3.20.3", "rustversion@1.0.23"),
            new=("bytemuck_derive@1.12.1",),
        ),
        "forbidden_edges": (),
        "outside_paths": (),
    },
}

# 门禁拒绝顺序的实测事实：原生路径 → 禁库守卫 → GPUI 指纹 → 运行时分组。
# 记录在此是因为它决定“为什么某组只报第一条”。顺序本身由
# `engineering_dependencies.validate()` 固定，探针不改变它。
GATE_ORDER = ("structure", "tree-reachability", "native-paths", "metadata-bypass",
              "incoming-edges", "gpui-fingerprint", "admission-groups")


class ContractError(ValueError):
    """规则表自相矛盾或不符合冻结口径；探针据此失败关闭。"""


def package_key(name, version):
    """返回 `名称@版本` 身份串。"""
    return "%s@%s" % (name, version)


def split_key(key):
    """把 `名称@版本` 拆成 (name, version)；版本里的 `+build` 保留。"""
    name, _, version = key.partition("@")
    if not name or not version:
        raise ContractError("包身份缺少名称或版本: " + key)
    return name, version


def expected_rejection(group, scenario):
    """返回该组在该场景的预期拒绝前缀；通过时返回 None。"""
    if group not in GROUPS:
        raise ContractError("未知分组: " + group)
    if scenario not in SCENARIOS:
        raise ContractError("未知场景: " + scenario)
    verdict = GROUPS[group]["expected_verdicts"][scenario]
    if verdict == PASS:
        return None
    return REJECTION_PREFIX[verdict]


def verdict_class(message):
    """把门禁错误消息映射回裁决类别；无法归类时抛 ContractError。"""
    for verdict in (NATIVE_PATH, GUARD_BYPASS, GPUI_DRIFT, ADMISSION_GROUP):
        if message.startswith(REJECTION_PREFIX[verdict]):
            return verdict
    raise ContractError("拒绝消息不属于已冻结类别: " + message[:160])


def direct_for_consumer(consumer):
    """返回所有以该 crate 为直接消费方的候选身份。"""
    return tuple(key for key, item in DIRECT.items() if item["consumer"] == consumer)


def new_package_keys(group):
    """返回该组相对基线的新增包身份集合。"""
    return frozenset(GROUPS[group]["new_packages"])


def license_atoms(expression, legacy_slash_means_or=True):
    """把 SPDX 表达式拆成原子集合；只用于自测的覆盖性检查。

    正式许可证判定仍由 `dependency_admission.license_review()` 执行；这里只
    用于确认冻结表没有漏登记一个候选，不复制门禁的判定语义。
    """
    text = expression.strip()
    if legacy_slash_means_or and "/" in text and "WITH" not in text:
        text = " OR ".join(part.strip() for part in text.split("/"))
    return frozenset(
        atom.strip() for atom in re.split(r"\s+(?:AND|OR|WITH)\s+|[()]", text)
        if atom.strip()
    )
