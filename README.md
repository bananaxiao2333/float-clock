# FloatClock (Python edition)

> **This is the `python` branch** — the earliest implementation, written with uv + Tk, kept here as an archive.
> The current version is the [Rust rewrite on `main`](https://github.com/bananaxiao2333/float-clock):
> one codebase cross-compiled into a single-file executable for macOS, Linux and Windows, with no Python
> runtime to install and the same look and feel on all three platforms.

[![Python CI](https://github.com/bananaxiao2333/float-clock/actions/workflows/python.yml/badge.svg?branch=python)](https://github.com/bananaxiao2333/float-clock/actions/workflows/python.yml)
[![Python](https://img.shields.io/badge/Python-3.11%2B-3776AB?logo=python&logoColor=white)](https://www.python.org/)
[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![Tk](https://img.shields.io/badge/Tk-8.6%20required-orange)](#troubleshooting-black-background-and-text-smearing)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-2ea44f)](#cross-platform-notes)
[![Tests](https://img.shields.io/badge/test-47%20passed-success)](#tests)

A floating T± countdown overlay with **text only and no background**:

- Bold green monospace text (Menlo / Consolas / DejaVu Sans Mono, picked automatically)
- Every number is **zero-padded and aligned** (`00:05:03`, `01:02:03:04` past a day), so the width does not
  jitter as the seconds tick
- **Drag** with the left mouse button, **right-click to lock / unlock**
- Main title = how long until the `Offset moment`; subtitle = how long ago the `Target time` was
- Third line = target time + offset, rendered as **solid green with the glyphs knocked out** (the desktop
  shows through the strokes)
- **System notifications** fire automatically around each moment
- Managed with uv; the only dependency is Pillow (used solely for the knocked-out third line bitmap),
  everything else is the standard library

```
T-00:04:32              <- main title: 4 min 32 s until the Offset moment
T+00:00:28              <- subtitle: 28 s since the Target time
▛09:30:00 | -00:05:00▟  <- third line: green knockout (the desktop shows through the glyphs)
```

## Tests

```bash
uv run python -m unittest discover -s tests -v      # 47 tests
```

`tests/test_overlay_smoke.py` really opens a Tk window and **needs a real display**
(on CI only the pure-logic `test_float_clock.py` runs).

---

## Cross-platform notes

The core features (floating window, dragging, locking, T± timing, system notifications) are one and the
same code on all three platforms, with no platform-specific dependency:

| Capability | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Borderless always-on-top window | ✅ `overrideredirect` | ✅ | ✅ |
| **No background (text only)** | ✅ `-transparent` + `systemTransparent`, **needs Tk 8.6** | ✅ `-transparentcolor` keying | ⚠️ X11 has no real transparency, falls back to the `x11_background` colour |
| Drag / right-click lock / double-click settings | ✅ | ✅ | ✅ |
| T± timing, zero-padded monospace, main title + subtitle | ✅ | ✅ | ✅ |
| System notifications | ✅ `osascript` (uses terminal-notifier when installed) | ✅ PowerShell WinRT Toast | ✅ `notify-send` |
| Third line "green knockout" | ✅ AppKit overlay | Falls back to plain green text | Falls back to plain green text |

**Only two things are platform-specific**, and both degrade gracefully instead of stopping the program:

1. **How the transparent background is painted** (`overlay._configure_window`) — one branch per platform,
   with Linux falling back to a solid colour.
2. **Knocking out the third line** (`knockout.py` + `macos_overlay.py`) — the knockout needs an image with
   alpha composited onto the window, and Tk cannot draw images on a transparent window (see
   "Implementation notes" below). On macOS this goes through an AppKit subview; elsewhere `info_style` is
   treated as `text` automatically, which is just plain green text.
   If you do not want any of it, simply hard-code `info_style = "text"`.

As for notification icons: **none of the three backends offers an API for a custom icon** — the icon is
chosen by the system from the program that sends the notification (Script Editor on macOS). That is a
system limitation, so there is no platform-specific workaround for it.

**Honest verification status:** the Python/Tk path was verified on **macOS only**. The Windows and Linux
columns above describe code paths that exist and are guarded by graceful fallbacks, but they have not been
exercised on real Windows or Linux machines on this branch. The `python.yml` workflow runs on
`ubuntu-latest`, but it only runs the headless unit tests — the Tk smoke tests need a real display, so
they never run there.

---

## How to configure it

Three ways, all effective immediately, **no restart needed**:

1. **Double-click the overlay** → a settings window opens; change the target time / offset / colour / font
   size and press "Apply"
2. **Edit `config.toml` directly** → just save it; the program checks the file every second and picks the
   change up within about a second (comments are preserved)
3. **Temporary command-line override** → `uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00`

Controls: **drag = left button, lock/unlock = right button, settings = double-click, menu = middle button**.
The overlay shows three lines of text and no operating hints at all; errors go to system notifications and
to stderr on the terminal.

You can also inspect the current state at any time without opening a window:

```bash
uv run float-clock --print
```

---

## Quick start

```bash
cd float-clock

uv run float-clock --init-config        # write the default config.toml (Target time = the next whole hour)
uv run float-clock                      # start the overlay
```

You do not have to write a config first either: the first run generates `config.toml` on its own.

To pin a time for one run:

```bash
uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00
```

To check the current state and the upcoming reminder schedule without opening a window:

```bash
uv run float-clock --print
```

> **Requirements**: Python ≥ 3.11.
> `.python-version` is pinned to **3.12.7** **for macOS** — that is the last uv-managed build that bundles
> Tk 8.6, and Tk 9.0 paints the transparent background black on macOS (the reason is in
> "Troubleshooting: black background and text smearing"). Windows and Linux are not bound by that; to use
> another interpreter just run `uv run --python 3.13 float-clock`, the pin in this repository is merely a
> conservative default for them.

Command-line options:

| Option | Meaning |
| --- | --- |
| `--config PATH` | path to the config file (default `./config.toml`) |
| `--init-config` | write a default config file, then exit |
| `--force` | with `--init-config`, overwrite an existing config |
| `--target T` | target time T, e.g. `'2026-01-01T09:30:00'` / `'09:30'` / `'+1h'` |
| `--offset D` | offset, e.g. `'-00:05:00'` / `'1h30m'` / `'-300'` |
| `--print` | print the state only, without opening a window |
| `--selftest SECONDS` | close the window automatically after N seconds (self-check) |
| `--settings` | open the settings window on startup |
| `--test-notify` | send one test notification, then exit |
| `--diagnose` | print the window state one second after it opens (whether borderless / transparent really took effect), then exit |
| `-h`, `--help` | show the help text |
| `-V`, `--version` | show the version |

---

## T± semantics

The whole program uses rocket-countdown notation:

| Display | Meaning |
| --- | --- |
| `T-00:05:00` | that moment is **5 minutes away** |
| `T-00:00:00` | the moment itself |
| `T+00:00:12` | the moment **passed 12 seconds ago** |

There are two moments in the program:

```
Target time    T  = [time] target in config.toml
Offset moment  M  = T + offset
```

- **Main title**: counts down to `M` ("how long until the offset moment")
- **Subtitle**: referenced to `T` ("how long ago the target time was")

Example: `target = "09:30:00"` and `offset = "-00:05:00"` give `M = 09:25:00`.
At 09:20 the main title is `T-00:05:00` (5 minutes until 09:25) and the subtitle is `T-00:10:00`
(10 minutes until 09:30); at 09:26 the main title becomes `T+00:01:00` (09:25 passed a minute ago) and the
subtitle is `T-00:04:00`.

With `offset = 0` both titles point at the same moment; a positive offset is after `T`, a negative one
before it.

---

## Worked example: a 20:29 leak with a 120-minute response window

Put the "moment the incident happened" in `target` and the "window" in `offset`, and both numbers fall
out of it:

```toml
[time]
# The moment the leak happened (20:29 that day)
target = "2026-09-19T20:29:00"
# 120-minute emergency response window -> deadline = 20:29 + 120min = 22:29
offset = "+120m"
```

So:

- **Main title** `= how long until the 22:29 deadline` <- watch this to know how much time is left
- **Subtitle** `= how long ago the leak happened` <- the wording you report with

At 21:45:38 it looks like this:

```
T-00:43:22            <- 43 min 22 s until the 22:29 response deadline
T+01:16:38            <- the leak happened 1 h 16 min 38 s ago
20:29:00 | +02:00:00  <- the leak moment | the 120-minute response window
```

How the main title and subtitle move along the timeline:

| Time | Main title | Subtitle | Meaning |
| --- | --- | --- | --- |
| 19:29 | `T-03:00:00` | `T-01:00:00` | 3 hours to the deadline, 1 hour until the leak |
| 20:29 | `T-02:00:00` | `T-00:00:00` | **the leak happens** |
| 21:29 | `T-01:00:00` | `T+01:00:00` | the window is half over |
| 22:24 | `T-00:05:00` | `T+01:55:00` | 5 minutes to the deadline |
| 22:29 | `T-00:00:00` | `T+02:00:00` | **the response window expires** |
| 22:35 | `T+00:06:00` | `T+02:06:00` | 6 minutes over |

Suggested reminders inside the window (for a 120-minute window, add `7200` and `3600` to `before`):

```toml
[notify]
before = [7200, 3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
after  = [1, 5, 30, 60, 300, 1800]
```

That fires a system notification at these points: **2 h / 1 h / 30 min / 15 min / 10 min / 5 min / 3 min /
2 min / 1 min / 30 s / 10 s / 5 s / 3 / 2 / 1 s before the deadline**, once more on the moment itself, and
again 1 s / 5 s / 30 s / 1 min / 5 min / 30 min after it is over.
(The `7200` entry lands exactly on the moment the leak happened.)

One command to see the effect, without opening a window:

```bash
uv run float-clock --print --target "2026-09-19T20:29:00" --offset +120m
```

The third line can be reshaped as needed:

```toml
[display]
info_template = "{time} | {delta}"                  # 20:29:00 | +02:00:00 (the default)
# info_template = "{datetime} ({delta_human})"      # 2026-09-19 20:29:00 (+2h)
# info_template = "{time} -> {mark}"                # 20:29:00 -> 22:29:00
# info_template = ""                                # no third line at all
# info_style = "text"                               # third line as plain green text (no knockout)
```

> If you only want the same clock time every day, `target` can be written as `"20:29:00"` — once that time
> has passed today it rolls over to tomorrow automatically.
> For a real incident, write the full date, or tomorrow it turns into "20:29 tomorrow".

---

## Controls

| Action | Effect |
| --- | --- |
| Left-button drag | Move the overlay (the position is written back to `config.toml` on release and restored there next time) |
| **Double-click** | Open the settings window |
| **Right-click** | **Lock / unlock**; the outline tells you which: **solid = locked (cannot be dragged)**, **dashed = draggable** |
| Middle button / Ctrl+right-click / Cmd+right-click | Pop-up menu (lock, settings, reload config, quit) |
| Ctrl+L | Lock / unlock |
| Ctrl+, | Open the settings window |
| Ctrl+R | Reload the config manually |
| Ctrl+Q | Quit |

The locked state is **no longer announced in words**; it uses the outline instead: solid means pinned,
dashed means movable, visible at a glance.
To drop the outline set `[display] lock_indicator` to `"none"`, or swap it round so that locked is dashed
and unlocked is solid (`solid_when_locked = false`).

The window is borderless, always on top, and does not take a taskbar / Dock slot.

> The keyboard shortcuts need the window to have keyboard focus, and a borderless window sometimes cannot
> get focus on macOS. **Right-click / double-click / middle button always work**, so none of those actions
> depend on a shortcut.

### Settings window

Menu (right-click) → "Settings…" edits the Target time, offset, colour and font sizes directly; pressing
"Apply" writes them back to `config.toml` (**keeping the existing comments**) and takes effect at once.

You can equally well edit `config.toml` by hand: the program checks the file's modification time every
second, so **saving is enough, no restart**.

---

## Configuration reference (config.toml)

```toml
[window]
x = 80                 # overlay top-left corner (updated automatically after a drag)
y = 80
borderless = true      # no title bar, text only
topmost = true         # always on top
locked = false         # toggled by right-click, written back here
opacity = 1.0          # overall opacity

[display]
font_family = ""       # empty = pick a system monospace font automatically; "Menlo" also works
main_size = 46         # main title font size
sub_size = 18          # subtitle font size
color = "#00FF66"      # green
sub_color = ""         # subtitle colour, empty = same as the main title
bold = true            # bold
show_days = true       # show DD:HH:MM:SS past a day
gap = 2
info_template = "{time} | {delta}"   # third line contents; "" hides the whole line
info_size = 14         # third-line font size
info_color = ""        # third-line colour (in knockout mode this is the "background"), empty = green
info_style = "auto"    # auto = knock out when possible; knockout / text force one of the two
x11_background = "#101010"   # background colour where transparency is unsupported, Linux for instance
interval_ms = 200      # refresh interval
lock_indicator = "border"    # outline showing the locked state; "none" turns it off
border_color = ""            # outline colour, empty = same colour as the text
border_width = 2             # outline width
solid_when_locked = true     # true: locked = solid / unlocked = dashed; false the other way round
main_template = "T{sign}{clock}"
sub_template = "T{sign}{clock}"

[time]
target = "2026-01-01T09:30:00"
offset = "-00:05:00"

[notify]
enabled = true
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
after = [1, 5, 30, 60, 300]
at_moment = true
sound = true
sound_name = "Glass"
title_template = "[T{sign}{clock}] {label}"
body_before = "{human} until {label}"
body_at = "{label} reached at {time}"
body_after = "{label} passed {human} ago"
```

### Target time formats

`target` accepts:

- `2026-01-01T09:30:00`, `2026-01-01 09:30`, `2026-01-01` (ISO / common formats)
- `2026/02/03 08:05`, `02-03 08:05`
- `09:30` / `09:30:00` — that clock time today, **rolled over to tomorrow once it has passed**
  (handy for a fixed daily schedule)
- `+1h30m` — relative to now

`offset` accepts:

- `-00:05:00`, `5:00`, `+00:00:30`, `01:02:03:04` (days:hours:minutes:seconds)
- `1h30m`, `90m`, `2d`, `90s`
- `-300` / `300` — a bare number counts as **seconds**

---

## System notifications

Reminders are scheduled separately **for both moments** (the `Target time` `T` and the `Offset moment`
`M`):

- every number of seconds in `before`: one reminder ahead of time, titled like
  `[T-00:05:00] Offset moment`, with the body `5m until Offset moment`
- `at_moment = true`: one reminder on the moment, titled `[T-00:00:00] Target time`, body
  `Target time reached at 20:29:00`
- every number of seconds in `after`: one reminder after the moment, titled like
  `[T+00:00:05] Target time`, with the body `Target time passed 5s ago`

The details of the rules:

- **Nothing is sent twice**: each reminder fires once only (even if you drag the window and the config is
  reloaded).
- **No catching up at startup**: reminders that are already in the past when the program starts are not
  sent; only the ones ahead are scheduled.
- Editing `before` / `after` re-arms the reminder queue.
- Sending goes through system commands and does not block the UI (background thread):
  - macOS: `terminal-notifier` when available, otherwise `osascript -e 'display notification …'`
  - Windows: the PowerShell WinRT Toast API
  - Linux: `notify-send`

**No notification on macOS the first time?** Go to System Settings → Notifications, allow notifications for
**Script Editor** (or Terminal, or terminal-notifier) and turn off Focus. When a notification is sent with
osascript, the system treats Script Editor as the sender.

---

## Implementation notes

- Transparent background: macOS uses `wm attributes -transparent` plus the `systemTransparent` colour
  (**Tk 8.6 required**, Tk 9.0 has a regression); Windows keys out a colour with `-transparentcolor`;
  Linux degrades to a dark background (configurable through `x11_background`). The three branches do not
  affect each other, other platforms merely lose one effect.
- **Window style must be set before the window is mapped**: `withdraw()` → style → widgets →
  `geometry()` → `deiconify()`, otherwise macOS puts the title bar back and transparency stops working.
  `--diagnose` verifies this.
- The lock indicator is an outline on the canvas (solid / dashed); no text hint is needed.
- The third line is content rather than a hint: it is composed from `info_template`, whose placeholders are
  `{date} {time} {datetime}`, `{mark} {mark_datetime}` and `{delta} {delta_human}`.
- **Why the third line's "green knockout" takes such a detour**: Tk 8.6 **draws no image at all** on a
  `-transparent` window (`tk.Label(image=...)` and `canvas.create_image(...)` both come out fully
  transparent in practice, while the same alpha PNG is perfectly fine in an opaque window), and
  `fg=systemTransparent` does not punch a hole either (measured pixel by pixel it is identical to
  `fg=black`, i.e. a no-op). So the bitmap for that line is generated with Pillow (`knockout.py`) and then
  composited onto the window through an AppKit subview (`macos_overlay.py`, which talks to the ObjC runtime
  through plain ctypes, without pulling in PyObjC). The subview overrides `hitTest:` to return nil, so
  mouse events keep falling through to Tk and dragging is unaffected.
- Zero-padded and fixed width: `format_hms()` only ever produces `HH:MM:SS` / `DD:HH:MM:SS`, which together
  with a monospace font keeps the text from shifting as the seconds tick.
- Remaining time is rounded up and elapsed time rounded down, so the very second of the moment shows
  exactly `T-00:00:00`.
- Config write-back locates `section.key` line by line, so neither comments nor ordering are lost.

### Project layout

```
float-clock/
├── pyproject.toml            # uv project definition + the float-clock entry script
├── .python-version           # 3.12.7 (uv-managed interpreter: bundles Tk 8.6, which transparency needs)
├── config.toml               # generated / edited at runtime
├── src/float_clock/
│   ├── __main__.py           # command-line entry point
│   ├── overlay.py            # the overlay: transparent background, drag, lock, rendering
│   ├── tclenv.py             # fixes the Tcl/Tk data directory search path inside the venv
│   ├── knockout.py           # third-line knockout bitmap (Pillow, cross-platform, degrades to green text)
│   ├── macos_overlay.py      # macOS only: composites the knockout bitmap onto the window (a no-op elsewhere)
│   ├── notifier.py           # reminder scheduling around the moments (de-duplicated)
│   ├── notify.py             # cross-platform system notifications
│   ├── config.py             # TOML loading / comment-preserving write-back / default config
│   └── timefmt.py            # T± formatting and time parsing
└── tests/
    ├── test_float_clock.py       # pure logic
    ├── test_overlay_smoke.py     # real window: borderless / transparent pixels / drag / lock / hot reload
    └── macos_probe.py            # reads the NSWindow render bitmap (no Screen Recording permission needed)
```

### Tests

```bash
uv run python -m unittest discover -s tests -v
uv run float-clock --print           # no window: print the T± state and the reminder schedule
uv run float-clock --diagnose        # report one second after opening whether borderless / transparent really took effect
uv run float-clock --selftest 5      # open the window and self-check for 5 seconds
```

### Troubleshooting: the window has a title bar

On macOS `overrideredirect` (dropping the title bar) and `-transparent` (transparent background) **must be
set before the window is mapped**, and only then shown in one go with `deiconify()`; if the system remaps
the window in between, the title bar comes back and `geometry` is ignored too.

This project already does it the right way: `Tk()` → `withdraw()` → configure the style → build the widgets
→ `geometry()` → `deiconify()` → confirm the style once more, with "content origin − window frame origin =
title bar height" as a regression test (`test_no_title_bar`).

### Troubleshooting: black background and text smearing

**This is a macOS regression in Tk 9.0, not a configuration problem.**

The window backing store in Tk 9.0 is opaque: it treats `systemTransparent` as "the fully transparent
colour" and fills the buffer with it, which clears nothing, so an opaque black rectangle is left behind and
old glyphs are never erased (that is the smearing). Measured on the same piece of code:

| Tk version | Background pixels | Share of transparent pixels |
| --- | --- | --- |
| **Tk 9.0** | `RGBA(0,0,0,255)` opaque black | 0% |
| **Tk 8.6** | `RGBA(0,0,0,0)` fully transparent | 87% (only the green glyphs are opaque) |

That is why the project pins `.python-version` to **3.12.7** (the last uv build bundling Tk 8.6).
To check which generation of Tk is in use:

```bash
uv run python -c "import tkinter; print('Tk', tkinter.TkVersion)"
uv run float-clock --diagnose      # also reports whether borderless / transparent took effect
```

If it prints Tk 9 and you really want to stay on Tk 9, the program warns you up front at startup that
"Tk 9 transparency is buggy".
`tests/test_overlay_smoke.py::test_background_pixels_are_really_transparent` guards the regression by
reading the actual pixels (it fails on Tk 9).

### Troubleshooting: the third line is not knocked out

- Only macOS can knock it out. Other platforms (or a missing Pillow / font file, or a failed ObjC mount)
  fall back to plain green text automatically and print the reason to stderr. **That is a designed
  degradation, not a failure.**
- To switch back to plain green text by hand: `[display] info_style = "text"`.
- To check: `uv run float-clock --diagnose` prints the third line's actual contents.

### Troubleshooting: `Can't find a usable init.tcl`

python-build-standalone places the `tcl8.6` / `tk8.6` data directories under `lib/` in the interpreter's
base prefix, and Tcl cannot find them when started from inside a venv. The program sets `TCL_LIBRARY` /
`TK_LIBRARY` automatically at import time (see `src/float_clock/tclenv.py`), so no environment variables
need to be configured by hand.
