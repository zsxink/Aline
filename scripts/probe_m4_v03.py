#!/usr/bin/env python3
"""V-03 生产支持矩阵探针：用样本与锁定 GPUI 实测冻结支持清单。

用法：

    python3 scripts/probe_m4_v03.py --self-test
    python3 scripts/probe_m4_v03.py --output-dir logs/validation/...

`--self-test` 只校验规则层与产物比对逻辑，不编译 Rust、不起应用、不碰剪贴板，
因此在没有 GPUI 缓存、没有 GUI 会话的环境里也能运行。

真实运行会：

1. 把两个探针 crate（`probe-gpui`、`probe-content`）复制到临时目录，用
   `--locked --offline` 构建。两者都不是 Aline 八成员。
2. 用 `scripts/m4_v03_rules.py` 的契约生成样本矩阵：图片格式、SVG 安全边界、
   公式语法、Mermaid 图类型、静态主题。
3. 跑探针量真实结果，逐项与契约比对；失败样例保留源码。
4. 走系统剪贴板读/写一遍，核对读取优先级与写回行为。
5. 对两个探针的依赖闭包跑仓库自己的许可证与禁库判定。

证据只写 `logs/validation/` 内；脚本不改工作区依赖、不写工作区 target、
不下载任何东西。
"""

import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))

import m4_v03_rules as rules  # noqa: E402
from dependency_admission import license_review  # noqa: E402
from engineering_dependencies import Violation, forbidden_name, package_key  # noqa: E402
from evidence_paths import display_path, resolve_path, write_json, write_text  # noqa: E402
from validation_output import ROOT, output_directory  # noqa: E402

FIXTURES = ROOT / "scripts/fixtures/m4-v03"
SAMPLES = FIXTURES / "samples"
PROBE_GPUI = FIXTURES / "probe-gpui"
PROBE_CONTENT = FIXTURES / "probe-content"

# 两个探针的二进制名，与各自 Cargo.toml 的 package name 一致。
GPUI_BINARY = "m4-v03-gpui"
CONTENT_BINARY = "m4-v03-content"


class ProbeError(RuntimeError):
    """探针失败；调用方据此非零退出并保留证据。"""


# ------------------------------------------------------------------ 契约矩阵


def image_cells():
    """图片格式 × 路径矩阵。每个样本走文档路径与剪贴板路径各一次。"""
    cells = []
    for entry in rules.IMAGE_FORMATS:
        fmt = entry["format"]
        extension = entry["extensions"][0]
        animated = fmt in ("gif", "webp")
        static_sample = SAMPLES / ("plain." + extension)
        cells.append({
            "format": fmt,
            "sample": "plain." + extension,
            "path": "document",
            "expected_decoded": True,
            "expected_frames": 1,
            "expectation": "静态样本解码为 1 帧，尺寸与源一致",
            "source": static_sample,
        })
        if entry["animated"]:
            cells.append({
                "format": fmt,
                "sample": "anim." + extension,
                "path": "document",
                "expected_decoded": True,
                "expected_frames": 3,
                "expectation": "动画样本在文档路径逐帧解码（3 帧）",
                "source": SAMPLES / ("anim." + extension),
            })
            cells.append({
                "format": fmt,
                "sample": "anim." + extension,
                "path": "clipboard",
                "expected_decoded": True,
                # 剪贴板路径对 WebP 恒为静态；GIF 仍逐帧。这正是“路径分裂”。
                "expected_frames": 3 if fmt == "gif" else rules.CLIPBOARD_WEBP_ANIMATED_FRAMES,
                "expectation": ("剪贴板路径逐帧解码" if fmt == "gif"
                                else "剪贴板路径恒静态（与文档路径不同）"),
                "source": SAMPLES / ("anim." + extension),
            })
    # APNG：PNG 的超集，锁定实现只取首帧，作为“已知限制”单列。
    cells.append({
        "format": "png",
        "sample": "anim.png",
        "path": "document",
        "expected_decoded": True,
        "expected_frames": 1,
        "expectation": "APNG 只取首帧（已知限制，非缺陷）",
        "source": SAMPLES / "anim.png",
    })
    return cells


def svg_cases(work):
    """生成 SVG 安全样本，写到临时目录；每个样本引用同一份目标字节。

    目标文件是纯绿 4×4 PNG。若 SVG 读到了它，渲染结果的像素会变绿；
    若被阻断，像素保持透明或仅为基线图形。这样“读没读到文件”是像素事实，
    不是日志推断。
    """
    secret = work / "escape"
    (secret / "sub").mkdir(parents=True, exist_ok=True)
    # 生成纯绿样本：直接写字节，避免依赖 PIL。
    green_bytes = _solid_png(4, 4, (0, 255, 0, 255))
    for target in (secret / "secret.png", secret / "sub" / "secret.png"):
        target.write_bytes(green_bytes)
    absolute = str((secret / "secret.png").resolve())

    svg = '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image xlink:href="%s" width="4" height="4"/></svg>'
    baseline = '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#ff0000"/></svg>'

    cases = []
    # 相对路径按**进程 cwd** 解析，因此每个 case 带自己的 cwd：探针在该目录下
    # 启动，`secret.png` 与 `../secret.png` 才会真的指向预先写好的绿色样本。
    # 若 cwd 给错，样本根本不存在，“读到文件”这件事就无从发生——那样测出的
    # 透明是假阴性，不是安全结论。
    inner = secret / "sub"
    for case, href, cwd, expectation in (
        ("relative-cwd", "secret.png", inner, "锁定 GPUI 会读取 cwd 下的文件"),
        ("relative-dotdot", "../secret.png", inner, "锁定 GPUI 会穿越 ../ 读取"),
        ("absolute", absolute, work, "锁定 GPUI 会读取绝对路径"),
        ("file-uri", "file://" + absolute, work, "file: URI 不被默认 resolver 当作路径"),
        ("data-uri", "data:image/png;base64," + _base64(green_bytes), work,
         "内联 data: 被接受"),
    ):
        path = work / (case + ".svg")
        path.write_text(svg % href)
        cases.append({"case": case, "file": path, "href_kind": case, "cwd": cwd,
                      "expectation": expectation})
    base = work / "baseline.svg"
    base.write_text(baseline)
    # 4×4 的 SVG 按 SMOOTH_SVG_SCALE_FACTOR=2 渲染成 8×8，故不透明像素应为 64。
    cases.append({"case": "baseline", "file": base, "href_kind": "none", "cwd": work,
                  "expect_pixels": 64, "expectation": "基线矩形正常渲染"})
    missing = work / "missing.svg"
    missing.write_text(svg % "nope.png")
    cases.append({"case": "missing", "file": missing, "href_kind": "missing", "cwd": work,
                  "expectation": "引用不存在的文件时透明且不报错"})

    # 脚本/foreignObject/http/实体：期望“无额外效果”，既不加内容也不崩溃。
    # 前三者都带一个红色矩形，故应恰好 64 个不透明像素；http-href 只有一张远程
    # 图片，若真的抓取会变绿，若不抓取应保持全透明。
    neutral = {
        "script": ('<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">'
                   '<script>alert(1)</script><rect width="4" height="4" fill="#ff0000"/></svg>', 64),
        "foreign-object": ('<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">'
                           '<foreignObject width="4" height="4"><div xmlns="http://www.w3.org/1999/xhtml">x</div>'
                           '</foreignObject><rect width="4" height="4" fill="#ff0000"/></svg>', 64),
        "http-href": (svg % "https://example.invalid/x.png", 0),
        "entity": ('<!DOCTYPE svg [<!ENTITY e "x">]>'
                   '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">'
                   '<rect width="4" height="4" fill="#ff0000"/></svg>', 64),
    }
    for case, (text, expect_pixels) in neutral.items():
        path = work / ("neutral-" + case + ".svg")
        path.write_text(text)
        cases.append({"case": "neutral-" + case, "file": path, "href_kind": "none",
                      "cwd": work, "expect_pixels": expect_pixels,
                      "expectation": "无额外内容、无崩溃、无执行"})
    return cases


