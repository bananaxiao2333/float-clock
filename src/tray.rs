//! Tray / menu-bar icon.
//!
//! Available on macOS (an `NSStatusItem` in the menu bar) and on Windows (a
//! `Shell_NotifyIcon` entry in the notification area). Both go through the
//! `tray-icon` crate, so the menu is built once and behaves the same way.
//!
//! Linux deliberately has no tray. The only two options there are XEmbed (X11
//! only, dead on Wayland) and a D-Bus `StatusNotifierItem`, and both would drag
//! a GTK3 or D-Bus runtime into a binary that currently links nothing but libc —
//! the whole point of this rewrite. `--tray` is accepted and ignored there; the
//! menu bar is replaced by right-click to lock, Ctrl+, for settings and Ctrl+Q
//! to quit.

/// What the user picked in the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ToggleVisible,
    ToggleLock,
    OpenSettings,
    OpenConfig,
    RevealConfig,
    ReloadConfig,
    Quit,
}

/// `None` when a tray icon can actually be created here.
///
/// This is the only place that knows which platforms have a tray backend, so
/// both the startup path and `--diagnose` ask this one question.
pub fn unavailable_reason() -> Option<&'static str> {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        None
    } else {
        Some("no tray backend for this platform")
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use super::TrayCommand;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    const ID_TOGGLE_VISIBLE: &str = "float-clock.toggle-visible";
    const ID_TOGGLE_LOCK: &str = "float-clock.toggle-lock";
    const ID_SETTINGS: &str = "float-clock.settings";
    const ID_OPEN_CONFIG: &str = "float-clock.open-config";
    const ID_REVEAL_CONFIG: &str = "float-clock.reveal-config";
    const ID_RELOAD: &str = "float-clock.reload";
    const ID_QUIT: &str = "float-clock.quit";

    /// The embedded tray artwork. The macOS variant is a black-and-alpha
    /// template image (the system recolours it for light and dark menu bars);
    /// the Windows variant is the normal green-on-dark app icon.
    #[cfg(target_os = "macos")]
    const ICON_PNG: &[u8] = include_bytes!("../assets/tray-macos.png");
    #[cfg(not(target_os = "macos"))]
    const ICON_PNG: &[u8] = include_bytes!("../assets/tray-windows.png");

    #[cfg(target_os = "macos")]
    const ICON_IS_TEMPLATE: bool = true;
    #[cfg(not(target_os = "macos"))]
    const ICON_IS_TEMPLATE: bool = false;

    /// Decode one of our own tray PNGs into the raw buffer `tray_icon::Icon`
    /// wants.
    ///
    /// The two assets are committed and produced by `tools/make_icons.py`, which
    /// always writes 8-bit RGBA, so no colour-type conversion is needed;
    /// `normalize_to_color8` stays as cheap insurance against a 16-bit source.
    /// `both_tray_assets_decode` guards that assumption so a regenerated asset
    /// in a different format fails a test rather than the running app.
    fn decode_icon(bytes: &[u8]) -> Result<Icon, String> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("tray icon header: {e}"))?;
        // `output_buffer_size` is `None` only for images past the crate's sanity
        // limit; our icons are a few hundred bytes.
        let capacity = reader
            .output_buffer_size()
            .ok_or("tray icon is too large")?;
        let mut buffer = vec![0; capacity];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|e| format!("tray icon data: {e}"))?;
        buffer.truncate(info.buffer_size());

        let expected = info.width as usize * info.height as usize * 4;
        if info.color_type != png::ColorType::Rgba || buffer.len() != expected {
            return Err(format!(
                "tray icon must be 8-bit RGBA, got {:?} at {}x{}",
                info.color_type, info.width, info.height
            ));
        }

        Icon::from_rgba(buffer, info.width, info.height).map_err(|e| format!("tray icon: {e}"))
    }

    pub struct Tray {
        // Kept alive for as long as the app runs: dropping it removes the icon.
        _icon: TrayIcon,
        visible_item: MenuItem,
        lock_item: CheckMenuItem,
    }

    impl Tray {
        pub fn new(locked: bool) -> Result<Self, String> {
            let visible_item = MenuItem::with_id(ID_TOGGLE_VISIBLE, "Hide overlay", true, None);
            let lock_item = CheckMenuItem::with_id(
                ID_TOGGLE_LOCK,
                "Locked (cannot be dragged)",
                true,
                locked,
                None,
            );
            let settings_item = MenuItem::with_id(ID_SETTINGS, "Settings…", true, None);
            let open_item = MenuItem::with_id(ID_OPEN_CONFIG, "Open config file", true, None);
            let reveal_item =
                MenuItem::with_id(ID_REVEAL_CONFIG, "Show config in folder", true, None);
            let reload_item = MenuItem::with_id(ID_RELOAD, "Reload config", true, None);
            let quit_item = MenuItem::with_id(ID_QUIT, "Quit FloatClock", true, None);

            let menu = Menu::new();
            menu.append_items(&[
                &visible_item,
                &lock_item,
                &PredefinedMenuItem::separator(),
                &settings_item,
                &open_item,
                &reveal_item,
                &reload_item,
                &PredefinedMenuItem::separator(),
                &quit_item,
            ])
            .map_err(|e| format!("tray menu: {e}"))?;

            let icon = decode_icon(ICON_PNG)?;
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .with_icon_as_template(ICON_IS_TEMPLATE)
                .with_tooltip("FloatClock")
                .with_menu_on_left_click(true)
                .with_menu_on_right_click(true)
                .build()
                .map_err(|e| format!("tray icon: {e}"))?;

            Ok(Self {
                _icon: tray,
                visible_item,
                lock_item,
            })
        }

        /// Drain every menu click that happened since the last frame.
        pub fn poll(&self) -> Vec<TrayCommand> {
            let receiver = MenuEvent::receiver();
            let mut commands = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                let id: &MenuId = event.id();
                let command = match id.0.as_str() {
                    ID_TOGGLE_VISIBLE => TrayCommand::ToggleVisible,
                    ID_TOGGLE_LOCK => TrayCommand::ToggleLock,
                    ID_SETTINGS => TrayCommand::OpenSettings,
                    ID_OPEN_CONFIG => TrayCommand::OpenConfig,
                    ID_REVEAL_CONFIG => TrayCommand::RevealConfig,
                    ID_RELOAD => TrayCommand::ReloadConfig,
                    ID_QUIT => TrayCommand::Quit,
                    _ => continue,
                };
                commands.push(command);
            }
            commands
        }

        pub fn set_locked(&self, locked: bool) {
            self.lock_item.set_checked(locked);
        }

        /// Flip the first entry between "Hide" and "Show".
        pub fn set_visible(&self, visible: bool) {
            self.visible_item.set_text(if visible {
                "Hide overlay"
            } else {
                "Show overlay"
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The icon decoder accepts 8-bit RGBA only, so both committed assets
        /// have to be exactly that. `tools/make_icons.py` is what guarantees it;
        /// this is the check that notices if that ever stops being true.
        #[test]
        fn both_tray_assets_decode() {
            for (name, bytes) in [
                (
                    "tray-macos.png",
                    &include_bytes!("../assets/tray-macos.png")[..],
                ),
                (
                    "tray-windows.png",
                    &include_bytes!("../assets/tray-windows.png")[..],
                ),
            ] {
                let icon = decode_icon(bytes)
                    .unwrap_or_else(|e| panic!("{name} should decode as a tray icon: {e}"));
                drop(icon);
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod imp {
    use super::TrayCommand;

    /// Stub so the rest of the app can call the same API unconditionally.
    pub struct Tray;

    impl Tray {
        pub fn new(_locked: bool) -> Result<Self, String> {
            Ok(Self)
        }
        pub fn poll(&self) -> Vec<TrayCommand> {
            Vec::new()
        }
        pub fn set_locked(&self, _locked: bool) {}
        pub fn set_visible(&self, _visible: bool) {}
    }
}

pub use imp::Tray;
