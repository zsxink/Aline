#!/usr/bin/env python3
"""证据路径规范化：仓库内记录不保存本机绝对路径。

证据脚本在写入 argv、cwd、环境变量和原始输出前调用本模块，把本机路径
替换为稳定占位符；历史证据用同一规则一次性规范化。占位符不指向任何磁盘位置：

- ``<repo>`` 本仓库根
- ``<cargo-home>`` / ``<rustup-home>`` 工具链缓存
- ``<tmp>`` 临时目录根
- ``<home>`` 用户主目录
- ``<external>/<name>`` 仓库外参考项目
- ``<command-line-tools>`` / ``<system-volume>`` / ``<netbsd-sysroot>`` 平台路径

路径片段（``argv`` 元素、``cwd``、环境变量值、日志文本）都可能含有本机路径，
统一经过 :func:`normalize_text`；结构化记录用 :func:`normalize_record` 递归处理。
"""

import re

# 单个路径段：排除分隔符、空白、引号和常见定界符。
_SEG = r'[^/\s"\'`<>()\[\]{},;:|]+'

# 路径 token 起点：前面不是路径字符，也不是已生成的占位符尾部。
# 用于系统目录规则，避免把 ``<home>/Library/...`` 再替换成 ``<home><library>/...``。
_START = r'(?<![A-Za-z0-9_$~}>])'

# 临时目录根：macOS /var/folders/<a>/<b>/{T,C}/ 与 /tmp。
_TEMP_FOLDERS = r'/(?:private/)?var/folders/' + _SEG + r'/' + _SEG + r'/[TC]/'

_GENERIC_RULES = (
    # macOS 固件卷前缀等价于根目录：先剥离，后续规则按普通路径处理。
    (_START + r'/System/Volumes/Data(?=/)', ''),
    # 外部参考仓库先于用户主目录兜底；仓库自身单独识别。
    (_START + r'/(?:Users|home)/' + _SEG + r'/Project/(?:book|music)/(' + _SEG + r')', r'<external>/\1'),
    (_START + r'/(?:Users|home)/' + _SEG + r'/Project/editor/Aline', '<repo>'),
    # 工具链缓存先于用户主目录兜底。
    (_START + r'/(?:Users|home)/' + _SEG + r'/(?:Library/Caches/aline-m0/cargo-home|\.cargo)', '<cargo-home>'),
    (_START + r'/(?:Users|home)/' + _SEG + r'/\.rustup', '<rustup-home>'),
    # NetBSD 交叉 sysroot：临时解包与系统安装位置统一。
    (_START + r'/(?:private/)?tmp/netbsd', '<netbsd-sysroot>'),
    (_START + r'/usr/local/x86_64-unknown-netbsd', '<netbsd-sysroot>'),
    # macOS 命令行工具：解释器退化为 PATH 中的名字。
    (_START + r'/Library/Developer/CommandLineTools/usr/bin/(?:python3|python)', 'python3'),
    (_START + r'/Library/Developer/CommandLineTools', '<command-line-tools>'),
    # 系统卷与 /usr/local：只在 token 起点替换，避免嵌套路径二次改写。
    (_START + r'/System/Volumes/Data', '<system-volume>/Data'),
    (_START + r'/Volumes/Data', '<volumes>/Data'),
    (_START + r'/usr/local/lib', '<local>/lib'),
    (_START + r'/usr/local', '<local>'),
    (_START + r'/usr/lib', '<system>/lib'),
    (_START + r'/usr/include', '<system>/include'),
    (_START + r'/usr/bin/(?:python3|python)', 'python3'),
    (_START + r'/usr/bin/(env)', r'\1'),
    (_START + r'/usr/bin/(xattr|chflags|printenv|clang|df|ls|chmod)', r'\1'),
    (_START + r'/bin/(zsh|bash|sh|chmod|df|ls)', r'\1'),
    (_START + r'/sbin/', '<system>/sbin/'),
    (_START + r'/Library/', '<library>/'),
    (_START + r'/System/', '<system>/'),
    (_START + r'/etc/', '<system>/etc/'),
    (_START + r'/opt/homebrew', '<homebrew>'),
    (_START + r'/Applications/', '<applications>/'),
    # 临时目录先于用户主目录兜底。
    (_START + r'/(?:private/)?var/folders/' + _SEG + r'/\.\.\.', '<tmp>/...'),
    (_START + _TEMP_FOLDERS, '<tmp>/'),
    (_START + r'/(?:private/)?tmp/', '<tmp>/'),
    (_START + r'/(?:Users|home)/' + _SEG, '<home>'),
)


def _runtime_rules(root, cargo_home, rustup_home, temp_dirs):
    """当前进程的真实根目录优先，覆盖 CI 等非 /Users、/home 布局。"""
    rules = []
    for base, label in ((cargo_home, '<cargo-home>'), (rustup_home, '<rustup-home>'),
                        (root, '<repo>')):
        if base and str(base).rstrip('/') not in ('', '/'):
            rules.append((_START + re.escape(str(base).rstrip('/')), label))
    for base in temp_dirs:
        if base and str(base).rstrip('/') not in ('', '/'):
            rules.append((_START + re.escape(str(base).rstrip('/')), '<tmp>'))
    return tuple(rules)


