//! BlindFB backend: a thin, GUI-free driver for the `fb` command-line tool
//! (hieu-lee/facebook-cli) plus a GitHub release check for updates.
//!
//! `fb` prints one record per line, tab separated, and gives posts, chats and
//! profiles short refs (`p3k9a`, `c8x2f`, `u1abc`) that every command accepts
//! back. Everything here is plain process spawning and line parsing so it can
//! be unit tested without a display.

pub mod fb;
pub mod update;

pub use fb::{Fb, Record, parse_records};
