//! Command-line entry point.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{Duration, Local, NaiveDateTime};

use float_clock::app::{self, RunOptions};
use float_clock::config::{self, Config};
use float_clock::notifier::MomentNotifier;
use float_clock::notify;
use float_clock::render::{self, RenderInput};
use float_clock::text::Fonts;
use float_clock::timefmt::{self, format_hms, parse_duration, parse_target};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
float-clock - a borderless, transparent T+/- countdown overlay

USAGE:
    float-clock [OPTIONS]

OPTIONS:
    --config <PATH>        use this config file (default: see --config-path)
    --config-path          print the config file that would be used, then exit
    --init-config          write a default config file, then exit
    --force                with --init-config, overwrite an existing file
    --target <WHEN>        override [time] target for this run
    --offset <DURATION>    override [time] offset for this run
    --settings             open the settings window on startup
    --print                print the current state and the upcoming reminders, then exit
    --render-png <PATH>    render the overlay to a PNG, then exit (no window)
    --scale <FACTOR>       with --render-png, the render scale (default 2)
    --now <WHEN>           with --print / --render-png, pretend it is this time
    --diagnose             print environment and render diagnostics, then exit
    --test-notify          send one test notification, then exit
    --no-transparent       force an opaque background (troubleshooting)
    --probe <SECONDS>      read the real pixels back from the GPU after N seconds
                           and print a report (troubleshooting)
    --probe-png <PATH>     with --probe, write the read-back pixels to this PNG
    -h, --help             show this help
    -V, --version          show the version

INTERACTION:
    drag with the left mouse button  ·  right-click locks / unlocks
    (solid frame = locked, dashed = draggable)
    double-click opens the settings window
    Cmd/Ctrl+L lock  ·  Cmd/Ctrl+, settings  ·  Cmd/Ctrl+R reload config
    Cmd/Ctrl+H hide or show  ·  Cmd/Ctrl+Q quit

Both moments get a system notification as they approach: T is the target time
you configure, M is T plus the offset, and the main title counts down to M while
the subtitle counts since T. See config.example.toml for every setting.
";

