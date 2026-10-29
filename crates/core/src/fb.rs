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
        if first.is_empty() || first == "-" { None } else { Some(first) }
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

/// Feed rows (`ref, author, time, counts, text…`) without Facebook's badge
/// text in the author, posts repeated by the feed scroller, or text the
/// page rendered twice back to back.
pub fn parse_feed(output: &str) -> Vec<Record> {
    const NOISE: [&str; 7] = [
        "Online status indicator",
        "Active",
        "Verified account",
        "Suggested for you",
        "Shared with Public",
        "Follow",
        "View more comments",
    ];
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for mut record in parse_records(output) {
        if record.fields.len() >= 5 {
            let mut text = record.fields[4..].join(" ");
            // Text repeated back to back: keep the first half.
            let flat = squash(&text);
            let half = flat.len() / 2;
            if half > 0 && flat.is_char_boundary(half) && flat[..half] == flat[half..] {
                let mut kept = 0;
                let cut = text
                    .char_indices()
                    .find(|(_, c)| {
                        if !c.is_whitespace() {
                            kept += c.len_utf8();
                        }
                        kept > half
                    })
                    .map_or(text.len(), |(i, _)| i);
                text = text[..cut].trim_end().to_owned();
            }
            let mut author = String::new();
            let mut joined = true;
            let mut parts: Vec<&str> = Vec::new();
            for part in record.fields[1].split(" / ").map(str::trim) {
                let noise = part.is_empty()
                    || NOISE.contains(&part)
                    || part.bytes().all(|b| b.is_ascii_digit())
                    || (part.len() > 30 && text.contains(part))
                    || parts.contains(&part);
                if noise {
                    continue;
                }
                parts.push(part);
                if matches!(part, "and" | "follows" | "is with" | "is in" | "is at") {
                    author.push(' ');
                    author.push_str(part);
                    joined = true;
                } else {
                    author.push_str(if author.is_empty() {
                        ""
                    } else if joined {
                        " "
                    } else {
                        " / "
                    });
                    author.push_str(part);
                    joined = false;
                }
            }
            record.fields.truncate(4);
            record.fields[1] = author;
            record.fields.push(text);
            record.raw = record.fields.join("\t");
        }
        // The same post can come back under different photo links, so text wins over the link.
        let key = feed_record_key(&record);
        if seen.insert(key) {
            rows.push(record);
        }
    }
    rows
}

/// Stable identity for a normalized feed row, including alternate photo links.
pub fn feed_record_key(record: &Record) -> String {
    let text = record.field(4).trim_end_matches("… See more").split_whitespace().collect::<String>();
    if text.is_empty() { record.raw.clone() } else { format!("{}\t{text}", record.field(1)) }
}

/// Keep notification post URLs instead of replacing them with daemon-local references.
pub fn parse_notifications(output: &str) -> Result<Vec<Record>, String> {
    let value: serde_json::Value = serde_json::from_str(output).map_err(|_| {
        "The backend returned unreadable notifications. Refresh to retry.".to_owned()
    })?;
    if let Some(error) = value["err"].as_str() {
        return Err(error.to_owned());
    }
    let rows = value["rows"].as_array().ok_or("The backend returned no notification list")?;
    rows.iter()
        .map(|row| {
            let cells = row
                .as_array()
                .filter(|cells| matches!(cells.len(), 4 | 5))
                .ok_or("The backend returned a malformed notification")?;
            let mut fields = Vec::new();
            for cell in &cells[..3] {
                fields.push(
                    cell.as_str()
                        .ok_or("The backend returned an unreadable notification field")?
                        .to_owned(),
                );
            }
            fields.push(
                cells[3]["v"]
                    .as_str()
                    .ok_or("This notification has no Facebook target")?
                    .to_owned(),
            );
            if let Some(metadata) = cells.get(4).filter(|value| value.is_object()) {
                fields.push(metadata.to_string());
            }
            Ok(Record { raw: fields.join("\t"), fields })
        })
        .collect()
}

pub fn recent_feed_args(count: u32) -> Vec<String> {
    vec![
        "posts".into(),
        "https://www.facebook.com/?filter=all&sk=h_chr".into(),
        "-n".into(),
        count.to_string(),
        "--full".into(),
        "--urls".into(),
    ]
}

/// Locates and runs the `fb` binary.
#[derive(Debug, Clone)]
pub struct Fb {
    program: PathBuf,
    engine: Option<crate::browsers::Browser>,
}

