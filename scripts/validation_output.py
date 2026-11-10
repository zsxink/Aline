"""验证运行输出统一放入仓库 logs/validation，默认每次创建独立目录。"""

from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parent.parent

# 唯一的越界拒绝消息：守卫按其中的 logs/validation 字样断言，不另写措辞。
REJECTION = '验证输出目录必须位于仓库 logs/validation 内'


def _validation_base(root):
    """仓库内唯一允许承载验证输出的目录根。"""
    return Path(root).resolve() / 'logs' / 'validation'


def _resolve_within(requested, root):
    """唯一的落点解析规则：只解析并检查，不创建任何目录。

    相对路径始终相对仓库根解析；``resolve()`` 在检查前暴露符号链接，指向
    仓库外（含 ``logs`` 本身被替换为链接）时一并拒绝。此检查用于防止误把
    运行输出写入源码、文档或仓库外；不承诺对抗并发替换路径的进程。调用方
    仍须使用受控工作树，不共享可写输出目录。
    """
    root = Path(root).resolve()
    target = Path(requested)
    if not target.is_absolute():
        target = root / target
    target = target.resolve()
    try:
        target.relative_to(_validation_base(root))
    except ValueError as error:
        raise ValueError(REJECTION) from error
    return target


def require_validation_path(requested, root=ROOT):
    """校验调用方给出的证据落点并返回解析后的真实路径；只校验，绝不创建。

    供自带“目录必须已存在／拒绝覆盖”语义的 CLI 参数使用（例如
    ``mkdir(exist_ok=False)`` 的导出目录、必须已存在的矩阵目录），落点规则
    与 :func:`output_directory` 完全同源，不另写第二套路径逻辑。
    """
    if requested is None:
        raise ValueError(REJECTION)
    return _resolve_within(requested, root)


def output_directory(check_name, requested=None, root=ROOT):
    """在写盘前检查真实落点；相对 --output-dir 始终相对仓库根解析。

    此检查用于防止误把运行输出写入源码、文档或仓库外；不承诺对抗并发
    替换路径的进程。调用方仍须使用受控工作树，不共享可写输出目录。
    """
    if not check_name or Path(check_name).name != check_name or check_name in ('.', '..'):
        raise ValueError('验证检查名必须是单个目录名')
    if requested is None:
        target = _resolve_within(_validation_base(root) / check_name, root)
    else:
        target = _resolve_within(requested, root)
    target.mkdir(parents=True, exist_ok=True)
    if requested is None:
        return Path(tempfile.mkdtemp(prefix='run-', dir=target))
    return target