/// On Windows, launching from Explorer pops up a console box first, because the
/// binary is built for the console subsystem. The console subsystem is kept on
/// purpose - that way `--print` / `--diagnose` output always works - but when
/// there are no arguments at all, i.e. this is a double-click, the box is hidden
/// again before it has a chance to be seen.
#[cfg(target_os = "windows")]
fn hide_console_for_double_click() {
    const SW_HIDE: i32 = 0;
    extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[inline]
fn hide_console_for_double_click() {}

fn main() -> ExitCode {
    // First thing, so that nothing can panic before it is in place: a shipped
    // build aborts on panic and a double-clicked launch throws stderr away.
    float_clock::crash::install_hook();

    // This has to be the very first thing, before the box has time to appear.
    if std::env::args_os().len() <= 1 {
        hide_console_for_double_click();
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("[float-clock] {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Args {
    config: Option<PathBuf>,
    config_path: bool,
    init_config: bool,
    force: bool,
    target: Option<String>,
    offset: Option<String>,
    print: bool,
    render_png: Option<PathBuf>,
    scale: Option<f32>,
    now: Option<String>,
    diagnose: bool,
    test_notify: bool,
    settings: bool,
    no_transparent: bool,
    probe: Option<f32>,
    probe_png: Option<PathBuf>,
}

fn run() -> Result<(), String> {
    let args = parse_args()?;

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(config::resolve_config_path);

    if args.config_path {
        println!("{}", config_path.display());
        return Ok(());
    }

    if args.init_config {
        if config_path.exists() && !args.force {
            return Err(format!(
                "{} already exists (pass --force to overwrite it)",
                config_path.display()
            ));
        }
        config::write_default_config(&config_path, 80, 80, "", "-00:05:00")?;
        println!("wrote {}", config_path.display());
        return Ok(());
    }

    // The very first run - a double-click, most likely - has no config file yet.
    // Write one immediately, otherwise there is nowhere to store the window
    // position the moment the user drags it.
    let mut first_run = false;
    if !config_path.exists() {
        config::write_default_config(&config_path, 80, 80, "", "-00:05:00")?;
        println!("wrote a default config to {}", config_path.display());
        first_run = true;
    }

    let mut config = config::load_config(&config_path)?;
    if let Some(target) = &args.target {
        config.time.target = target.clone();
    }
    if let Some(offset) = &args.offset {
        config.time.offset = offset.clone();
    }

    if args.test_notify {
        let sound = config
            .notify
            .sound
            .then(|| config.notify.sound_name.clone());
        let sent = notify::send(
            "[T-00:00:00] FloatClock test",
            "If you can see this, system notifications work",
            sound.as_deref(),
        );
        println!("notification backend: {}", notify::backend());
        return if sent {
            Ok(())
        } else {
            Err("could not send a notification".into())
        };
    }

    let now = match &args.now {
        Some(spec) => parse_target(spec, Local::now().naive_local())?,
        None => Local::now().naive_local(),
    };

    if args.diagnose {
        return print_diagnose(&config, &config_path, now);
    }

    if args.print {
        return print_status(&config, now);
    }

    if let Some(path) = &args.render_png {
        let overlay = build_overlay(&config, now, args.scale.unwrap_or(2.0), true)?;
        std::fs::write(path, overlay.pixmap.to_png()?)
            .map_err(|e| format!("could not write the PNG: {e}"))?;
        for line in &overlay.layout.lines {
            println!(
                "{:>7.1},{:<7.1} {:>5.1}pt {:>7.1}px  {}{}",
                line.x,
                line.top,
                line.size,
                line.width,
                line.text,
                if line.knockout { "   [knockout]" } else { "" }
            );
        }
        println!(
            "wrote {} ({} x {} pixels, scale {})",
            path.display(),
            overlay.width(),
            overlay.height(),
            overlay.layout.scale
        );
        return Ok(());
    }

    // First run: open the settings window, because otherwise the only way to
    // discover how to configure this thing is to read the source.
    let open_settings =
        args.settings || (first_run && std::env::var_os("FLOAT_CLOCK_NO_SETUP").is_none());

    app::run(
        config,
        RunOptions {
            open_settings,
            force_opaque: args.no_transparent,
            probe_after: args.probe.map(std::time::Duration::from_secs_f32),
            probe_png: args.probe_png.clone(),
        },
    )
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1).peekable();
    while let Some(arg) = argv.next() {
        // Accept both --opt=value and --opt value
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let mut take = |name: &str| -> Result<String, String> {
            match &inline {
                Some(value) => Ok(value.clone()),
                None => argv
                    .next()
                    .ok_or_else(|| format!("{name} needs a value after it")),
            }
        };
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("float-clock {VERSION}");
                std::process::exit(0);
            }
            "--config" => args.config = Some(PathBuf::from(take("--config")?)),
            "--config-path" => args.config_path = true,
            "--init-config" => args.init_config = true,
            "--force" => args.force = true,
            "--target" => args.target = Some(take("--target")?),
            "--offset" => args.offset = Some(take("--offset")?),
            "--print" => args.print = true,
            "--render-png" => args.render_png = Some(PathBuf::from(take("--render-png")?)),
            "--scale" => args.scale = Some(positive_number("--scale", &take("--scale")?)?),
            "--now" => args.now = Some(take("--now")?),
            "--diagnose" => args.diagnose = true,
            "--test-notify" => args.test_notify = true,
            "--settings" => args.settings = true,
            "--no-transparent" => args.no_transparent = true,
            "--probe-png" => args.probe_png = Some(PathBuf::from(take("--probe-png")?)),
            "--probe" => args.probe = Some(positive_number("--probe", &take("--probe")?)?),
            other => return Err(format!("unrecognised argument: {other} (try --help)")),
        }
    }
    Ok(args)
}

/// Parse a strictly positive, finite number.
///
/// `--probe -1` used to reach `Duration::from_secs_f32`, which panics - and in a
/// release build a panic aborts, so a bad flag took the process down instead of
/// printing a usage error.
fn positive_number(flag: &str, text: &str) -> Result<f32, String> {
    let value: f32 = text
        .parse()
        .map_err(|_| format!("{flag} needs a number, got {text:?}"))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{flag} needs a positive number, got {text:?}"));
    }
    Ok(value)
}

fn offset_duration(offset_seconds: f64) -> Duration {
    Duration::milliseconds((offset_seconds * 1000.0).round() as i64)
}