def _solid_png(width, height, rgba):
    """生成确定性的纯色 PNG，不依赖 PIL。"""
    import struct
    import zlib

    def chunk(tag, payload):
        body = tag + payload
        return (struct.pack(">I", len(payload)) + body
                + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    row = b"\x00" + bytes(rgba) * width
    raw = row * height
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def _base64(data):
    import base64
    return base64.b64encode(data).decode("ascii")


def math_cases():
    """公式语法矩阵：应成功与应失败分开，失败必须带诊断。"""
    cases = []
    for case, source, expect_ok in rules.MATH_SYNTAX_CASES:
        cases.append({"case": case, "source": source, "expect_ok": expect_ok})
    for case, source in rules.MATH_ERROR_CASES:
        cases.append({"case": case, "source": source, "expect_ok": False})
    return cases


def theme_cases(work):
    """静态主题导入矩阵：合法、未知字段、注入串、缺字段、超限各一份。"""
    template = rules.mermaid_theme_template()
    cases = []

    def write(name, payload):
        path = work / (name + ".json")
        path.write_text(json.dumps(payload, ensure_ascii=False))
        return path

    cases.append({"case": "valid", "file": write("theme-valid", template),
                  "expect_import": True, "note": "契约模板本身必须通过校验"})

    unknown = dict(template, unexpected_field="x")
    cases.append({"case": "unknown-field", "file": write("theme-unknown", unknown),
                  "expect_import": False, "note": "封闭字段：未知字段拒绝"})

    injected = dict(template, background='</style><script>alert(1)</script>')
    cases.append({"case": "script-injection",
                  "file": write("theme-script", injected),
                  "expect_import": False, "note": "颜色字段含脚本必须拒绝"})

    quote_break = dict(template, primary_color='#fff" onload="alert(1)')
    cases.append({"case": "attribute-break",
                  "file": write("theme-quote", quote_break),
                  "expect_import": False, "note": "颜色字段含引号属性注入必须拒绝"})

    url_ref = dict(template, primary_color="url(javascript:alert(1))")
    cases.append({"case": "external-url", "file": write("theme-url", url_ref),
                  "expect_import": False, "note": "颜色字段含 url() 必须拒绝"})

    missing = dict(template)
    del missing["font_family"]
    cases.append({"case": "missing-field", "file": write("theme-missing", missing),
                  "expect_import": False, "note": "缺字段拒绝"})

    oversized = dict(template, pie_colors=["#ECECFF"] * 13)
    cases.append({"case": "oversized-array", "file": write("theme-oversized", oversized),
                  "expect_import": False, "note": "数组长度必须精确匹配"})
    return cases


# ------------------------------------------------------------------ 探针调用


def run(argv, cwd=None, env=None, timeout=900):
    """运行命令并保留完整输出；失败抛出带 stderr 的异常。"""
    completed = subprocess.run(argv, cwd=cwd, env=env, text=True,
                               capture_output=True, timeout=timeout)
    if completed.returncode != 0:
        raise ProbeError("命令失败(%d): %s\nstdout:\n%s\nstderr:\n%s"
                         % (completed.returncode, " ".join(argv),
                            completed.stdout[-4000:], completed.stderr[-4000:]))
    return completed


def parse_json_lines(text):
    """探针每行输出一个 JSON 对象；跳过非 JSON 行（GPUI 可能打印日志）。"""
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            rows.append(json.loads(line))
        except ValueError:
            continue
    return rows


def build_probe(source, name, work, output, log_name):
    """复制到临时目录并离线构建；不写工作区 target，不改工作区依赖。"""
    crate = work / name
    shutil.copytree(source, crate, ignore=shutil.ignore_patterns("target"))
    # 夹具里的 `[patch.crates-io] gpui-pre` 是相对路径；复制到临时目录后
    # 相对路径会指向不存在的位置，改写成仓库内 vendor 的绝对路径。
    manifest = crate / "Cargo.toml"
    text = manifest.read_text()
    vendor = (ROOT / "vendor/gpui-pre-0.3.9").resolve()
    text = text.replace('path = "../../../../vendor/gpui-pre-0.3.9"',
                        'path = "%s"' % vendor)
    manifest.write_text(text)
    target = work / ("target-" + name)
    env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(target))
    build = run(["cargo", "build", "--locked", "--offline", "--manifest-path",
                 str(manifest)], env=env)
    write_text(output / (log_name + "-build.log"), build.stdout + build.stderr)
    binary = target / "debug" / name
    if not binary.is_file():
        raise ProbeError("探针二进制未生成: " + str(binary))
    return binary, env


def dependency_facts(crate, env):
    """用仓库自己的准入策略判候选闭包；结论不构成正式准入（任务 1.5）。

    许可证判定逐包调用 `dependency_admission.license_review`，禁库判定调用
    `engineering_dependencies.forbidden_name`——两者都是仓库守卫正在用的同一份
    代码。策略里的 `exact_exceptions`（cbindgen 等 MPL-2.0 精确例外）按包身份
    传入并核对校验和，避免把“已批准的精确例外”误报成未准入；但本函数**不**
    复核例外的入边/分组，那属正式门禁（6.1）的职责。
    """
    policy = json.loads((ROOT / "scripts/dependency_admission_policy.json").read_text())
    license_policy = policy["license"]
    exceptions = {entry["package"]: entry
                  for entry in license_policy.get("exact_exceptions", [])}
    completed = run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1",
                     "--manifest-path", str(crate / "Cargo.toml")], env=env)
    metadata = json.loads(completed.stdout)
    roots = set(metadata["workspace_members"])
    lock = {}
    for block in (crate / "Cargo.lock").read_text().split("[[package]]")[1:]:
        entry = {}
        for line in block.splitlines():
            match = re.match(r'(name|version|source|checksum) = "(.*)"', line)
            if match:
                entry[match[1]] = match[2]
        if {"name", "version"} <= set(entry):
            lock[entry["name"] + "@" + entry["version"]] = entry
    rows = []
    for package in sorted(metadata["packages"], key=package_key):
        key = package_key(package)
        exception = exceptions.get(key)
        if exception is not None:
            # 例外只对登记过的来源与校验和成立；漂移即视为未准入。
            if (package.get("source") != exception["source"]
                    or lock.get(key, {}).get("checksum") != exception["checksum"]):
                exception = None
        review = license_review(package, license_policy, package["id"] in roots,
                                exception)
        rows.append({
            "package": key,
            "workspace": package["id"] in roots,
            "license": review["raw"],
            "verdict": review["verdict"],
            "forbidden": forbidden_name(package["name"]),
            "source": package.get("source"),
            "checksum": lock.get(key, {}).get("checksum"),
        })
    forbidden = [row["package"] for row in rows if row["forbidden"]]
    denied = [row["package"] for row in rows
              if row["verdict"] not in ("allowed", "workspace-exempt",
                                        "allowed-exact-exception")]
    return {
        "packages": len(rows),
        "forbidden": forbidden,
        "forbidden_incoming": _incoming_edges(metadata, set(forbidden)),
        # 探针独立解析锁文件，小版本会与工作区不同；有些禁库在工作区里是靠
        # 登记的**精确版本例外**放行的（如 regex@1.13.1）。把工作区实际版本记
        # 下来，避免把“探针解析到 regex@1.13.2”误读成“新增禁库”。
        "workspace_versions": _workspace_versions(set(name.split("@")[0]
                                                      for name in forbidden)),
        "license_denied": denied,
        "max_rust_version": _max_rust_version(metadata),
        "rows": rows,
        "limits": "探针 crate 不是 Aline 成员；本结果只说明候选闭包能否被现有策略接受，"
                  "不构成正式准入，也不覆盖跨平台原生构建；精确例外只核对了来源与"
                  "校验和，未复核入边与分组（属正式门禁职责）。",
    }


def _workspace_versions(names, text=None):
    """工作区 Cargo.lock 里同名包的版本，用于区分“探针新解析”与“真实新增”。"""
    found = {}
    if text is None:
        text = (ROOT / "Cargo.lock").read_text()
    for block in text.split("[[package]]")[1:]:
        match_name = re.search(r'^name = "(.*)"', block, re.M)
        match_version = re.search(r'^version = "(.*)"', block, re.M)
        if match_name and match_version and match_name.group(1) in names:
            found.setdefault(match_name.group(1), []).append(match_version.group(1))
    return found


