# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.1] - 2026-09-20

`--print` reported a negative offset as zero.

### Fixed

- **`float-clock --print` printed `(offset 00:00:00)` for a negative offset.**
  The line renders the offset with `format_hms`, which is a magnitude
  formatter - it clamps negatives to zero on purpose, because every other
  caller renders the sign itself. The diagnostic passed the signed value
  straight in, so `-00:05:00` came out as `00:00:00` while the offset moment M
  printed beside it was plainly five minutes earlier. Both callers now go
  through one `format_signed_hms`, so the sign cannot be dropped again. The
  third line was always correct; only the diagnostic lied.

  This went unnoticed because CI only checked that `--print` exits `0`, and the
  offset it passes there (`+120m`) is positive.

### Changed

- Unit tests: 73 → **74 passed**.

## [0.4.0] - 2026-09-20

Windows can now put the overlay above Task Manager, the on-screen keyboard and
the lock screen.

### Added

- **UIAccess on Windows**, so the overlay can occupy the top window band. From
  Windows 8 on, windows live in bands and `SetWindowPos(HWND_TOPMOST)` cannot
  lift one out of `ZBID_DESKTOP`, which is where an ordinary process's windows
  are created - so anything in a higher band covered the overlay no matter what.
  Reaching `ZBID_UIACCESS` needs the process itself to hold UIAccess, which
  normally requires an Authenticode signature and installation under
  `%ProgramFiles%`; the manifest route is not available to an unsigned portable
  binary, and `uiAccess="true"` on one makes it refuse to start at all. Instead
  FloatClock takes a copy of a SYSTEM process's token, sets `TokenUIAccess` and
  starts a second instance with it, then exits.

  This needs administrator rights, so Windows asks once per launch. Declining is
  fine: the overlay runs normally, just below higher-band windows. The prompt is
  skipped entirely for anything that only prints and exits, so `--print`,
  `--render-png`, `--diagnose`, `--test-notify` and `--init-config` are
  unaffected, and so is CI.

  `[window] ui_access = false` turns the whole thing off.

### Changed

- `config.example.toml` gained the `ui_access` key. A test pins it to the
  template that `--init-config` writes, so the two cannot drift apart again.

## [0.3.1] - 2026-09-20

Hiding the overlay used to be a one-way door. That, and a few smaller pieces of
sharpness, are fixed here.

### Fixed

- **Hiding the overlay made it impossible to bring back.** `eframe` runs no egui
  pass at all for an invisible window - it calls `App::logic` instead of
  `App::ui`, on purpose, so that no UI state is disturbed. Everything
  background-ish was in `ui`, including the tray menu polling, so the moment the
  overlay was hidden the tray menu item that shows it again was never even read.
  Anything that has to keep running now lives in `logic`, and `logic` also asks
  for the next repaint - without that request the event loop simply sleeps once
  the window is hidden, and the tray goes dead for the same reason.
- **Notifications and config hot reload stopped while the overlay was hidden**,
  for exactly the same reason. A hidden overlay used to mean a silent one.
- **`--probe` and `--scale` accepted nonsense.** A non-positive value reached
  `Duration::from_secs_f32`, which panics; with `panic = "abort"` in the release
  profile that took the whole process down instead of printing a usage error.

### Added

- **Crash reports.** A shipped build aborts on panic and a double-clicked launch
  throws stderr away, so a crash used to leave nothing behind - which is how
  [0.3.0] produced an unexplained `SIGABRT` with no message. A panic hook now
  writes the message, the source location, the arguments and the executable path
  to `crash.log` next to the config file.

### Changed

- **Release archives contain the program and nothing else.** No read-me, no
  example config, no wrapper folder: unpack the zip and the thing you downloaded
  is right there.

## [0.3.0] - 2026-09-20

A tray icon, a settings window you get shown on the first run, a drag that no longer shakes, and one archive format for every platform.

### Added

- **Tray icon on macOS and Windows.** macOS shows an `NSStatusItem` in the menu bar, Windows a `Shell_NotifyIcon` entry in the notification area. Both open the same menu: Hide overlay / Show overlay, a Locked checkbox, Settings…, Open config file, Show config in folder, Reload config, Quit FloatClock. There is no tray backend on Linux in this build: the only options are XEmbed (X11-only and dead on Wayland) or a D-Bus `StatusNotifierItem`, and both would pull a GTK3 or D-Bus runtime into a binary that currently links nothing but libc. `[window] tray = false` turns the icon off.
- **The first run writes a config file and opens the settings window**, so the config path is discoverable without reading any documentation.
- **The settings window is a real editor now**, not just a viewer. It edits the target time, the offset, the colour, the title size, the subtitle size, the font family and the opacity, plus Locked / Always on top / notifications checkboxes, and it has buttons to open the config file, show it in the folder and reload it from disk. It also displays the exact config path. `--settings` opens it on startup.
- **`float-clock --config-path`** prints the config file that would be used and exits.

