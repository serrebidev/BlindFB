//! BlindFB backend: a thin, GUI-free driver for the `fb` command-line tool
//! (hieu-lee/facebook-cli).
//!
//! `fb` prints one record per line, tab separated, and gives posts, chats and
//! profiles short refs (`p3k9a`, `c8x2f`, `u1abc`) that every command accepts
//! back. Everything here is plain process spawning and line parsing so it can
//! be unit tested without a display.

pub mod alerts;
pub mod backend;
pub mod browsers;
pub mod debug;
mod engine;
pub mod fb;
pub mod i18n;
pub mod http;
pub mod labels;
pub mod media;
pub mod page_errors;
pub mod mpv;
pub mod posts;
pub mod reels;
pub mod tray_hotkey;

pub use fb::{parse_records, Fb, Record};