def _incoming_edges(metadata, forbidden_keys):
    """记录禁库是**谁**拉进来的：只报“命中 regex”不够，1.5 要按入边裁决。

    入边只能从 `resolve.nodes[].deps` 读：`packages[].dependencies` 只有版本
    **要求**（`req`）没有解析后的版本，拿它拼 `name@version` 会 KeyError。
    """
    names = {package["id"]: package_key(package) for package in metadata["packages"]}
    edges = []
    for node in metadata["resolve"]["nodes"]:
        source = names[node["id"]]
        if source in forbidden_keys:
            # 禁库之间的传递边（regex → regex-automata）不回答“谁拉进来的”，
            # 只留入口边，1.5 才能对着真实入边裁决。
            continue
        for dependency in node["deps"]:
            key = names.get(dependency["pkg"])
            if key in forbidden_keys:
                kinds = dependency.get("dep_kinds") or [{}]
                edges.append({"from": source, "to": key,
                              "kind": kinds[0].get("kind") or "normal",
                              "target": kinds[0].get("target")})
    return sorted(edges, key=lambda edge: (edge["to"], edge["from"]))


def _max_rust_version(metadata):
    """闭包里声明的最高 `rust-version`。

    用来回答“这个候选需要多新的编译器”，而不是凭印象写一个门槛——工具链
    要求会随依赖漂移，只有从元数据读出来才可复跑。
    """
    versions = [package["rust_version"] for package in metadata["packages"]
                if package.get("rust_version")]
    return max(versions, key=_version_key) if versions else None


def _version_key(text):
    return tuple(int(part) for part in re.findall(r"\d+", text))


# ------------------------------------------------------------------ 结果比对


def check_document_images(cells, rows):
    """把探针的文档路径结果与契约逐格比对。"""
    by_file = {row["file"]: row.get("result") or {} for row in rows}
    results = []
    for cell in cells:
        if cell["path"] != "document":
            continue
        actual = by_file.get(str(cell["source"].resolve())) or by_file.get(str(cell["source"]))
        if actual is None:
            results.append({"cell": cell["sample"], "ok": False,
                            "detail": "探针未返回该样本"})
            continue
        if "error" in actual:
            results.append({"cell": cell["sample"], "ok": False,
                            "detail": actual["error"]})
            continue
        frames = actual.get("frames")
        ok = frames == cell["expected_frames"]
        results.append({
            "cell": cell["sample"],
            "format": cell["format"],
            "ok": ok,
            "frames": frames,
            "expected_frames": cell["expected_frames"],
            "width": actual.get("width"),
            "height": actual.get("height"),
            "delay_numer_ms": actual.get("delay_numer_ms"),
            "first_pixel_bgra": actual.get("first_pixel_bgra"),
            "detail": "" if ok else "帧数不符",
        })
    return results


def check_clipboard_images(cells, clip_rows, clip_facts):
    """剪贴板路径：由驱动写入样本再读回，核对帧数与路由。"""
    results = []
    for cell in cells:
        if cell["path"] != "clipboard":
            continue
        facts = clip_facts.get(cell["sample"])
        if facts is None:
            results.append({"cell": cell["sample"], "ok": False, "detail": "未写回剪贴板"})
            continue
        decoded = facts.get("decoded") or {}
        frames = decoded.get("frames")
        ok = frames == cell["expected_frames"]
        results.append({
            "cell": cell["sample"],
            "format": cell["format"],
            "ok": ok,
            "frames": frames,
            "expected_frames": cell["expected_frames"],
            "detail": "" if ok else "帧数不符",
        })
    return results


def _group_by_cwd(cases):
    """把 SVG 样本按 cwd 分组，保持首次出现顺序（纯函数，可自测）。"""
    groups = {}
    for case in cases:
        groups.setdefault(str(case["cwd"]), []).append(case)
    return list(groups.items())


def _run_svg_grouped(gpui_binary, subcommand, cases):
    """按 cwd 分组运行 SVG 探针。

    相对 href 的解析基准是**进程 cwd**，而 `subprocess` 的 cwd 是整次调用共享的；
    把不同 cwd 的样本塞进同一次调用，相对路径样本必然落到错的目录。分组后每次
    调用只带同一 cwd 的样本，结果行再合并。
    """
    rows = []
    for cwd, group in _group_by_cwd(cases):
        rows.extend(parse_json_lines(
            run([str(gpui_binary), subcommand] + [str(c["file"]) for c in group],
                cwd=cwd).stdout))
    return rows


def check_svg(cases, default_rows, deny_rows):
    """SVG 安全：默认 resolver 的读取事实 + 自定义 resolver 的阻断结果。"""
    default_by_file = {row["file"]: row.get("result") or {} for row in default_rows}
    deny_by_file = {row["file"]: row.get("result") or {} for row in deny_rows}
    results = []
    for case in cases:
        key = str(case["file"].resolve())
        default = default_by_file.get(key) or default_by_file.get(str(case["file"])) or {}
        deny = deny_by_file.get(key) or deny_by_file.get(str(case["file"])) or {}
        kind = case["href_kind"]
        if kind in ("relative-cwd", "relative-dotdot", "absolute"):
            # 期望：默认 resolver 读到文件（首像素绿），自定义 resolver 阻断。
            read = _is_green(default.get("first_pixel_bgra"))
            blocked = deny.get("image_nodes") == 0
            results.append({
                "case": case["case"],
                "default_read": read,
                "deny_blocked": blocked,
                "ok": read and blocked,
                "detail": "" if (read and blocked) else "默认未读到或阻断未生效",
            })
        elif kind == "file-uri":
            # file: URI 不是路径，默认 resolver 不读；自定义 resolver 同样阻断。
            read = _is_green(default.get("first_pixel_bgra"))
            results.append({
                "case": case["case"],
                "default_read": read,
                "deny_blocked": deny.get("image_nodes") == 0,
                "ok": not read,
                "detail": "" if not read else "file: URI 被当作路径读取",
            })
        elif kind == "data-uri":
            read = _is_green(default.get("first_pixel_bgra"))
            allowed = deny.get("image_nodes") == 1
            results.append({
                "case": case["case"],
                "default_read": read,
                "deny_allowed": allowed,
                "ok": read and allowed,
                "detail": "" if (read and allowed) else "data: 未被接受",
            })
        elif kind == "missing":
            transparent = default.get("first_pixel_bgra") == "00000000"
            results.append({
                "case": case["case"],
                "default_transparent": transparent,
                "ok": transparent,
                "detail": "" if transparent else "缺文件时不是透明",
            })
        else:
            # baseline 与 neutral：必须渲染成功、不报错，且不透明像素数恰好等于
            # 预期——脚本/foreignObject 不能多画，远程图片不能真的被抓取。
            expect_pixels = case["expect_pixels"]
            rendered = "error" not in default
            pixels = default.get("pixels")
            results.append({
                "case": case["case"],
                "rendered": rendered,
                "pixels": pixels,
                "expected_pixels": expect_pixels,
                "ok": rendered and pixels == expect_pixels,
                "detail": default.get("error", "") if not rendered else
                          ("" if pixels == expect_pixels
                           else "不透明像素 %r，预期 %r" % (pixels, expect_pixels)),
            })
    return results


def _is_green(pixel):
    """首像素是否为纯绿（BGRA）。"""
    return pixel == "00ff00ff"


def check_math(cases, rows):
    by_case = {row["case"]: row for row in rows}
    results = []
    for case in cases:
        actual = by_case.get(case["case"])
        if actual is None:
            results.append({"case": case["case"], "ok": False, "detail": "探针未返回"})
            continue
        ok = bool(actual.get("ok")) == case["expect_ok"]
        detail = ""
        if case["case"] == "cjk-text" and not actual.get("ok"):
            # 已知边界：STIX Two Math 无 CJK 字形；诊断应提到缺字形。
            hint = rules.MATH_CJK_DIAGNOSTIC_HINT
            detail = ("诊断未提到缺字形" if hint not in str(actual.get("diagnostic", "")).lower()
                      else "已知边界：字体无 CJK 字形，源码保留")
        elif not ok:
            detail = "与契约不符"
        results.append({
            "case": case["case"],
            "ok": ok,
            "expect_ok": case["expect_ok"],
            "actual_ok": bool(actual.get("ok")),
            "svg_bytes": actual.get("svg_bytes"),
            "diagnostic": actual.get("diagnostic"),
            "detail": detail,
        })
    return results