### Changed

- **Every platform now ships one `.zip` and nothing else.** The release assets are `float-clock-macos-universal.zip`, `float-clock-linux-x86_64.zip`, `float-clock-windows-x86_64.zip` and `SHA256SUMS`; the bare binaries are no longer published at all. A browser download strips the executable bit, and Finder then treats a bare Mach-O binary as a text file and hands it to TextEdit, which reports "the text encoding Unicode (UTF-8) is not applicable". A zip records the file mode, so unpacking restores it, and one archive format for every platform removes the "which file do I download?" question.
- **Drag no longer jitters.** Dragging is handed to the window manager through `ViewportCommand::StartDrag`, so our code never sees the movement and there is no feedback loop. If the window manager does not take the drag over, the app detects within about 120 ms that the pointer is moving while the window is not and falls back to moving the window itself from an absolute anchor in monitor space.
- The third-line separator changed from `·` to `|`, so the default `info_template` is now `"{time} | {delta}"`.
- Notification bodies read `{human} until {label}`, `{label} reached at {time}` and `{label} passed {human} ago`, and durations render compactly as `45s`, `2m`, `1h 30m`, `1d 1h 1m`.
- `--diagnose` also reports the tray status, including the reason when there is no tray backend.
- The whole project is English only now: documentation, source comments, user-facing strings, notifications, config comments and workflow names. CI no longer installs a CJK font, because nothing in the repository is written in Chinese any more.
- Unit tests: 61 → **69 passed**.

## [0.2.1] - 2026-09-19

Fixed "downloaded it, double-clicked it, nothing happens", and added the icon.

### Fixed

- **On macOS the download would not open by double-clicking.** The release only had a bare binary, a browser download stripped the executable bit, and Finder then handed it to TextEdit as a text file, which reported "the text encoding Unicode (UTF-8) is not applicable". Releases now also carry `float-clock-macos-universal.zip`, containing `FloatClock.app` with its icon; unpack and double-click. The bare binary is still published for scripts, and the README documents the `chmod +x` it needs.
- **A double-clicked launch had nowhere to store its config.** The `.app` runs with `/` as its working directory, so a relative `config.toml` could neither be found nor written. The config file is now looked up in the order `--config` → `FLOAT_CLOCK_CONFIG` → the current directory → the per-user location (macOS `~/Library/Application Support/FloatClock/`, Windows `%APPDATA%\FloatClock\`, Linux `~/.config/float-clock/`), and a default one is written when none exists.
- **The Linux download lost its executable bit in the same way.** Releases now also carry `float-clock-linux-x86_64.tar.gz`.

### Added

- **The icon**: a near-black rounded square with a bright green countdown dial and a monospace `T−`, in the same palette as the overlay itself. Generated by `tools/make_icons.py`, with the PNG / ICO / ICNS output committed.
- macOS is packaged into a `.app` (`tools/make_app.sh`) with an `Info.plist`, the icon, and an ad-hoc signature.
- The Windows exe carries the icon and a version block (`build.rs` + `winresource`).
- CI smoke tests now genuinely run `--print` and `--render-png` on Linux and Windows.
- `--print` groups the upcoming reminders by moment and shows the nearest few of each, instead of letting one moment fill the whole list.

## [0.2.0] - 2026-09-19

The Rust rewrite: the first usable version.

- One codebase cross-compiles to single-file executables for macOS / Linux / Windows
- Three lines: the main title counts down to the offset moment, the subtitle counts since the target time, and the third line is drawn as a green bar with knocked-out glyphs
- T± semantics, zero-padded equal-width digits, `DD:HH:MM:SS` past a day
- Left-button drag / right-click lock / double-click settings, with the locked state shown as a solid-versus-dashed frame
- System notifications around both moments: thresholds, de-duplication, and no re-announcing of reminders that passed before startup
- Config hot reload with comment-preserving write-back
- 59 unit tests

[0.4.1]: https://github.com/bananaxiao2333/float-clock/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/bananaxiao2333/float-clock/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/bananaxiao2333/float-clock/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/bananaxiao2333/float-clock/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/bananaxiao2333/float-clock/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/bananaxiao2333/float-clock/releases/tag/v0.2.0
