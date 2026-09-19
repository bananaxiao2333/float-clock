"""跨平台系统通知（零依赖，全部走系统自带命令）。"""

from __future__ import annotations

import base64
import os
import shutil
import subprocess
import sys
import threading
__all__ = ["send", "send_async", "backend"]

_TIMEOUT = 10


def backend() -> str:
    """当前平台实际会用的通知后端，供 --diagnose / --test-notify 显示。

    通知图标由系统按「发送通知的程序」决定，三个后端都不提供自定义图标的接口，
    所以这里不做任何平台特有的图标处理。
    """
    if sys.platform == "darwin":
        if shutil.which("terminal-notifier"):
            return "terminal-notifier"
        return "osascript（系统会把发送者认成「脚本编辑器」）"
    if sys.platform.startswith("win"):
        return "PowerShell WinRT Toast"
    return "notify-send"


def _run(cmd: list[str], timeout: float = _TIMEOUT) -> None:
    subprocess.run(
        cmd,
        check=False,
        timeout=timeout,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def _applescript_string(text: str) -> str:
    return text.replace("\\", "\\\\").replace('"', '\\"')


def _send_macos(title: str, body: str, sound: str | None) -> bool:
    terminal_notifier = shutil.which("terminal-notifier")
    if terminal_notifier:
        cmd = [
            terminal_notifier,
            "-title",
            title,
            "-message",
            body,
            "-group",
            "float-clock",
        ]
        if sound:
            cmd += ["-sound", sound]
        _run(cmd)
        return True

    osascript = shutil.which("osascript")
    if not osascript:
        return False
    script = f'display notification "{_applescript_string(body)}"'
    script += f' with title "{_applescript_string(title)}"'
    if sound:
        script += f' sound name "{_applescript_string(sound)}"'
    _run([osascript, "-e", script])
    return True


_WINDOWS_PS = """
$ErrorActionPreference = 'Stop'
[void][Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime]
[void][Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType=WindowsRuntime]
$template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent(
    [Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$texts = $template.GetElementsByTagName('text')
[void]$texts.Item(0).AppendChild($template.CreateTextNode($env:FLOAT_CLOCK_TITLE))
[void]$texts.Item(1).AppendChild($template.CreateTextNode($env:FLOAT_CLOCK_BODY))
$toast = [Windows.UI.Notifications.ToastNotification]::new($template)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('FloatClock').Show($toast)
"""


def _send_windows(title: str, body: str, sound: str | None) -> bool:
    powershell = shutil.which("powershell") or shutil.which("pwsh")
    if not powershell:
        return False
    encoded = base64.b64encode(_WINDOWS_PS.encode("utf-16-le")).decode("ascii")
    env = {
        "FLOAT_CLOCK_TITLE": title,
        "FLOAT_CLOCK_BODY": body,
        "PATH": os.environ.get("PATH", ""),
    }
    try:
        subprocess.run(
            [powershell, "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
            check=False,
            timeout=_TIMEOUT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
    except Exception:
        return False
    return True


def _send_linux(title: str, body: str, sound: str | None) -> bool:
    notify_send = shutil.which("notify-send")
    if not notify_send:
        return False
    _run([notify_send, "-a", "FloatClock", "-u", "normal", title, body])
    return True


def send(title: str, body: str, sound: str | None = None) -> bool:
    """同步发送一条系统通知。失败返回 False 并打印到 stderr。"""
    try:
        if sys.platform == "darwin":
            ok = _send_macos(title, body, sound)
        elif sys.platform.startswith("win"):
            ok = _send_windows(title, body, sound)
        else:
            ok = _send_linux(title, body, sound)
    except Exception as exc:  # pragma: no cover - 平台相关
        print(f"[float-clock] 发送通知失败：{exc}", file=sys.stderr)
        return False
    if not ok:
        print(f"[float-clock] {title} — {body}", file=sys.stderr)
    return ok


def send_async(title: str, body: str, sound: str | None = None) -> None:
    """后台线程发送，避免阻塞 Tk 主循环。"""
    threading.Thread(
        target=send, args=(title, body, sound), daemon=True
    ).start()