def _roots(root, cargo_home, rustup_home, temp_dirs):
    if root is None:
        from pathlib import Path
        root = Path(__file__).resolve().parent.parent
    if cargo_home is None:
        import os
        cargo_home = os.environ.get('CARGO_HOME')
    if rustup_home is None:
        import os
        rustup_home = os.environ.get('RUSTUP_HOME')
    if temp_dirs is None:
        import tempfile
        temp_dirs = (tempfile.gettempdir(),)
    return root, cargo_home, rustup_home, temp_dirs


def normalize_text(text, root=None, cargo_home=None, rustup_home=None, temp_dirs=None):
    """替换文本中的本机绝对路径；保留 shebang 行原样。"""
    if not text:
        return text
    head, separator, rest = text.partition('\n')
    if head.startswith('#!') or head.startswith('﻿#!'):
        return head + separator + normalize_text(
            rest, root, cargo_home, rustup_home, temp_dirs) if separator else head
    root, cargo_home, rustup_home, temp_dirs = _roots(root, cargo_home, rustup_home, temp_dirs)
    for pattern, replacement in _runtime_rules(root, cargo_home, rustup_home, temp_dirs):
        text = re.sub(pattern, replacement, text)
    for pattern, replacement in _GENERIC_RULES:
        text = re.sub(pattern, replacement, text)
    return text


def normalize_path(path):
    """返回路径的占位符形式，便于写入诊断和汇总。"""
    return normalize_text(str(path))


def normalize_record(value):
    """递归规范化结构化证据中的字符串，哈希与布尔值原样保留。"""
    if isinstance(value, str):
        return normalize_text(value)
    if isinstance(value, list):
        return [normalize_record(item) for item in value]
    if isinstance(value, tuple):
        return tuple(normalize_record(item) for item in value)
    if isinstance(value, dict):
        return {key: normalize_record(item) for key, item in value.items()}
    return value


def display_path(path, root=None):
    """仓库内路径显示为相对路径，仓库外路径走占位符规则。"""
    from pathlib import Path
    if root is None:
        root = Path(__file__).resolve().parent.parent
    candidate = Path(path)
    try:
        return str(candidate.resolve().relative_to(Path(root).resolve()))
    except (OSError, ValueError):
        return normalize_text(str(candidate), root=root)


def write_text(path, text):
    """写入证据文本前先规范化，避免脚本重新引入本机路径。"""
    from pathlib import Path
    Path(path).write_text(normalize_text(text))


def write_json(path, value):
    """结构化证据按 :func:`normalize_record` 规范化后写入。"""
    import json
    from pathlib import Path
    Path(path).write_text(
        json.dumps(normalize_record(value), ensure_ascii=False, indent=2) + '\n')


def _home():
    import os
    return os.environ.get('HOME') or ''


def _platform_roots():
    """占位符对应的当前平台根；无法确定时省略，由调用方保留原文。"""
    import os
    home = _home()
    return {
        '<home>': home,
        '<cargo-home>': os.environ.get('CARGO_HOME') or (
            home + '/Library/Caches/aline-m0/cargo-home' if home else ''),
        '<rustup-home>': os.environ.get('RUSTUP_HOME') or (home + '/.rustup' if home else ''),
        '<command-line-tools>': '/Library/Developer/CommandLineTools',
        '<library>': '/Library',
        '<system>': '/System',
        '<system-volume>': '/System/Volumes',
        '<volumes>': '/Volumes',
        '<local>': '/usr/local',
        '<homebrew>': '/opt/homebrew',
        '<applications>': '/Applications',
    }


def _temp_bases(temp_dirs=None):
    """临时目录候选：标准库默认与 ``TMPDIR`` 在前，``/tmp`` 兜底。

    顺序保证还原结果与进程自身 ``tempfile`` 产生的绝对路径一致。
    """
    import os
    import tempfile
    bases = []
    for base in list(temp_dirs or ()):
        if base:
            bases.append(str(base).rstrip('/'))
    bases.append(tempfile.gettempdir().rstrip('/'))
    for base in (os.environ.get('TMPDIR'), '/tmp', '/private/tmp'):
        if base:
            bases.append(str(base).rstrip('/'))
    seen = []
    for base in bases:
        if base and base not in seen:
            seen.append(base)
    return seen


def resolve_path(text, root=None, temp_dirs=None):
    """把占位符还原成当前机器上的真实路径；含不可还原占位符时返回 ``None``。

    ``<tmp>`` 可能有多个候选根（``TMPDIR``、``/tmp``、``/private/tmp``），
    优先选路径确实存在的那个根；都不存在时退回首个候选，使同一进程内
    同一占位符始终得到同一字符串，记录之间的路径比较不会误报不一致。
    """
    from pathlib import Path
    if not text:
        return text
    root = Path(root).resolve() if root is not None else Path(__file__).resolve().parent.parent
    mapping = _platform_roots()
    mapping['<repo>'] = str(root)
    for placeholder, base in mapping.items():
        if base:
            text = text.replace(placeholder, base)
    if '<tmp>' in text:
        bases = _temp_bases(temp_dirs)
        if not bases:
            return None
        chosen = bases[0]
        fallback = None
        for base in bases:
            candidate = text.replace('<tmp>', base)
            if Path(candidate).exists():
                chosen = base
                break
            if fallback is None and Path(candidate).parent.exists():
                fallback = base
        else:
            if fallback is not None:
                chosen = fallback
        text = text.replace('<tmp>', chosen)
    if '<netbsd-sysroot>' in text or '<external>' in text:
        return None
    return text if '<' not in text.split('/')[0] else None