def check_mermaid(rows):
    by_type = {row["type"]: row for row in rows}
    results = []
    for kind in rules.MERMAID_DIAGRAM_TYPES:
        actual = by_type.get(kind)
        if actual is None:
            results.append({"type": kind, "ok": False, "detail": "探针未返回"})
            continue
        ok = bool(actual.get("ok")) and actual.get("has_text")
        results.append({
            "type": kind,
            "ok": ok,
            "svg_bytes": actual.get("svg_bytes"),
            "has_text": actual.get("has_text"),
            "diagnostic": actual.get("diagnostic"),
            "detail": "" if ok else "未产出含文字的 SVG",
        })
    return results


def check_themes(cases, rows):
    by_file = {row["file"]: row for row in rows}
    results = []
    for case in cases:
        key = str(case["file"].resolve())
        actual = by_file.get(key) or by_file.get(str(case["file"])) or {}
        # 我们自己的校验契约
        payload = json.loads(case["file"].read_text())
        violations = rules.mermaid_theme_violations(payload)
        ours_ok = not violations
        # 候选后端是否接受（用于证明后端不校验）
        backend_accepted = bool(actual.get("accepted"))
        results.append({
            "case": case["case"],
            "expect_import": case["expect_import"],
            "rules_ok": ours_ok,
            "rules_violations": violations,
            "backend_accepted": backend_accepted,
            "backend_has_script": bool(actual.get("svg_has_script")),
            "backend_has_onload": bool(actual.get("svg_has_onload")),
            "ok": ours_ok == case["expect_import"],
            "detail": case["note"],
        })
    return results


# ------------------------------------------------------------------ 主流程


def environment_facts(lock_hashes):
    isolation = json.loads((ROOT / "scripts/fixtures/isolation.json").read_text())
    registered = resolve_path(isolation["cargo_home"])
    actual = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    exists = registered is not None and Path(registered).is_dir()
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "probe_lock_sha256": lock_hashes,
        "cargo_home": display_path(actual),
        "registered_cargo_home": isolation["cargo_home"],
        "registered_cargo_home_exists": exists,
        "cargo_home_note": ("使用登记隔离缓存" if exists else
                            "登记的隔离缓存不存在；本次以宿主缓存 + CARGO_NET_OFFLINE=true "
                            "构建，证据不能反推实际缓存，属入口环境缺陷而非本变更引入"),
    }


