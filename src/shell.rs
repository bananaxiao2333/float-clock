//! Handing a file over to the desktop: open it, or reveal it in the file manager.
//!
//! Everything goes through the commands the platform already ships, so this stays
//! dependency-free. Used by the tray menu and by the settings window.

use std::path::Path;
use std::process::{Command, Stdio};

/// `CREATE_NO_WINDOW` on Windows: without it every `cmd` / `explorer` launch
/// flashes a console box.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Run a helper process and wait a moment for it to finish, without inheriting
/// our stdio and without leaving zombies behind.
fn run(command: &mut Command) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Open `path` in whatever the user has associated with that kind of file.
/// For a `.toml` this is normally their text editor.
pub fn open(path: &Path) -> Result<(), String> {
    let path = path.as_os_str();

    #[cfg(target_os = "macos")]
    {
        run(Command::new("open").arg("-t").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        // `start` is a cmd builtin, hence the extra `""` (it is the window title).
        run(Command::new("cmd").arg("/C").arg("start").arg("").arg(path))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(Command::new("xdg-open").arg(path))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    {
        let _ = path;
        Err("opening files is not supported on this platform".to_string())
    }
}

/// Open the file manager with `path` selected, so the user can see where the
/// file actually lives.
pub fn reveal(path: &Path) -> Result<(), String> {
    let path = path.as_os_str();

    #[cfg(target_os = "macos")]
    {
        run(Command::new("open").arg("-R").arg(path))
    }

    #[cfg(target_os = "windows")]
    {
        // `/select,` needs the path glued to the comma, as one argument.
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        run(Command::new("explorer").arg(arg))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let parent = Path::new(path).parent().unwrap_or(Path::new("."));
        run(Command::new("xdg-open").arg(parent))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
    {
        let _ = path;
        Err("revealing files is not supported on this platform".to_string())
    }
}