impl Fb {
    /// Resolution order: `$BLINDFB_FB`, `fb` on `PATH`, `~/.local/bin/fb`,
    /// and next to the BlindFB executable (the installers drop it there).
    pub fn discover() -> Option<Self> {
        if let Ok(path) = std::env::var("BLINDFB_FB") {
            let candidate = PathBuf::from(path);
            if candidate.is_file() {
                return Some(Self::with_program(candidate));
            }
        }
        if let Some(candidate) = crate::backend::managed_bin() {
            if candidate.is_file() {
                return Some(Self::with_program(candidate));
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in ["fb", "fb.exe"] {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return Some(Self::with_program(candidate));
                    }
                }
            }
        }
        if let Ok(found) = which("fb") {
            return Some(Self::with_program(found));
        }
        if let Some(home) = std::env::var_os("HOME") {
            let candidate = PathBuf::from(home).join(".local").join("bin").join("fb");
            if candidate.is_file() {
                return Some(Self::with_program(candidate));
            }
        }
        None
    }

    pub fn with_program(program: PathBuf) -> Self {
        let engine = if std::env::var_os("FB_CDP").is_some() {
            None
        } else {
            crate::browsers::fallback_engine(&crate::browsers::detect_browsers())
        };
        Self { program, engine }
    }

    pub fn program(&self) -> &PathBuf {
        &self.program
    }

    pub(crate) fn browser_endpoint(&self) -> Result<(u16, String), String> {
        if std::env::var_os("FB_CDP").is_some() {
            return Err("Cookie import is unavailable with a custom FB_CDP browser. Sign in directly in that browser instead.".to_owned());
        }
        if self.engine.is_some() {
            crate::engine::endpoint()
        } else {
            crate::browsers::fb_browser_endpoint()
        }
        .ok_or_else(|| "the backend browser is not running".to_owned())
    }

    /// Run `fb` with `args` and return stdout. On non-zero exit the error is
    /// the CLI's own stderr/stdout text (its messages are short and meant to
    /// be relayed, e.g. "not logged in").
    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        if self.engine.is_some() && args == ["login", "--visible"] {
            return self.login_visible();
        }
        let output = self
            .command(args)?
            .args(args)
            .output()
            .map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        let result = collect(output);
        if self.engine.is_some() && args == ["quit"] {
            crate::engine::close()?;
        }
        result
    }

    fn command(&self, args: &[&str]) -> Result<Command, String> {
        let mut command = Command::new(&self.program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        if let Some(browser) = &self.engine {
            if !matches!(args.first(), Some(&"stop" | &"quit" | &"help" | &"--version")) {
                let ((port, path), started) = crate::engine::ensure(browser, false)?;
                if started {
                    self.run(&["stop"])?;
                }
                command.env("FB_CDP", format!("ws://127.0.0.1:{port}{path}"));
            }
        }
        Ok(command)
    }

    fn login_visible(&self) -> Result<String, String> {
        let browser = self.engine.as_ref().ok_or("no dedicated browser selected")?;
        self.run(&["stop"])?;
        crate::engine::close()?;
        let (endpoint, _) = crate::engine::ensure(browser, true)?;
        let result = self.wait_for_login(&endpoint);
        self.run(&["stop"])?;
        crate::engine::close()?;
        result?;
        // Keep the saved session, then return to a browser window the user cannot close accidentally.
        self.run(&["status"])
    }

    fn wait_for_login(&self, endpoint: &(u16, String)) -> Result<(), String> {
        use crate::browsers::cdp_call;
        // Revoked cookies must not satisfy status before the user signs in again.
        cdp_call(endpoint, "Storage.clearCookies", &serde_json::json!({}))?;
        let target = cdp_call(
            endpoint,
            "Target.createTarget",
            &serde_json::json!({"url": "https://www.facebook.com/"}),
        )?;
        cdp_call(
            endpoint,
            "Target.activateTarget",
            &serde_json::json!({"targetId": target["targetId"]}),
        )?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
        let mut last_error = String::new();
        while std::time::Instant::now() < deadline {
            if crate::engine::endpoint().is_none() {
                return Err("Sign-in was cancelled: the browser window was closed".to_owned());
            }
            let status = self.run(&["status"])?;
            if crate::browsers::signed_in(&status) {
                match self.run(&["profile", "me"]) {
                    Ok(_) => return Ok(()),
                    Err(error) => {
                        last_error = error;
                        // A checkpoint can leave the daemon's expired-session flag set until restart.
                        self.run(&["stop"])?;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        Err(format!(
            "Sign-in did not finish within 15 minutes. Use Backend → Sign in to retry. {last_error}"
        ))
    }

    /// Run `fb` feeding `input` on stdin (used for `unlock -`, so a Messenger
    /// PIN never appears in a process list).
    pub fn run_stdin(&self, args: &[&str], input: &str) -> Result<String, String> {
        let mut child = self
            .command(args)?
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
        let message = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
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

pub fn feed_args(count: u32, refresh: bool, full: bool, urls: bool) -> Vec<String> {
    let mut args = vec!["feed".to_owned(), "-n".to_owned(), count.to_string()];
    if refresh {
        args.push("--refresh".to_owned());
    }
    if full {
        args.push("--full".to_owned());
    }
    if urls {
        args.push("--urls".to_owned());
    }
    args
}

pub fn posts_args(who: &str, count: u32, urls: bool) -> Vec<String> {
    let mut args = vec![
        "posts".to_owned(),
        who.to_owned(),
        "-n".to_owned(),
        count.to_string(),
        "--full".to_owned(),
    ];
    if urls {
        args.push("--urls".to_owned());
    }
    args
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
    vec![
        "eval".to_owned(),
        "https://www.facebook.com/notifications/".to_owned(),
        format!("{};window.__fb.notifs({count}).then(out=>JSON.stringify({{...out,rows:out.rows.map(row=>{{if(!row[2].toLowerCase().includes(' new reels'))return row;const metadata=window.__blindfbReels.notificationNow(new URL(row[3].v,window.location.origin).href);return metadata?[...row,metadata]:row}})}}))", include_str!("reels.js")),
    ]
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

// --- Web link-outs -------------------------------------------------------
// Features the CLI cannot do (voice/video calls, adding/removing friends,
// posting into a group) live on the web. These helpers turn the URLs and
// paths `fb` reports into browser links for exactly those places.

/// Make a full https://www.facebook.com URL from a path or URL `fb` printed.
pub fn web_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        path_or_url.to_owned()
    } else if let Some(rest) = path_or_url.strip_prefix('/') {
        format!("https://www.facebook.com/{rest}")
    } else {
        format!("https://www.facebook.com/{path_or_url}")
    }
}

/// The profile URL of a post's author, derived from the post URL that
/// `feed --urls` / `posts --urls` print. Group posts return None (their
/// first path segment is the group, not the author).
pub fn profile_url_from_post_url(post_url: &str) -> Option<String> {
    let url = web_url(post_url);
    let rest = url.strip_prefix("https://www.facebook.com/")?;
    if rest.starts_with("groups/") {
        return None;
    }
    if let Some(query) = rest.strip_prefix("permalink.php?") {
        for pair in query.split('&') {
            if let Some(id) = pair.strip_prefix("id=") {
                return Some(format!("https://www.facebook.com/profile.php?id={id}"));
            }
        }
        return None;
    }
    if let Some(query) = rest.strip_prefix("profile.php?") {
        for pair in query.split('&') {
            if let Some(id) = pair.strip_prefix("id=") {
                return Some(format!("https://www.facebook.com/profile.php?id={id}"));
            }
        }
        return None;
    }
    let vanity = rest.split('/').next()?;
    if vanity.is_empty() {
        return None;
    }
    Some(format!("https://www.facebook.com/{vanity}"))
}

/// The Messenger web thread URL for a 1:1 chat with the person behind a
/// profile URL. Voice and video calls live on that page.
pub fn thread_url_from_profile_url(profile_url: &str) -> Option<String> {
    let url = web_url(profile_url);
    let rest = url.strip_prefix("https://www.facebook.com/")?;
    if let Some(query) = rest.strip_prefix("profile.php?") {
        for pair in query.split('&') {
            if let Some(id) = pair.strip_prefix("id=") {
                return Some(format!("https://www.facebook.com/messages/t/{id}"));
            }
        }
        return None;
    }
    let vanity = rest.split('/').next()?;
    if vanity.is_empty() || vanity == "messages" || vanity == "groups" {
        return None;
    }
    Some(format!("https://www.facebook.com/messages/t/{vanity}"))
}

// --- Groups ---------------------------------------------------------------
// The backend's `groups` command (since 0.1.5-blindfb.6) reads the
// joined-groups list page itself, scrolling until it stops growing, so the
// records are exactly the groups the user belongs to: fields [name, group
// URL, privacy/size info], parsed by parse_records like every other list.
// Group posts then load through the ordinary `posts <group url>` command,
// and every post action works.

pub fn groups_args(count: u32) -> Vec<String> {
    vec!["groups".to_owned(), "-n".to_owned(), count.to_string(), "--urls".to_owned()]
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_preserve_post_urls_in_the_fourth_field() {
        let raw = r#"{"rows":[["1h","!","Someone commented",{"r":"p","v":"https://www.facebook.com/permalink.php?story_fbid=123&id=456"}]]}"#;
        let records = parse_notifications(raw).unwrap();
        assert_eq!(records[0].field(0), "1h");
        assert_eq!(
            records[0].field(3),
            "https://www.facebook.com/permalink.php?story_fbid=123&id=456"
        );
        assert!(parse_notifications(r#"{"err":"notifications unavailable"}"#).is_err());
        assert!(parse_notifications(r#"{"rows":[["1h","!","body"]]}"#).is_err());
    }

    #[test]
    fn feeds_requests_full_chronological_facebook_view() {
        assert_eq!(
            recent_feed_args(20),
            [
                "posts",
                "https://www.facebook.com/?filter=all&sk=h_chr",
                "-n",
                "20",
                "--full",
                "--urls"
            ]
        );
        assert!(notifs_args(20).iter().any(|script| script.contains("window.__fb.notifs(20)") && script.contains("notificationNow")));
    }

    #[test]
    fn feed_rows_drop_badges_repeats_and_doubled_text() {
        let out = [
            "-\tOnline status indicator / Active / Kawaling Pinoy / Verified account / Suggested for you\t\t9/1/1\tSkip takeout!",
            "-\tSteve Barclay / Shared with Public / Will David Eby go down as our FAFO premier? / 18 / View more comments / Steve Barclay\t3d\t1/0/0\tWill David Eby go down as our FAFO premier?",
            "-\tJohanne Foulds / follows / Scotiabank\t\t0/0/0\tSaving? … See more",
            "-\tAI group / Abdul\t\t0/0/0\tWhat fails? Let it.What fails? Let it.",
            "-\tC\t\t1/2/0\tI feel bad. Crying helps.",
            "-\tC\t\t1/2/0\tI feel bad.Crying helps.",
            "-\tOnline status indicator / Active / Kawaling Pinoy / Verified account / Suggested for you\t\t9/1/1\tSkip takeout!",
            "/photo/?fbid=1&set=a.9\tJeremy Smith\t\t0/0/0\tNormal people rearrange eggs",
            "/photo/?fbid=2&set=a.9\tJeremy Smith\t\t0/0/0\tNormal people rearrange eggs",
            "-\tEvan / is in / Vancouver\t\t0/0/0\tRadio Free Roscoe",
        ]
        .join("\n");
        let rows = parse_feed(&out);
        let summary: Vec<(&str, &str)> = rows.iter().map(|r| (r.field(1), r.field(4))).collect();
        assert_eq!(
            summary,
            [
                ("Kawaling Pinoy", "Skip takeout!"),
                ("Steve Barclay", "Will David Eby go down as our FAFO premier?"),
                ("Johanne Foulds follows Scotiabank", "Saving? … See more"),
                ("AI group / Abdul", "What fails? Let it."),
                ("C", "I feel bad. Crying helps."),
                ("Jeremy Smith", "Normal people rearrange eggs"),
                ("Evan is in Vancouver", "Radio Free Roscoe"),
            ]
        );
    }

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
        let args = feed_args(30, true, true, true);
        assert_eq!(args, ["feed", "-n", "30", "--refresh", "--full", "--urls"]);
    }

    #[test]
    fn profile_and_thread_urls() {
        assert_eq!(
            profile_url_from_post_url("https://www.facebook.com/jane.doe/posts/123").as_deref(),
            Some("https://www.facebook.com/jane.doe")
        );
        assert_eq!(
            profile_url_from_post_url("/permalink.php?story_fbid=9&id=42").as_deref(),
            Some("https://www.facebook.com/profile.php?id=42")
        );
        assert_eq!(profile_url_from_post_url("/groups/777/posts/1"), None);
        assert_eq!(
            thread_url_from_profile_url("https://www.facebook.com/jane.doe").as_deref(),
            Some("https://www.facebook.com/messages/t/jane.doe")
        );
        assert_eq!(
            thread_url_from_profile_url("https://www.facebook.com/profile.php?id=42").as_deref(),
            Some("https://www.facebook.com/messages/t/42")
        );
    }

    #[test]
    fn groups_command_args() {
        assert_eq!(groups_args(120), ["groups", "-n", "120", "--urls"]);
    }
}
