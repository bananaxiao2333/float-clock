"""修正 venv 里 Tcl/Tk 的脚本库搜索路径。

python-build-standalone 的解释器把 ``tcl8.6`` / ``tk8.6`` 数据目录放在**基础前缀**的
``lib/`` 下，但从 venv 里启动时 Tcl 只会在 venv 前缀里找，于是报
``Can't find a usable init.tcl``。这里在创建 ``Tk()`` 之前把 ``TCL_LIBRARY`` /
``TK_LIBRARY`` 指过去（已有的环境变量不覆盖）。
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

__all__ = ["ensure_tcl_library"]


def _first_dir(root: Path, pattern: str, marker: str) -> Path | None:
    if not root.is_dir():
        return None
    for candidate in sorted(root.glob(pattern)):
        if (candidate / marker).is_file():
            return candidate
    return None


def ensure_tcl_library() -> dict[str, str]:
    """按需设置 TCL_LIBRARY / TK_LIBRARY，返回本次实际设置的值。"""
    applied: dict[str, str] = {}
    library = Path(sys.base_prefix) / "lib"
    for var, pattern, marker in (
        ("TCL_LIBRARY", "tcl[0-9]*", "init.tcl"),
        ("TK_LIBRARY", "tk[0-9]*", "tk.tcl"),
    ):
        if os.environ.get(var):
            continue
        found = _first_dir(library, pattern, marker)
        if found is not None:
            os.environ[var] = str(found)
            applied[var] = str(found)
    return applied
