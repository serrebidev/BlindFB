//! Opt-in troubleshooting log, and the report a user can send when something
//! did not work. The log records what was run and how it ended, never what was
//! read or written: no post text, messages, feed content, file names or PINs.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use patois::t;

use crate::backend::data_dir;
use crate::fb::Fb;
use crate::tf;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const REPORT_TAIL_LINES: usize = 3000;
/// Arguments that are command vocabulary, not something the user wrote.
const SAFE_WORDS: [&str; 13] = [
    "like", "love", "care", "haha", "wow", "sad", "angry", "none", "react", "reply", "public",
    "friends", "me",
];

static ENABLED: AtomicBool = AtomicBool::new(false);

fn marker() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("debug-logging-on"))
}

fn log_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("debug.log"))
}

/// Read the saved checkbox state. Call once at startup.
pub fn load() -> bool {
    let on = marker().is_some_and(|path| path.is_file());
    ENABLED.store(on, Ordering::Relaxed);
    on
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Turn logging on or off, and remember the choice across restarts.
pub fn set_enabled(on: bool) -> Result<(), String> {
    ENABLED.store(on, Ordering::Relaxed);
    let path = marker().ok_or_else(|| t("could not find BlindFB's data folder"))?;
    if on {
        let dir = path.parent().ok_or_else(|| t("could not find BlindFB's data folder"))?;
        std::fs::create_dir_all(dir)
            .map_err(|e| tf!("could not save the setting: {error}", error = e))?;
        std::fs::write(&path, "")
            .map_err(|e| tf!("could not save the setting: {error}", error = e))?;
    } else if path.is_file() {
        std::fs::remove_file(&path)
            .map_err(|e| tf!("could not save the setting: {error}", error = e))?;
    }
    append_line(&format!("{}\tdebug logging {}", now(), if on { "on" } else { "off" }));
    Ok(())
}

/// Log one backend command: sanitized arguments, duration and outcome. A
/// success records only its size, because the output is the user's Facebook.
pub fn record(args: &[&str], started: Instant, result: &Result<String, String>) {
    if !enabled() {
        return;
    }
    let outcome = match result {
        Ok(output) => format!("ok, {} lines, {} bytes", output.lines().count(), output.len()),
        Err(error) => format!("ERROR {}", one_line(error)),
    };
    append_line(&format!(
        "{}\t{} ms\t{}\t{outcome}",
        now(),
        started.elapsed().as_millis(),
        sanitize_args(args)
    ));
}

/// Log a failure the app reported that may not have come from a backend command.
pub fn note_failure(error: &str) {
    if enabled() {
        append_line(&format!("{}\tshown to user\tERROR {}", now(), one_line(error)));
    }
}

/// The command as a log line. Only the command, flags, numbers, Facebook links
/// and command vocabulary survive; anything else may be something the user wrote.
pub fn sanitize_args(args: &[&str]) -> String {
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let page_verb = index <= 2 && args.first() == Some(&"page") && arg.len() <= 12;
            if index == 0
                || page_verb
                || matches!(*arg, "-" | "-n")
                || (arg.starts_with("--") && !arg.contains(' '))
                || arg.bytes().all(|b| b.is_ascii_digit())
                || SAFE_WORDS.contains(arg)
            {
                (*arg).to_owned()
            } else if let Some(link) = facebook_link(arg) {
                link
            } else {
                format!("<{} characters>", arg.chars().count())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A Facebook link, minus any search words in its query.
fn facebook_link(arg: &str) -> Option<String> {
    let path = arg
        .strip_prefix("https://www.facebook.com")
        .or_else(|| arg.strip_prefix("https://facebook.com"))?;
    if !(path.is_empty() || path.starts_with('/') || path.starts_with('?')) || arg.contains(' ') {
        return None;
    }
    Some(if path.contains("/search") {
        format!("https://www.facebook.com{}", path.split('?').next().unwrap_or(""))
    } else {
        arg.to_owned()
    })
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

fn append_line(line: &str) {
    let Some(path) = log_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

fn tail(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(REPORT_TAIL_LINES)..].join("\n")
        }
        Err(_) => "(no file)".to_owned(),
    }
}

/// Replace long runs of digits: `fb status` prints the account's numeric id.
fn hide_account_id(text: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.len() >= 8 {
            out.push_str("<id>");
        } else {
            out.push_str(&digits);
        }
        digits.clear();
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

fn command_text(fb: &Fb, args: &[&str]) -> String {
    match fb.run(args) {
        Ok(output) => hide_account_id(output.trim()),
        Err(error) => format!("failed: {}", hide_account_id(error.trim())),
    }
}

fn backend_state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FB_STATE") {
        return Some(dir.into());
    }
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("fb"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache").join("fb"))
    }
}

fn hide_home(text: String) -> String {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"));
    match home {
        Ok(home) if !home.is_empty() => text.replace(&home, "~"),
        _ => text,
    }
}

fn report_text(fb: Option<&Fb>) -> String {
    let section = |title: &str, body: String| format!("=== {title} ===\n{body}\n\n");
    let mut report = String::from(
        "BlindFB debug report. Times are seconds since 1 January 1970.\n\
         Post text, messages, feed content, file names and PINs are not logged.\n\
         Facebook links you opened or reacted to, and error messages, are included.\n\n",
    );
    report += &section(
        "System",
        format!(
            "BlindFB {}\nPinned backend {}\nOS {} {}\nGenerated {}\nDebug logging {}",
            env!("CARGO_PKG_VERSION"),
            crate::backend::BACKEND_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            now(),
            if enabled() { "on" } else { "off" }
        ),
    );
    report += &match fb {
        Some(fb) => section(
            "Backend",
            format!(
                "Program {}\nBrowser {}\nVersion: {}\nStatus: {}",
                fb.program().display(),
                fb.browser_description(),
                command_text(fb, &["--version"]),
                command_text(fb, &["status"])
            ),
        ),
        None => section("Backend", "not installed".to_owned()),
    };
    let data = data_dir().unwrap_or_default();
    report += &section("debug.log", tail(&data.join("debug.log")));
    report += &section("startup.log", tail(&data.join("startup.log")));
    if let Some(dir) = backend_state_dir() {
        report += &section("backend daemon.log", tail(&dir.join("daemon.log")));
    }
    hide_home(report)
}

/// Write the report to `path` and return a message for the user.
pub fn write_report(fb: Option<&Fb>, path: &str) -> Result<String, String> {
    std::fs::write(path, report_text(fb))
        .map_err(|e| tf!("could not save the debug report: {error}", error = e))?;
    Ok(tf!(
        "Saved the debug report to {path}.\n\nSend that file to the developer. It holds what \
         BlindFB ran and the errors it got, plus Facebook links you opened or reacted to. It does \
         not hold your post text, messages or PIN.",
        path = path
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_writing_never_reaches_the_log() {
        let line = sanitize_args(&["post", "my private words", "--img", "C:\\Users\\me\\a.jpg"]);
        assert_eq!(line, "post <16 characters> --img <17 characters>");
        assert_eq!(sanitize_args(&["post", "-secret"]), "post <7 characters>");
        let line = sanitize_args(&["comment", "p3k9a", "hello"]);
        assert_eq!(line, "comment <5 characters> <5 characters>");
        assert_eq!(sanitize_args(&["unlock", "-"]), "unlock -");
    }

    #[test]
    fn diagnostic_arguments_stay() {
        assert_eq!(
            sanitize_args(&["react", "https://www.facebook.com/a/posts/1", "love"]),
            "react https://www.facebook.com/a/posts/1 love"
        );
        assert_eq!(sanitize_args(&["feed", "-n", "10"]), "feed -n 10");
        assert_eq!(sanitize_args(&["post", "words", "--to", "me"]), "post <5 characters> --to me");
    }

    #[test]
    fn page_scripts_and_lookalike_links_are_hidden() {
        assert_eq!(
            sanitize_args(&["page", "reader", "eval", "document.body.innerText"]),
            "page reader eval <23 characters>"
        );
        assert_eq!(
            sanitize_args(&["x", "https://www.facebook.com.evil.example/a"]),
            "x <39 characters>"
        );
        assert_eq!(
            sanitize_args(&["x", "https://www.facebook.com/search/people/?q=secret"]),
            "x https://www.facebook.com/search/people/"
        );
    }

    #[test]
    fn account_id_is_hidden_but_small_numbers_stay() {
        assert_eq!(
            hide_account_id("browser ok, tabs idle=0\nlogged in as 100012345678"),
            "browser ok, tabs idle=0\nlogged in as <id>"
        );
        assert_eq!(hide_account_id("0.1.5-blindfb.21"), "0.1.5-blindfb.21");
    }
}
