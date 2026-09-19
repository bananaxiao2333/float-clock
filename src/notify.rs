//! Cross-platform system notifications (zero dependencies, everything goes through
//! the commands the OS already ships).
//!
//! The notification icon is decided by the system based on the program that sends
//! the notification; none of the three backends exposes an API for a custom icon,
//! so no platform-specific icon handling happens here.

use std::process::{Command, Stdio};
use std::time::Duration;

/// The notification backend actually used on this platform, shown by
/// `--diagnose` / `--test-notify`.
pub fn backend() -> &'static str {
    if cfg!(target_os = "macos") {
        if which("terminal-notifier").is_some() {
            "terminal-notifier"
        } else {
            "osascript (the system credits \"Script Editor\" as the sender)"
        }
    } else if cfg!(target_os = "windows") {
        "PowerShell WinRT Toast"
    } else {
        "notify-send"
    }
}

fn which(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// `CREATE_NO_WINDOW` on Windows: without it a black PowerShell console flashes
/// up on every notification.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn quiet(command: &mut Command) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(mut child) => {
            // give it a moment to exit on its own so zombie processes do not pile up
            for _ in 0..100 {
                match child.try_wait() {
                    Ok(Some(_)) => return true,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return false,
                }
            }
            let _ = child.kill();
            true
        }
        Err(_) => false,
    }
}

fn applescript_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn send_macos(title: &str, body: &str, sound: Option<&str>) -> bool {
    if let Some(binary) = which("terminal-notifier") {
        let mut command = Command::new(binary);
        command
            .arg("-title")
            .arg(title)
            .arg("-message")
            .arg(body)
            .arg("-group")
            .arg("float-clock");
        if let Some(sound) = sound {
            command.arg("-sound").arg(sound);
        }
        return quiet(&mut command);
    }
    let Some(osascript) = which("osascript") else {
        return false;
    };
    let mut script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_string(body),
        applescript_string(title)
    );
    if let Some(sound) = sound {
        script.push_str(&format!(" sound name \"{}\"", applescript_string(sound)));
    }
    quiet(Command::new(osascript).arg("-e").arg(script))
}

const WINDOWS_PS: &str = r#"
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
"#;

fn base64_utf16_le(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn send_windows(title: &str, body: &str, _sound: Option<&str>) -> bool {
    let Some(powershell) = which("powershell").or_else(|| which("pwsh")) else {
        return false;
    };
    let encoded = base64_utf16_le(WINDOWS_PS);
    quiet(
        Command::new(powershell)
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-EncodedCommand")
            .arg(encoded)
            .env("FLOAT_CLOCK_TITLE", title)
            .env("FLOAT_CLOCK_BODY", body),
    )
}

fn send_linux(title: &str, body: &str, _sound: Option<&str>) -> bool {
    let Some(notify_send) = which("notify-send") else {
        return false;
    };
    quiet(
        Command::new(notify_send)
            .arg("-a")
            .arg("FloatClock")
            .arg("-u")
            .arg("normal")
            .arg(title)
            .arg(body),
    )
}

/// Send one system notification synchronously. Returns false on failure and
/// prints to stderr.
pub fn send(title: &str, body: &str, sound: Option<&str>) -> bool {
    let ok = if cfg!(target_os = "macos") {
        send_macos(title, body, sound)
    } else if cfg!(target_os = "windows") {
        send_windows(title, body, sound)
    } else {
        send_linux(title, body, sound)
    };
    if !ok {
        eprintln!("[float-clock] {title} — {body}");
    }
    ok
}

/// Send from a background thread so the main loop is never blocked.
pub fn send_async(title: &str, body: &str, sound: Option<&str>) {
    let title = title.to_string();
    let body = body.to_string();
    let sound = sound.map(|value| value.to_string());
    std::thread::spawn(move || {
        send(&title, &body, sound.as_deref());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        // PowerShell -EncodedCommand uses exactly UTF-16LE + base64
        assert_eq!(base64_utf16_le("A"), "QQA=");
        assert_eq!(base64_utf16_le("AB"), "QQBCAA==");
        assert_eq!(base64_utf16_le("ABC"), "QQBCAEMA");
        assert_eq!(base64_utf16_le(""), "");
        // Latin-1 range characters must encode too (UTF-16LE uses two bytes per unit)
        assert_eq!(base64_utf16_le("é"), "6QA=");
        // a non-BMP character becomes a surrogate pair, i.e. four bytes
        assert_eq!(base64_utf16_le("𝄞"), "NNge3Q==");
    }

    #[test]
    fn applescript_strings_are_escaped() {
        assert_eq!(applescript_string(r#"a"b"#), "a\\\"b");
        assert_eq!(applescript_string(r"a\b"), r"a\\b");
    }

    #[test]
    fn backend_is_named() {
        assert!(!backend().is_empty());
    }
}
