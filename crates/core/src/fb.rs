//! The `fb` CLI driver.
//!
//! Command surface (from `fb help`): feed, posts, view, profile, friends,
//! search, notifs, chats, read, send, unsend, unlock, post, comment, react,
//! delete, batch, login, status, stop, quit. Voice/video calls are not part
//! of the CLI and therefore not part of BlindFB.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// One line of `fb` output: the tab-separated fields plus the raw line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub fields: Vec<String>,
    pub raw: String,
}

impl Record {
    pub fn field(&self, index: usize) -> &str {
        self.fields.get(index).map_or("", String::as_str)
    }

    /// The short ref (`p3k9a`, `c8x2f`, `u1abc`) a record is addressed by, if
    /// it has one. Some feed lines carry `-` instead of a ref; those are
    /// display-only rows (ads and suggestions without a permalink yet).
    pub fn refr(&self) -> Option<&str> {
        let first = self.field(0).trim();
        if first.is_empty() || first == "-" {
            None
        } else {
            Some(first)
        }
    }
}

/// Split `fb` output into records, one per non-empty line.
pub fn parse_records(output: &str) -> Vec<Record> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Record {
            fields: line.split('\t').map(str::to_owned).collect(),
            raw: line.to_owned(),
        })
        .collect()
}

/// Locates and runs the `fb` binary.
#[derive(Debug, Clone)]
pub struct Fb {
    program: PathBuf,
}

impl Fb {
    /// Resolution order: `$BLINDFB_FB`, `fb` on `PATH`, `~/.local/bin/fb`,
    /// and next to the BlindFB executable (the installers drop it there).
    pub fn discover() -> Option<Self> {
        if let Ok(path) = std::env::var("BLINDFB_FB") {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return Some(Self { program: candidate });
            }
        }
        if let Ok(found) = which("fb") {
            return Some(Self { program: found });
        }
        if let Some(home) = std::env::var_os("HOME") {
            let candidate = PathBuf::from(home).join(".local").join("bin").join("fb");
            if candidate.is_file() {
                return Some(Self { program: candidate });
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in ["fb", "fb.exe"] {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(Self { program: candidate });
                    }
                }
            }
        }
        None
    }

    pub fn with_program(program: PathBuf) -> Self {
        Self { program }
    }

    pub fn program(&self) -> &PathBuf {
        &self.program
    }

    /// Run `fb` with `args` and return stdout. On non-zero exit the error is
    /// the CLI's own stderr/stdout text (its messages are short and meant to
    /// be relayed, e.g. "not logged in").
    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.program)
            .args(args)
            .output()
            .map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        collect(output)
    }

    /// Run `fb` feeding `input` on stdin (used for `unlock -`, so a Messenger
    /// PIN never appears in a process list).
    pub fn run_stdin(&self, args: &[&str], input: &str) -> Result<String, String> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
        let output = child
            .wait_with_output()
            .map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        collect(output)
    }
}

fn collect(output: std::process::Output) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(if message.is_empty() {
            format!("fb exited with {}", output.status)
        } else {
            message.to_owned()
        })
    }
}

fn which(name: &str) -> Result<PathBuf, ()> {
    let path_var = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
        let candidate = dir.join(format!("{name}.exe"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(())
}

// --- Command builders: one per `fb` subcommand BlindFB exposes. ---

pub fn feed_args(count: u32, refresh: bool, full: bool) -> Vec<String> {
    let mut args = vec!["feed".to_owned(), "-n".to_owned(), count.to_string()];
    if refresh {
        args.push("--refresh".to_owned());
    }
    if full {
        args.push("--full".to_owned());
    }
    args
}

pub fn posts_args(who: &str, count: u32) -> Vec<String> {
    vec!["posts".to_owned(), who.to_owned(), "-n".to_owned(), count.to_string()]
}

pub fn view_args(post: &str, comments: u32) -> Vec<String> {
    vec!["view".to_owned(), post.to_owned(), "-n".to_owned(), comments.to_string()]
}

pub fn profile_args(who: &str) -> Vec<String> {
    vec!["profile".to_owned(), who.to_owned()]
}

pub fn friends_args(who: Option<&str>, count: u32) -> Vec<String> {
    let mut args = vec!["friends".to_owned()];
    if let Some(who) = who {
        args.push(who.to_owned());
    }
    args.push("-n".to_owned());
    args.push(count.to_string());
    args
}

pub fn search_args(query: &str, kind: Option<&str>, count: u32) -> Vec<String> {
    let mut args = vec!["search".to_owned(), query.to_owned()];
    if let Some(kind) = kind {
        args.push("--type".to_owned());
        args.push(kind.to_owned());
    }
    args.push("-n".to_owned());
    args.push(count.to_string());
    args
}

pub fn notifs_args(count: u32) -> Vec<String> {
    vec!["notifs".to_owned(), "-n".to_owned(), count.to_string()]
}

pub fn chats_args(count: u32, unread_only: bool) -> Vec<String> {
    let mut args = vec!["chats".to_owned(), "-n".to_owned(), count.to_string()];
    if unread_only {
        args.push("--unread".to_owned());
    }
    args
}

pub fn read_args(chat: &str, count: u32) -> Vec<String> {
    vec!["read".to_owned(), chat.to_owned(), "-n".to_owned(), count.to_string()]
}

pub fn send_args(chat: &str, text: &str, files: &[String]) -> Vec<String> {
    let mut args = vec!["send".to_owned(), chat.to_owned(), text.to_owned()];
    for file in files {
        args.push("--file".to_owned());
        args.push(file.clone());
    }
    args
}

pub fn unsend_args(chat: &str, match_text: Option<&str>) -> Vec<String> {
    let mut args = vec!["unsend".to_owned(), chat.to_owned()];
    if let Some(text) = match_text {
        args.push("--match".to_owned());
        args.push(text.to_owned());
    }
    args
}

pub fn post_args(text: &str, audience: Option<&str>, images: &[String]) -> Vec<String> {
    let mut args = vec!["post".to_owned(), text.to_owned()];
    for image in images {
        args.push("--img".to_owned());
        args.push(image.clone());
    }
    if let Some(audience) = audience {
        args.push("--to".to_owned());
        args.push(audience.to_owned());
    }
    args
}

pub fn comment_args(post: &str, text: &str) -> Vec<String> {
    vec!["comment".to_owned(), post.to_owned(), text.to_owned()]
}

pub fn react_args(post: &str, reaction: &str) -> Vec<String> {
    vec!["react".to_owned(), post.to_owned(), reaction.to_owned()]
}

pub fn delete_args(post: &str) -> Vec<String> {
    vec!["delete".to_owned(), post.to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_feed_lines_and_refs() {
        let out = "p3k9a\tAlice\t5 min\t6/2/1\tHello world\n-\tAd Page\t\t1/0/0\tBuy things\n\n";
        let records = parse_records(out);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].refr(), Some("p3k9a"));
        assert_eq!(records[0].field(1), "Alice");
        assert_eq!(records[1].refr(), None);
    }

    #[test]
    fn builds_send_with_files() {
        let args = send_args("Weekend plans", "on my way", &["a.pdf".to_owned()]);
        assert_eq!(args, ["send", "Weekend plans", "on my way", "--file", "a.pdf"]);
    }

    #[test]
    fn builds_feed_refresh_full() {
        let args = feed_args(30, true, true);
        assert_eq!(args, ["feed", "-n", "30", "--refresh", "--full"]);
    }
}