def real_run(output):
    if sys.platform != "darwin":
        raise ProbeError("V-03 探针当前只批准 macOS（锁定 GPUI 与剪贴板）")
    work = Path(tempfile.mkdtemp(prefix="aline-m4-v03-"))
    artifacts = output / "artifacts"
    if artifacts.exists():
        shutil.rmtree(artifacts)
    artifacts.mkdir()
    try:
        return _real_run(output, artifacts, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def _real_run(output, artifacts, work):
    gpui_binary, gpui_env = build_probe(PROBE_GPUI, GPUI_BINARY, work, output, "gpui")
    content_binary, content_env = build_probe(PROBE_CONTENT, CONTENT_BINARY, work, output,
                                              "content")
    dependencies = {
        "gpui": dependency_facts(work / GPUI_BINARY, gpui_env),
        "content": dependency_facts(work / CONTENT_BINARY, content_env),
    }
    write_json(output / "dependencies.json", dependencies)

    # ---- 图片：文档路径
    cells = image_cells()
    document_files = [str(cell["source"].resolve()) for cell in cells
                      if cell["path"] == "document"]
    document_rows = parse_json_lines(
        run([str(gpui_binary), "images"] + document_files).stdout)
    write_text(output / "images-document.log", json.dumps(document_rows, ensure_ascii=False,
                                                          indent=2))
    document_results = check_document_images(cells, document_rows)

    # ---- 图片：剪贴板路径（写入再读回）
    clipboard_cells = [cell for cell in cells if cell["path"] == "clipboard"]
    clip_facts = {}
    clip_routes = []
    for cell in clipboard_cells:
        sample = cell["sample"]
        if sample.endswith(".gif"):
            # 必须声明正确的 UTI：GPUI 不嗅探魔数，会按声明格式解码。
            run([str(gpui_binary), "clipboard-write-image",
                 str(cell["source"].resolve()), cell["format"]])
        else:
            # WebP 无法用 ClipboardEntry::Image 写出（write_image 只写 PNG/TIFF 等），
            # 因此用系统剪贴板写入原始 flavor，再让探针读。
            _write_flavor(sample, cell["source"], work)
        read = run([str(gpui_binary), "clipboard"])
        row = parse_json_lines(read.stdout)
        facts = _first_clipboard_image(row)
        clip_facts[sample] = facts
        clip_routes.append({"sample": sample, "route": row[0].get("route") if row else None,
                            "facts": facts})
    clipboard_results = check_clipboard_images(cells, [], clip_facts)
    write_text(output / "clipboard.log", json.dumps(clip_routes, ensure_ascii=False, indent=2))

    # ---- 剪贴板 flavor 路由矩阵（纯文本/HTML/RTF/文件/图片）
    # `png` 必须带真实字节：不带文件的 png flavor 会写入 0 字节，读回时必然
    # 解码失败——那测的是“空数据”，不是“图片路由”。
    flavor_rows = []
    for case in rules.CLIPBOARD_FLAVOR_CASES:
        flavor = case["flavor"]
        if flavor == "gif-webp":
            _set_flavor(flavor, work, SAMPLES / "anim.gif", SAMPLES / "plain.webp")
        else:
            _set_flavor(flavor, work, SAMPLES / "plain.png")
        read = run([str(gpui_binary), "clipboard"])
        rows = parse_json_lines(read.stdout)
        # 探针只区分“命中/未命中”，语义路由由规则层从条目推导，避免两边各写
        # 一套优先级判断而互相印证不了。
        entries = rows[0].get("entries") if rows and rows[0].get("route") == "hit" else None
        route = rules.clipboard_route_from_entries(entries)
        ok = route == case["route"]
        if ok and case["kind"] is not None:
            first = (entries or [{}])[0]
            ok = first.get("kind") == case["kind"]
            if ok and "format" in case:
                ok = first.get("format") == case["format"]
        flavor_rows.append({"flavor": flavor, "route": route, "entries": entries,
                            "expected_route": case["route"],
                            "expected_kind": case["kind"],
                            "note": case["note"], "ok": ok})
    write_text(output / "clipboard-flavors.log",
               json.dumps(flavor_rows, ensure_ascii=False, indent=2))

    # ---- 剪贴板写入矩阵（单文本/单图片/混合）
    # 写和读必须用**同一个进程外**的系统剪贴板：写入进程退出后再读，才证明
    # 数据真的落到了系统剪贴板，而不是留在进程内存里。
    write_rows = []
    for case in rules.CLIPBOARD_WRITE_CASES:
        if case["case"] == "single-text":
            run([str(gpui_binary), "clipboard-write-text", case["expect_text"]])
        elif case["case"] == "single-image":
            run([str(gpui_binary), "clipboard-write-image",
                 str(SAMPLES / "plain.png"), "png"])
        else:
            run([str(gpui_binary), "clipboard-write-mixed",
                 case["expect_text"], str(SAMPLES / "plain.png")])
        rows = parse_json_lines(run([str(gpui_binary), "clipboard"]).stdout)
        entries = rows[0].get("entries") if rows and rows[0].get("route") == "hit" else None
        kinds = [entry.get("kind") for entry in (entries or [])]
        ok = kinds == case["expect_entries"]
        detail = "" if ok else "条目 %r，期望 %r" % (kinds, case["expect_entries"])
        if ok and "expect_text" in case:
            text = (entries or [{}])[0].get("text")
            ok = text == case["expect_text"]
            detail = "" if ok else "文本 %r，期望 %r" % (text, case["expect_text"])
        if ok and "expect_format" in case:
            actual_format = (entries or [{}])[0].get("format")
            ok = actual_format == case["expect_format"]
            detail = "" if ok else "格式 %r，期望 %r" % (actual_format, case["expect_format"])
        write_rows.append({"case": case["case"], "note": case["note"],
                           "entries": entries, "expected_entries": case["expect_entries"],
                           "ok": ok, "detail": detail})
    write_text(output / "clipboard-write.log",
               json.dumps(write_rows, ensure_ascii=False, indent=2))

    # ---- SVG 安全
    cases = svg_cases(work)
    default_rows = _run_svg_grouped(gpui_binary, "svg-default", cases)
    deny_rows = _run_svg_grouped(gpui_binary, "svg-deny", cases)
    write_text(output / "svg-default.log",
               json.dumps(default_rows, ensure_ascii=False, indent=2))
    write_text(output / "svg-deny.log", json.dumps(deny_rows, ensure_ascii=False, indent=2))
    svg_results = check_svg(cases, default_rows, deny_rows)
    # 保留失败/安全样本源码：证据必须能复现“读到文件”这件事。
    sample_dir = artifacts / "svg-samples"
    sample_dir.mkdir()
    for case in cases:
        shutil.copyfile(case["file"], sample_dir / case["file"].name)
    shutil.copyfile(work / "escape/secret.png", sample_dir / "secret-target.png")

    # ---- 公式
    math = math_cases()
    env = dict(os.environ)
    for case in math:
        env["M4V03_TEX_" + case["case"]] = case["source"]
    math_rows = parse_json_lines(
        run([str(content_binary), "math"] + [c["case"] for c in math], env=env).stdout)
    write_text(output / "math.log", json.dumps(math_rows, ensure_ascii=False, indent=2))
    math_results = check_math(math, math_rows)

    # ---- Mermaid
    mermaid_rows = parse_json_lines(
        run([str(content_binary), "mermaid"] + list(rules.MERMAID_DIAGRAM_TYPES)).stdout)
    write_text(output / "mermaid.log", json.dumps(mermaid_rows, ensure_ascii=False, indent=2))
    mermaid_results = check_mermaid(mermaid_rows)

    # ---- 主题
    themes = theme_cases(work)
    theme_rows = parse_json_lines(
        run([str(content_binary), "theme"] + [str(c["file"]) for c in themes]).stdout)
    write_text(output / "theme.log", json.dumps(theme_rows, ensure_ascii=False, indent=2))
    theme_results = check_themes(themes, theme_rows)
    theme_dir = artifacts / "themes"
    theme_dir.mkdir()
    for case in themes:
        shutil.copyfile(case["file"], theme_dir / case["file"].name)

    # 契约与实测必须一致：期望值已把已知限制写进去了（APNG 只取首帧、剪贴板
    # WebP 恒静态、CJK 公式失败），所以任何一行不符都是**真实分歧**，不能带着
    # 它去写“通过”的证据。
    failures = []
    for group, rows in (("图片-文档", document_results), ("图片-剪贴板", clipboard_results),
                        ("SVG", svg_results), ("剪贴板 flavor", flavor_rows),
                        ("剪贴板写入", write_rows),
                        ("公式", math_results), ("Mermaid", mermaid_results),
                        ("主题", theme_results)):
        for row in rows:
            if not row.get("ok"):
                failures.append("%s: %s %s" % (group, row.get("cell") or row.get("case")
                                               or row.get("type") or row.get("flavor"),
                                               row.get("detail") or "不符"))
    if failures:
        raise ProbeError("契约与实测不符 %d 项：\n%s"
                         % (len(failures), "\n".join(failures)))

    summary = {
        "check": "v03-support-matrix",
        "scope": "macOS arm64；锁定 GPUI 0.3.9 + 候选内容后端（未准入）",
        "environment": environment_facts({
            GPUI_BINARY: _sha256(work / GPUI_BINARY / "Cargo.lock"),
            CONTENT_BINARY: _sha256(work / CONTENT_BINARY / "Cargo.lock"),
        }),
        # 样本指纹：结论挂在这批字节上，样本换了结论就不自动成立。
        "samples": {path.name: {"bytes": path.stat().st_size, "sha256": _sha256(path)}
                    for path in sorted(SAMPLES.iterdir()) if path.is_file()},
        "images": {
            "formats": [{"format": entry["format"], "extensions": list(entry["extensions"]),
                         "animated": entry["animated"], "note": entry["note"]}
                        for entry in rules.IMAGE_FORMATS],
            "document": document_results,
            "clipboard": clipboard_results,
            "svg_scale_factor": rules.SMOOTH_SVG_SCALE_FACTOR,
            "pixel_order": rules.DOCUMENT_PIXEL_ORDER,
        },
        "clipboard": {
            "read_order": [{"priority": index + 1, "kind": kind, "detail": detail}
                           for index, (kind, detail) in enumerate(rules.CLIPBOARD_READ_ORDER)],
            "write_rules": rules.CLIPBOARD_WRITE_RULES,
            "write_cases": write_rows,
            "flavors": flavor_rows,
        },
        "svg": {
            "policy": rules.SVG_POLICY,
            "cases": svg_results,
            "artifacts": "artifacts/svg-samples",
        },
        "links": {
            "allowed_schemes": list(rules.ALLOWED_LINK_SCHEMES),
            "allowed_prefixes": list(rules.ALLOWED_LINK_PREFIXES),
            "rejected_schemes": list(rules.REJECTED_LINK_SCHEMES),
            "verdicts": [{"href": href, "allowed": rules.link_verdict(href)[0],
                          "reason": rules.link_verdict(href)[1]}
                         for href in ("https://example.com", "http://example.com",
                                      "mailto:a@b", "#anchor", "/docs",
                                      "javascript:alert(1)", "file:///etc/passwd",
                                      "notes/a.md", "")],
            "limits": "allowlist 来自 aline-markdown 的 is_allowed_link（镜像在本层，"
                      "真实判定在 Rust 侧并有 tests/html.rs 覆盖）；点击打开与拒绝提示"
                      "的生产路径尚未实现，本轮无法取证。",
        },
        "math": {"cases": math_results},
        "mermaid": {
            "diagram_types": mermaid_results,
            "appearance_map": rules.MERMAID_APPEARANCE_MAP,
            "theme_fields": list(rules.MERMAID_THEME_FIELDS),
            "themes": theme_results,
            "artifacts": "artifacts/themes",
        },
        "dependencies": {
            name: {"packages": facts["packages"], "forbidden": facts["forbidden"],
                   "forbidden_incoming": facts["forbidden_incoming"],
                   "workspace_versions": facts["workspace_versions"],
                   "license_denied": facts["license_denied"],
                   "max_rust_version": facts["max_rust_version"],
                   "limits": facts["limits"]}
            for name, facts in dependencies.items()
        },
        "untested": [
            "链接点击打开与拒绝提示的生产路径（IMAGE-03 未实现，本轮只能冻结 allowlist）",
            "Mermaid/公式候选后端的准入（mermaid-rs-renderer 依赖禁库 regex 并带入 "
            "resvg/usvg 0.47，与工作区 0.46 重复；latex-rust 带入 RUSTSEC-2026-0192"
            "（ttf-parser 0.25.1 unmaintained））由任务 1.5 裁决；本轮结论不构成准入",
            "真实图片预览 tab 的适应/1:1/缩放/平移（PL-03，任务 3.3）",
            "Mermaid 独立图像复制与 SVG/PNG 导出（任务 4.3）",
            "公式/Mermaid 的 300ms 防抖与当前资产反馈（任务 4.2）",
            "主题导入的重名/删除/事务式安装（任务 5.2）",
            "护眼模式到亮色的实际渲染外观（只冻结映射契约，未做视觉取证）",
            "Windows/Linux/Intel Mac 的剪贴板与解码差异",
        ],
    }
    write_json(output / "summary.json", summary)
    write_text(output / "matrix.md", render_matrix(summary))
    return summary


def _sha256(path):
    import hashlib
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def _write_flavor(sample, source, work):
    """用系统剪贴板写入原始 flavor（WebP 无法经 ClipboardEntry::Image 写出）。"""
    _set_flavor("webp" if sample.endswith(".webp") else "png", work, source)


def _set_flavor(flavor, work, source=None, second=None):
    """调用系统剪贴板写入指定的 flavor 组合。

    macOS 的 `pbcopy` 只写纯文本，无法声明 webp/gif/html/rtf flavor。为保证
    证据可复跑，真实运行要求系统有 `swiftc`：脚本编译一个最小的 flavor 写入器。
    `source`/`second` 是图片字节来源（按 flavor 需要选用）。
    """
    writer = work / "flavor-writer"
    if not writer.is_file():
        source_file = work / "flavor_writer.swift"
        source_file.write_text(FLAVOR_WRITER_SWIFT)
        run(["swiftc", "-O", str(source_file), "-o", str(writer)], timeout=300)
    argv = [str(writer), flavor]
    if source is not None:
        argv.append(str(source))
    if second is not None:
        argv.append(str(second))
    run(argv)


FLAVOR_WRITER_SWIFT = r'''
import AppKit

// 写入指定 flavor 的剪贴板样本。用法: flavor-writer <flavor> [file] [second]
// flavor 决定声明哪些 UTI；file/second 提供图片字节（可选）。
let arguments = CommandLine.arguments
let flavor = arguments.count > 1 ? arguments[1] : "text"
let file = arguments.count > 2 ? arguments[2] : ""
let second = arguments.count > 3 ? arguments[3] : ""
let pasteboard = NSPasteboard.general
pasteboard.clearContents()

func data(_ path: String) -> Data {
    return (try? Data(contentsOf: URL(fileURLWithPath: path))) ?? Data()
}

switch flavor {
case "text":
    pasteboard.declareTypes([.string], owner: nil)
    pasteboard.setString("纯文本 plain", forType: .string)
case "html-text":
    // 浏览器复制富文本的典型形态：HTML + 纯文本
    pasteboard.declareTypes([.html, .string], owner: nil)
    pasteboard.setString("<b>粗体 bold</b><script>alert(1)</script>", forType: .html)
    pasteboard.setString("粗体 bold", forType: .string)
case "html-only":
    pasteboard.declareTypes([.html], owner: nil)
    pasteboard.setString("<b>仅 HTML</b>", forType: .html)
case "rtf-text":
    let rtf = "{\\rtf1\\ansi \\b hello rtf}".data(using: .ascii)!
    let rtfType = NSPasteboard.PasteboardType("public.rtf")
    pasteboard.declareTypes([rtfType, .string], owner: nil)
    pasteboard.setData(rtf, forType: rtfType)
    pasteboard.setString("hello rtf", forType: .string)
case "file-text":
    // public.file-url 不是 gpui 读的类型；gpui 读的是 NSFilenamesPboardType。
    pasteboard.declareTypes([.fileURL, .string], owner: nil)
    pasteboard.setString("file://" + file, forType: .fileURL)
    pasteboard.setString(file, forType: .string)
case "files":
    // gpui 的文件分支读 NSFilenamesPboardType（属性列表数组）。
    let namesType = NSPasteboard.PasteboardType("NSFilenamesPboardType")
    pasteboard.declareTypes([namesType], owner: nil)
    pasteboard.setPropertyList([file], forType: namesType)
case "png":
    pasteboard.declareTypes([.png], owner: nil)
    pasteboard.setData(data(file), forType: .png)
case "text-png":
    // 纯文本与图片同时存在：验证读取优先级（文本先于图片）。
    pasteboard.declareTypes([.string, .png], owner: nil)
    pasteboard.setString("文本优先 text first", forType: .string)
    pasteboard.setData(data(file), forType: .png)
case "gif-webp":
    // GIF 与 WebP 同时存在，且**先声明 GIF**：若读回的是 WebP，说明选择依据是
    // ImageFormat 的枚举顺序（png→jpeg→webp→gif…），而不是声明顺序。
    let gifType = NSPasteboard.PasteboardType("com.compuserve.gif")
    let webpType = NSPasteboard.PasteboardType("org.webmproject.webp")
    pasteboard.declareTypes([gifType, webpType], owner: nil)
    pasteboard.setData(data(file), forType: gifType)
    pasteboard.setData(data(second), forType: webpType)
case "webp":
    let webpType = NSPasteboard.PasteboardType("org.webmproject.webp")
    pasteboard.declareTypes([webpType], owner: nil)
    pasteboard.setData(data(file), forType: webpType)
default:
    break
}
print("set \(flavor)")
'''


def _first_clipboard_image(rows):
    """从探针输出里取第一个图片项的解码结果。"""
    for row in rows:
        for entry in row.get("entries") or []:
            if entry.get("kind") == "image":
                return entry
    return {}


def render_matrix(summary):
    """把结果渲染成可读矩阵；数字与 summary.json 同源。"""
    lines = ["# V-03 生产支持矩阵（自动生成）", "",
             "本文件由 `scripts/probe_m4_v03.py` 生成，数值与 `summary.json` 一致。", "",
             "## 图片格式 × 路径", "",
             "`文档` = `ImageAssetLoader::load`（应用里 `<img>` 走的入口）；"
             "`剪贴板` = `Image::to_image_data`。两者是**不同代码路径**，帧数可以不同。", "",
             "| 样本 | 格式 | 路径 | 期望帧 | 实测帧 | 尺寸 | 首像素 BGRA | 结论 |",
             "|---|---|---|---|---|---|---|---|"]
    for row in summary["images"]["document"]:
        lines.append("| %s | %s | 文档 | %s | %s | %s×%s | %s | %s |" % (
            row["cell"], row["format"], row["expected_frames"], row.get("frames", "—"),
            row.get("width", "—"), row.get("height", "—"),
            row.get("first_pixel_bgra", "—"), "通过" if row["ok"] else row["detail"]))
    for row in summary["images"]["clipboard"]:
        lines.append("| %s | %s | 剪贴板 | %s | %s | — | — | %s |" % (
            row["cell"], row["format"], row["expected_frames"], row.get("frames", "—"),
            "通过" if row["ok"] else row["detail"]))
    lines += ["", "> SVG 按 `SMOOTH_SVG_SCALE_FACTOR=%s` 放大；像素序为 %s。"
              % (summary["images"]["svg_scale_factor"], summary["images"]["pixel_order"]),
              "", "## 剪贴板读取优先级", "",
              "| 优先级 | 类型 | 说明 |", "|---|---|---|"]
    for row in summary["clipboard"]["read_order"]:
        lines.append("| %d | %s | %s |" % (row["priority"], row["kind"], row["detail"]))
    lines += ["", "写入规则：", ""]
    for key, value in summary["clipboard"]["write_rules"].items():
        lines.append("- `%s`：%s" % (key, value))
    lines += ["", "| 写入用例 | 读回条目 | 期望条目 | 说明 | 结论 |", "|---|---|---|---|---|"]
    for row in summary["clipboard"]["write_cases"]:
        entries = row.get("entries") or []
        read_back = ", ".join(entry.get("kind", "?") for entry in entries) or "—"
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["case"], read_back, ", ".join(row["expected_entries"]),
            row["note"], "通过" if row["ok"] else row["detail"]))
    lines += ["", "| 剪贴板 flavor | 期望路由 | 实测路由 | 条目 | 说明 | 结论 |",
              "|---|---|---|---|---|---|"]
    for row in summary["clipboard"]["flavors"]:
        entries = row.get("entries") or []
        summary_text = ", ".join(entry.get("kind", "?") for entry in entries) or "—"
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            row["flavor"], row["expected_route"], row["route"], summary_text,
            row["note"], "通过" if row["ok"] else "不符"))
    lines += ["", "## SVG 安全边界", "",
              "| 用例 | 默认 resolver 读到文件 | 自定义 resolver 阻断 | 结论 |",
              "|---|---|---|---|"]
    for row in summary["svg"]["cases"]:
        lines.append("| %s | %s | %s | %s |" % (
            row["case"],
            _yes_no(row.get("default_read")),
            _yes_no(row.get("deny_blocked", row.get("deny_allowed"))),
            "通过" if row["ok"] else row["detail"]))
    lines += ["", "> 生产要求：SVG 必须用只放行 `data:` 的自定义 `ImageHrefResolver`，"
              "否则恶意 SVG 可读取进程可读的任意本地文件。", "",
              "## 链接协议", "",
              "| href | 放行 | 原因 |", "|---|---|---|"]
    for row in summary["links"]["verdicts"]:
        lines.append("| `%s` | %s | %s |" % (
            row["href"], "是" if row["allowed"] else "否", row["reason"]))
    lines += ["", summary["links"]["limits"], "", "## 公式语法", "",
              "| 用例 | 期望 | 实测 | 产物字节 | 诊断 | 说明 |",
              "|---|---|---|---|---|---|"]
    for row in summary["math"]["cases"]:
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            row["case"], "成功" if row["expect_ok"] else "失败",
            "成功" if row.get("actual_ok") else "失败",
            row.get("svg_bytes", "—"), row.get("diagnostic") or "—", row["detail"] or "—"))
    lines += ["", "## Mermaid 图类型", "",
              "| 图类型 | 结果 | SVG 字节 | 含文字节点 | 诊断 |", "|---|---|---|---|---|"]
    for row in summary["mermaid"]["diagram_types"]:
        lines.append("| %s | %s | %s | %s | %s |" % (
            row["type"], "支持" if row["ok"] else "失败",
            row.get("svg_bytes", "—"), _yes_no(row.get("has_text")),
            row.get("diagnostic") or "—"))
    lines += ["", "护眼映射：%s" % ", ".join(
        "%s→%s" % (key, value) for key, value in summary["mermaid"]["appearance_map"].items()),
              "", "## 静态主题导入校验", "",
              "| 用例 | 期望导入 | 本层校验 | 候选后端接受 | 后端产物含脚本 | 说明 |",
              "|---|---|---|---|---|---|"]
    for row in summary["mermaid"]["themes"]:
        lines.append("| %s | %s | %s | %s | %s | %s |" % (
            row["case"], "是" if row["expect_import"] else "否",
            "通过" if row["rules_ok"] else "; ".join(row["rules_violations"]),
            _yes_no(row["backend_accepted"]), _yes_no(row["backend_has_script"]),
            row["detail"]))
    lines += ["", "> 候选后端的 `Theme` 反序列化**不封闭**（接受未知字段、不做语义校验），"
              "且会把颜色字段原样写进 SVG 属性；生产必须在导入前用本层契约校验。", "",
              "## 候选依赖闭包", ""]
    for name, facts in summary["dependencies"].items():
        lines.append("- `%s`：%d 包；最高 rust-version %s；禁库命中 %s；许可证未准入 %s" % (
            name, facts["packages"], facts["max_rust_version"] or "—",
            "无" if not facts["forbidden"] else ", ".join(facts["forbidden"]),
            "无" if not facts["license_denied"] else ", ".join(facts["license_denied"])))
        for edge in facts["forbidden_incoming"]:
            lines.append("  - 入边 `%s` → `%s`（%s%s）" % (
                edge["from"], edge["to"], edge["kind"],
                "" if not edge.get("target") else "，target " + edge["target"]))
        if facts["forbidden"]:
            workspace = ", ".join("%s@%s" % (key, "/".join(value))
                                  for key, value in sorted(facts["workspace_versions"].items()))
            lines.append("  - 工作区同名版本：%s（版本不同源于探针独立解析，不等于新增禁库）"
                         % (workspace or "无"))
    lines += ["", "本结果不构成准入；准入裁决在任务 1.5。", "", "## 本轮未测", ""]
    lines += ["- " + item for item in summary["untested"]]
    return "\n".join(lines) + "\n"


