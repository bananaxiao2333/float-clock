"""Fix the Tcl/Tk script library search path inside a venv.

A python-build-standalone interpreter keeps its ``tcl8.6`` / ``tk8.6`` data
directories under ``lib/`` in the **base prefix**, but when Tcl starts up from a
venv it only looks inside the venv prefix and then reports
``Can't find a usable init.tcl``. So before ``Tk()`` is created, point
``TCL_LIBRARY`` / ``TK_LIBRARY`` at the right place (an existing environment
variable is never overwritten).
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
    """Set TCL_LIBRARY / TK_LIBRARY when needed and return the values actually set this time."""
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
