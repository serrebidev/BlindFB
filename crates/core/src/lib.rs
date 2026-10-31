//! BlindFB backend: a thin, GUI-free driver for the `fb` command-line tool
//! (hieu-lee/facebook-cli).
//!
//! `fb` prints one record per line, tab separated, and gives posts, chats and
//! profiles short refs (`p3k9a`, `c8x2f`, `u1abc`) that every command accepts
//! back. Everything here is plain process spawning and line parsing so it can
//! be unit tested without a display.

pub mod backend;
pub mod browsers;
mod engine;
pub mod fb;
pub mod http;
pub mod media;
pub mod mpv;
pub mod posts;
pub mod reels;

pub use fb::{parse_records, Fb, Record};