def _yes_no(value):
    if value is None:
        return "—"
    return "是" if value else "否"


# ------------------------------------------------------------------ 自测


def self_test():
    """不依赖 Rust/GPUI/剪贴板的自测；任何断言失败即非零退出。"""
    cases = []

    def case(name, function):
        try:
            function()
        except Exception as error:  # noqa: BLE001 - 自测要报告任何失败
            cases.append({"case": name, "passed": False, "detail": str(error)})
        else:
            cases.append({"case": name, "passed": True, "detail": ""})

    case("extensions-cover-prd", lambda: _expect(
        rules.accepted_extensions(), ["bmp", "gif", "jpeg", "jpg", "png", "svg", "webp"]))
    case("jpeg-alias", lambda: _expect(
        rules.image_format_for_extension(".JPEG")["format"], "jpeg"))
    case("unknown-extension", lambda: _expect(
        rules.image_format_for_extension("tiff"), None))
    case("markdown-extensions", lambda: _expect(list(rules.MARKDOWN_EXTENSIONS), ["md"]))

    case("svg-data-allowed", lambda: _expect(
        rules.svg_href_verdict("data:image/png;base64,AA"), "allow"))
    case("svg-relative-denied", lambda: _expect(
        rules.svg_href_verdict("secret.png"), "deny"))
    case("svg-dotdot-denied", lambda: _expect(
        rules.svg_href_verdict("../secret.png"), "deny"))
    case("svg-absolute-denied", lambda: _expect(
        rules.svg_href_verdict("/etc/passwd"), "deny"))
    case("svg-file-uri-denied", lambda: _expect(
        rules.svg_href_verdict("file:///etc/passwd"), "deny"))
    case("svg-http-denied", lambda: _expect(
        rules.svg_href_verdict("https://e/x.png"), "deny"))

    case("clipboard-order", lambda: _expect(
        [kind for kind, _ in rules.CLIPBOARD_READ_ORDER], ["files", "text", "image"]))
    case("clipboard-files-first", lambda: _expect(
        rules.clipboard_route({"files", "text", "image"}), "files"))
    case("clipboard-text-before-image", lambda: _expect(
        rules.clipboard_route({"text", "image"}), "text"))
    case("clipboard-empty", lambda: _expect(rules.clipboard_route(set()), "none"))
    case("clipboard-webp-static", lambda: _expect(
        rules.CLIPBOARD_WEBP_ANIMATED_FRAMES, 1))

    def clipboard_flavor_matrix_shape():
        cases = rules.CLIPBOARD_FLAVOR_CASES
        _expect(len(cases), 9)
        _expect(len({case["flavor"] for case in cases}), len(cases))
        # HTML/RTF 单独存在时必须无路由：这是“不引入富文本导入”的证据行。
        _expect([case["route"] for case in cases
                 if case["flavor"] in ("html-only", "rtf-text")], ["none", "text"])
        # 图片格式选择按枚举顺序，不按声明顺序。
        _expect([case.get("format") for case in cases
                 if case["flavor"] == "gif-webp"], ["Webp"])

    case("clipboard-flavor-matrix", clipboard_flavor_matrix_shape)
    case("clipboard-route-from-entries", lambda: _expect(
        [rules.clipboard_route_from_entries(entries) for entries in (
            None, [], [{"kind": "paths"}], [{"kind": "string"}], [{"kind": "image"}])],
        ["none", "none", "files", "text", "image"]))
    case("clipboard-write-cases-declared", lambda: _expect(
        [case["case"] for case in rules.CLIPBOARD_WRITE_CASES],
        ["single-text", "single-image", "multi-entry"]))
    case("clipboard-write-rules-cover-cases", lambda: _expect(
        sorted(rules.CLIPBOARD_WRITE_RULES), sorted(
            case["case"] for case in rules.CLIPBOARD_WRITE_CASES)))

    # 版本比较必须按数字段比，不能按字符串比：'1.9.0' 在字符串序里大于
    # '1.87.0'，会让“最高 rust-version”报错到小版本上。
    case("rust-version-numeric-order", lambda: _expect(
        _max_rust_version({"packages": [{"rust_version": "1.9.0"},
                                        {"rust_version": "1.87.0"}]}), "1.87.0"))
    case("rust-version-absent", lambda: _expect(
        _max_rust_version({"packages": [{}, {}]}), None))

    def incoming_edges_only_for_forbidden():
        metadata = {
            "packages": [
                {"id": "a 0.1.0", "name": "a", "version": "0.1.0"},
                {"id": "regex 1.13.2", "name": "regex", "version": "1.13.2"},
                {"id": "regex-syntax 0.8.12", "name": "regex-syntax", "version": "0.8.12"},
                {"id": "serde 1.0.0", "name": "serde", "version": "1.0.0"},
            ],
            "resolve": {"nodes": [
                {"id": "a 0.1.0", "deps": [
                    {"pkg": "regex 1.13.2", "dep_kinds": [{"kind": None, "target": None}]},
                    {"pkg": "serde 1.0.0", "dep_kinds": [{"kind": None, "target": None}]},
                ]},
                # 禁库之间的传递边必须被滤掉，否则矩阵里会出现
                # “regex → regex-automata”，看起来像另一个入口。
                {"id": "regex 1.13.2", "deps": [
                    {"pkg": "regex-syntax 0.8.12", "dep_kinds": [{"kind": None}]},
                ]},
                {"id": "regex-syntax 0.8.12", "deps": []},
            ]},
        }
        _expect(_incoming_edges(metadata, {"regex@1.13.2", "regex-syntax@0.8.12"}),
                [{"from": "a@0.1.0", "to": "regex@1.13.2", "kind": "normal",
                  "target": None}])

    case("incoming-edges-filtered", incoming_edges_only_for_forbidden)

    def workspace_versions_from_lock():
        lock = """
[[package]]
name = "regex"
version = "1.13.1"

[[package]]
name = "regex-syntax"
version = "0.8.11"

[[package]]
name = "regex"
version = "1.13.2"

[[package]]
name = "serde"
version = "1.0.0"
"""
        _expect(_workspace_versions({"regex", "regex-syntax"}, lock),
                {"regex": ["1.13.1", "1.13.2"], "regex-syntax": ["0.8.11"]})

    case("workspace-versions-from-lock", workspace_versions_from_lock)

    case("link-https", lambda: _expect(rules.link_verdict("https://e"), (True, "link.allowed")))
    case("link-anchor", lambda: _expect(rules.link_verdict("#a"), (True, "link.allowed")))
    case("link-root-relative", lambda: _expect(rules.link_verdict("/d"), (True, "link.allowed")))
    case("link-mailto", lambda: _expect(rules.link_verdict("mailto:a@b"), (True, "link.allowed")))
    case("link-javascript", lambda: _expect(
        rules.link_verdict("javascript:alert(1)"), (False, "link.rejected.scheme")))
    case("link-file", lambda: _expect(
        rules.link_verdict("file:///etc"), (False, "link.rejected.scheme")))
    case("link-bare-path", lambda: _expect(
        rules.link_verdict("notes/a.md"), (False, "link.rejected.unsupported")))
    case("link-empty", lambda: _expect(
        rules.link_verdict("  "), (False, "link.rejected.empty")))
    case("link-case-insensitive", lambda: _expect(
        rules.link_verdict("HTTPS://e")[0], True))

    case("math-cases-declared", lambda: _expect(
        len(rules.MATH_SYNTAX_CASES), 8))
    case("math-cjk-known-limit", lambda: _expect(
        [row for row in rules.MATH_SYNTAX_CASES if row[0] == "cjk-text"][0][2], False))
    case("math-error-cases", lambda: _expect(
        len(rules.MATH_ERROR_CASES), 2))

    case("mermaid-type-count", lambda: _expect(len(rules.MERMAID_DIAGRAM_TYPES), 23))
    case("mermaid-types-unique", lambda: _expect(
        len(set(rules.MERMAID_DIAGRAM_TYPES)), len(rules.MERMAID_DIAGRAM_TYPES)))
    case("mermaid-eye-care-light", lambda: _expect(
        rules.MERMAID_APPEARANCE_MAP["eye-care"], "light"))
    case("theme-template-valid", lambda: _expect(
        rules.mermaid_theme_violations(rules.mermaid_theme_template()), []))

    def theme_reject(field, value, needle):
        payload = dict(rules.mermaid_theme_template())
        payload[field] = value
        violations = rules.mermaid_theme_violations(payload)
        if not any(needle in item for item in violations):
            raise AssertionError("应当拒绝但通过: %r" % violations)

    case("theme-reject-script", lambda: theme_reject(
        "background", "</style><script>alert(1)</script>", "不是合法颜色"))
    case("theme-reject-quote", lambda: theme_reject(
        "primary_color", '#fff" onload="x', "不是合法颜色"))
    case("theme-reject-url", lambda: theme_reject(
        "primary_color", "url(javascript:alert(1))", "不是合法颜色"))
    case("theme-reject-unknown-field", lambda: theme_reject(
        "unexpected", 1, "未知字段"))

    def theme_missing():
        payload = dict(rules.mermaid_theme_template())
        del payload["font_family"]
        if not any("缺少字段" in item
                   for item in rules.mermaid_theme_violations(payload)):
            raise AssertionError("缺字段未拒绝")

    case("theme-reject-missing", theme_missing)

    def theme_oversized():
        payload = dict(rules.mermaid_theme_template())
        payload["pie_colors"] = ["#ECECFF"] * 13
        if not any("长度 12" in item
                   for item in rules.mermaid_theme_violations(payload)):
            raise AssertionError("超长数组未拒绝")

    case("theme-reject-oversized", theme_oversized)

    def theme_wrong_type():
        payload = dict(rules.mermaid_theme_template())
        payload["font_size"] = "16"
        if not any("必须是数值" in item
                   for item in rules.mermaid_theme_violations(payload)):
            raise AssertionError("错误类型未拒绝")

    case("theme-reject-wrong-type", theme_wrong_type)
    case("theme-fields-closed", lambda: _expect(
        sorted(rules.MERMAID_THEME_FIELDS) == sorted(rules.mermaid_theme_template()),
        True))

    case("solid-png-header", lambda: _expect(
        _solid_png(1, 1, (0, 255, 0, 255))[:8], b"\x89PNG\r\n\x1a\n"))
    case("green-detect", lambda: _expect(_is_green("00ff00ff"), True))
    case("green-detect-bgr", lambda: _expect(_is_green("ff0000ff"), False))

    def image_cells_have_both_paths():
        cells = image_cells()
        paths = {cell["path"] for cell in cells}
        _expect(paths, {"document", "clipboard"})

    case("image-cells-both-paths", image_cells_have_both_paths)

    def webp_clipboard_static():
        cells = [cell for cell in image_cells()
                 if cell["path"] == "clipboard" and cell["format"] == "webp"]
        _expect([cell["expected_frames"] for cell in cells], [1])

    case("webp-clipboard-static", webp_clipboard_static)

    def gif_clipboard_animated():
        cells = [cell for cell in image_cells()
                 if cell["path"] == "clipboard" and cell["format"] == "gif"]
        _expect([cell["expected_frames"] for cell in cells], [3])

    case("gif-clipboard-animated", gif_clipboard_animated)

    def svg_cases_split_by_cwd():
        # 相对 href 必须以 secret/sub 为 cwd，绝对/其余以 work 为 cwd；
        # 若混在一起，相对路径样本永远读不到目标文件（假阴性）。
        work = Path(tempfile.mkdtemp(prefix="aline-m4-v03-selftest-"))
        try:
            cases = svg_cases(work)
            kinds = {case["case"]: case for case in cases}
            inner = str((work / "escape" / "sub").resolve())
            outer = str(work.resolve())
            _expect(str(Path(kinds["relative-cwd"]["cwd"]).resolve()), inner)
            _expect(str(Path(kinds["relative-dotdot"]["cwd"]).resolve()), inner)
            _expect(str(Path(kinds["absolute"]["cwd"]).resolve()), outer)
            groups = _group_by_cwd(cases)
            _expect(sorted(len(group) for _, group in groups), [2, 9])
        finally:
            shutil.rmtree(work, ignore_errors=True)

    case("svg-cases-split-by-cwd", svg_cases_split_by_cwd)

    def svg_pixel_mismatch_is_failure():
        # neutral 样本多画出内容必须判失败，而不是只要“没报错”就算通过。
        cases = [{"case": "neutral-script", "href_kind": "none", "expect_pixels": 64,
                  "file": Path("neutral-script.svg"), "cwd": Path(".")}]
        rows = [{"file": "neutral-script.svg",
                 "result": {"width": 8, "height": 8, "pixels": 64}}]
        _expect(check_svg(cases, rows, [])[0]["ok"], True)
        rows[0]["result"]["pixels"] = 128
        _expect(check_svg(cases, rows, [])[0]["ok"], False)

    case("svg-pixel-mismatch-fails", svg_pixel_mismatch_is_failure)

    passed = all(case["passed"] for case in cases)
    return {"passed": passed, "cases": cases}


def _expect(actual, expected):
    if actual != expected:
        raise AssertionError("期望 %r，实际 %r" % (expected, actual))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                        help="只运行规则与比对逻辑自测，不编译 Rust 或碰剪贴板")
    parser.add_argument("--output-dir", help="证据落点，必须在 logs/validation/ 内")
    args = parser.parse_args(argv)

    if args.self_test:
        result = self_test()
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result["passed"] else 1

    output = output_directory("probe-m4-v03", args.output_dir)
    try:
        summary = real_run(output)
    except (ProbeError, rules.ContractError, Violation) as error:
        write_text(output / "failure.log", str(error))
        print("PROBE_FAILED " + str(error), file=sys.stderr)
        return 1
    print("PROBE_OK images=%d svg=%d math=%d mermaid=%d themes=%d output=%s"
          % (len(summary["images"]["document"]) + len(summary["images"]["clipboard"]),
             len(summary["svg"]["cases"]), len(summary["math"]["cases"]),
             len(summary["mermaid"]["diagram_types"]),
             len(summary["mermaid"]["themes"]), output.relative_to(ROOT)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
