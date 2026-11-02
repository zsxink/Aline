"""验证运行输出统一放入仓库 logs/validation，默认每次创建独立目录。"""

from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def output_directory(check_name, requested=None, root=ROOT):
    """在写盘前检查真实落点；相对 --output-dir 始终相对仓库根解析。

    此检查用于防止误把运行输出写入源码、文档或仓库外；不承诺对抗并发
    替换路径的进程。调用方仍须使用受控工作树，不共享可写输出目录。
    """
    root = Path(root).resolve()
    base = root / 'logs' / 'validation'
    if not check_name or Path(check_name).name != check_name or check_name in ('.', '..'):
        raise ValueError('验证检查名必须是单个目录名')
    target = Path(requested) if requested is not None else base / check_name
    if not target.is_absolute():
        target = root / target
    target = target.resolve()
    try:
        target.relative_to(base)
    except ValueError as error:
        raise ValueError('验证输出目录必须位于仓库 logs/validation 内') from error
    target.mkdir(parents=True, exist_ok=True)
    if requested is None:
        return Path(tempfile.mkdtemp(prefix='run-', dir=target))
    return target
