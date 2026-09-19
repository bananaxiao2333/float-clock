//! FloatClock — a borderless, transparent, always-on-top T± countdown overlay.
//!
//! The whole overlay is rasterised into a single RGBA [`pixmap::Pixmap`] by this
//! crate; the window layer (`app`) only paints that image and forwards input.
//! That is what makes the result pixel-identical on macOS, Linux and Windows.

pub mod app;
pub mod config;
pub mod macos;
pub mod notifier;
pub mod notify;
pub mod pixmap;
pub mod render;
pub mod shell;
pub mod text;
pub mod timefmt;
pub mod tray;
