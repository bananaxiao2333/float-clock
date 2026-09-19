//! Last-resort crash reporting.
//!
//! The release profile builds with `panic = "abort"`, and a launch from Finder
//! or Explorer discards stderr entirely - so a panic in a shipped build leaves
//! nothing behind at all. That is exactly what happened with the 0.3.0 crash
//! report: a `SIGABRT` with no symbol names and no message.
//!
//! A panic hook still runs before the abort, which is enough to write the
//! message and the location somewhere findable.

use std::io::Write;

/// Name of the report inside the per-user config directory.
pub const REPORT_NAME: &str = "crash.log";

/// Install the hook. Call this before anything else can panic.
pub fn install_hook() {
    // Keep whatever was there: a panic inside a test harness should keep
    // behaving like a test harness.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The type of `info` is left to inference on purpose: naming it means
        // naming `PanicHookInfo`, which only exists from Rust 1.81, and this
        // crate supports 1.80.
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let report = format_report(info.location(), &payload);
        // stderr first: in a terminal this is still the most useful place.
        eprintln!("{report}");
        if let Some(path) = report_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = file.write_all(report.as_bytes());
                eprintln!("[float-clock] wrote a crash report to {}", path.display());
            }
        }
        previous(info);
    }));
}

/// Where the report is written: beside the config file, so it can be found
/// without knowing where the program itself lives.
pub fn report_path() -> Option<std::path::PathBuf> {
    crate::config::user_config_dir().map(|dir| dir.join(REPORT_NAME))
}

fn format_report(location: Option<&std::panic::Location<'_>>, payload: &str) -> String {
    use std::fmt::Write as _;

    let location = location
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown location".to_string());

    let mut out = String::new();
    let _ = writeln!(
        out,
        "--- float-clock {} panic ---",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(
        out,
        "when    : {}",
        crate::timefmt::datetime_of(chrono::Local::now().naive_local())
    );
    let _ = writeln!(out, "where   : {location}");
    let _ = writeln!(out, "message : {payload}");
    let _ = writeln!(
        out,
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Ok(exe) = std::env::current_exe() {
        let _ = writeln!(out, "exe     : {}", exe.display());
    }
    let _ = writeln!(out, "args    : {:?}", std::env::args().collect::<Vec<_>>());
    let _ = writeln!(out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_message_and_the_place() {
        let report = format_report(None, "something went wrong");
        assert!(report.contains("something went wrong"));
        assert!(report.contains("unknown location"));
        assert!(report.contains(env!("CARGO_PKG_VERSION")));
        assert!(report.contains(std::env::consts::OS));
    }

    #[test]
    fn the_report_path_sits_beside_the_config() {
        let path = report_path().expect("the test machine should have a config dir");
        assert_eq!(path.file_name().unwrap(), REPORT_NAME);
        assert_eq!(path.parent(), crate::config::user_config_dir().as_deref());
    }
}