fn moment_text(
    config: &Config,
    now: NaiveDateTime,
) -> Result<(NaiveDateTime, NaiveDateTime, f64), String> {
    let target = parse_target(&config.time.target, now)?;
    let offset = parse_duration(&config.time.offset)?;
    let mark = target + offset_duration(offset);
    Ok((target, mark, offset))
}

fn build_overlay(
    config: &Config,
    now: NaiveDateTime,
    scale: f32,
    transparent: bool,
) -> Result<render::Overlay, String> {
    let fonts = Fonts::load(&config.display.font_family)?;
    let (target, mark, _) = moment_text(config, now)?;
    render::render(&RenderInput {
        config,
        fonts: &fonts,
        target,
        mark,
        now,
        locked: config.window.locked,
        scale,
        transparent,
    })
}

fn print_status(config: &Config, now: NaiveDateTime) -> Result<(), String> {
    let (target, mark, offset) = moment_text(config, now)?;
    let overlay = build_overlay(config, now, 1.0, true)?;

    println!("now          : {}", timefmt::datetime_of(now));
    println!("target  T    : {}", timefmt::datetime_of(target));
    println!(
        "offset  M    : {}   (offset {})",
        timefmt::datetime_of(mark),
        format_hms(offset.round() as i64, true)
    );
    println!("main title   : {}", overlay.layout.lines[0].text);
    println!("subtitle     : {}", overlay.layout.lines[1].text);
    if let Some(info) = overlay.layout.lines.get(2) {
        println!(
            "third line   : {}{}",
            info.text,
            if info.knockout {
                " (knocked out of a green bar)"
            } else {
                " (plain green text)"
            }
        );
    }
    println!(
        "window size  : {} x {} pixels",
        overlay.width(),
        overlay.height()
    );
    println!(
        "lock state   : {}",
        if config.window.locked {
            "locked (solid frame)"
        } else {
            "unlocked (dashed frame)"
        }
    );
    println!("notify via   : {}", notify::backend());

    let mut notifier = MomentNotifier::new(config.notify.clone());
    notifier.arm(
        vec![
            ("Offset moment".to_string(), mark),
            ("Target time".to_string(), target),
        ],
        now,
    );
    println!("upcoming reminders:");
    // The two moments' reminders are interleaved, so simply taking the first N
    // would let one moment fill the whole list. Group by moment and show only
    // the nearest few of each.
    const PER_MOMENT: usize = 6;
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut shown = Vec::new();
    for (when, label, title) in notifier.upcoming(now, usize::MAX) {
        match seen.iter_mut().find(|(name, _)| *name == label) {
            Some((_, count)) if *count >= PER_MOMENT => continue,
            Some((_, count)) => *count += 1,
            None => seen.push((label, 1)),
        }
        shown.push((when, title));
    }
    for (when, title) in shown {
        let delta = (when - now).num_seconds().max(0);
        println!("    +{:<10} {}", format_hms(delta, true), title);
    }
    Ok(())
}

fn print_diagnose(config: &Config, path: &Path, now: NaiveDateTime) -> Result<(), String> {
    println!("float-clock  : {VERSION}");
    println!("platform     : {}", std::env::consts::OS);
    println!("config file  : {}", path.display());
    println!(
        "tray icon    : {}",
        match float_clock::tray::unavailable_reason() {
            None => "available",
            Some(reason) => reason,
        }
    );
    println!(
        "transparency : {}",
        if cfg!(target_os = "linux") {
            "needs a compositor, otherwise the [display] background fills in"
        } else {
            "per-window alpha, composited straight onto the desktop"
        }
    );
    println!("notify via   : {}", notify::backend());

    let fonts = Fonts::load(&config.display.font_family)?;
    println!(
        "monospace    : {}{}",
        fonts.primary_name,
        match &fonts.fallback_name {
            Some(name) => format!("   fallback: {name}"),
            None => "   (no fallback font found)".to_string(),
        }
    );

    let overlay = build_overlay(config, now, 1.0, true)?;
    println!(
        "render       : {} x {} pixels, {} lines",
        overlay.width(),
        overlay.height(),
        overlay.layout.lines.len()
    );
    for line in &overlay.layout.lines {
        println!(
            "    {:<8} {:>5.1}pt  {:<26} x={:.1} y={:.1} w={:.1}",
            if line.knockout { "knockout" } else { "text" },
            line.size,
            line.text,
            line.x,
            line.top,
            line.width
        );
    }
    Ok(())
}
