//! Video playback support: a managed yt-dlp turns a Facebook video or reel URL into a stream
//! the native player can open, using the backend browser's Facebook session.
//!
//! Like the backend, yt-dlp is downloaded, checksum-verified and kept current by BlindFB, so
//! the user never installs anything.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::Fb;
use crate::backend::{data_dir, expected_hash, get, sha256_hex};

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
/// Facebook changes its pages often and yt-dlp follows; refresh a copy older than this.
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Progressive MP4 (sound and picture in one file), which every native player can stream.
const FORMAT: &str = "hd/sd/best[vcodec!=none][acodec!=none]";

/// The yt-dlp release asset for this OS and CPU.
fn asset_name() -> Option<&'static str> {
    if cfg!(windows) {
        Some("yt-dlp.exe")
    } else if cfg!(target_os = "macos") {
        Some("yt-dlp_macos")
    } else if cfg!(target_os = "linux") {
        match std::env::consts::ARCH {
            "x86_64" => Some("yt-dlp_linux"),
            "aarch64" => Some("yt-dlp_linux_aarch64"),
            _ => None,
        }
    } else {
        None
    }
}

/// Where the app-managed yt-dlp lives (whether or not it exists yet).
pub fn managed_ytdlp() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("bin").join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" }))
}

fn fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < MAX_AGE)
}

/// Install yt-dlp if it is missing or older than a week. An old copy keeps working when the
/// refresh fails (offline, GitHub down). Blocking; call from a worker thread.
pub fn ensure() -> Result<PathBuf, String> {
    static INSTALL: Mutex<()> = Mutex::new(());
    let _install = INSTALL.lock().map_err(|_| "the video helper installer was interrupted")?;
    let target = managed_ytdlp().ok_or("could not find a place to install the video helper")?;
    if fresh(&target) {
        return Ok(target);
    }
    match install(&target) {
        Ok(()) => Ok(target),
        Err(_) if target.is_file() => Ok(target),
        Err(error) => Err(error),
    }
}

fn install(target: &Path) -> Result<(), String> {
    let asset = asset_name().ok_or("no video helper is available for this platform")?;
    let bytes = get(&format!("{RELEASES}{asset}"))?;
    let sums = String::from_utf8_lossy(&get(&format!("{RELEASES}SHA2-256SUMS"))?).into_owned();
    let expected = expected_hash(&sums, asset).ok_or("the video helper has no checksum")?;
    if sha256_hex(&bytes) != expected {
        return Err("the video helper download failed its checksum — not installing it".to_owned());
    }
    let dir = target.parent().ok_or("bad install path")?;
    fs::create_dir_all(dir).map_err(|e| format!("could not create the install folder: {e}"))?;
    let tmp = CookieFile::create(dir, "")?;
    fs::write(&tmp.0, &bytes).map_err(|e| format!("could not write the video helper: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp.0, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("could not make the video helper executable: {e}"))?;
    }
    fs::rename(&tmp.0, target).map_err(|e| format!("could not install the video helper: {e}"))
}

/// Only Facebook pages go to yt-dlp, which would otherwise fetch anything it is given.
pub fn is_facebook_url(url: &str) -> bool {
    [
        "https://www.facebook.com/",
        "https://facebook.com/",
        "https://m.facebook.com/",
        "https://web.facebook.com/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The session's Facebook cookies in the Netscape format yt-dlp reads.
fn netscape_cookies(cookies: &serde_json::Value) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for cookie in cookies.as_array().into_iter().flatten() {
        let text = |key: &str| cookie[key].as_str().unwrap_or("");
        let domain = text("domain");
        if !(domain.trim_start_matches('.') == "facebook.com" || domain.ends_with(".facebook.com"))
        {
            continue;
        }
        if [domain, text("path"), text("name"), text("value")]
            .iter()
            .any(|field| field.chars().any(char::is_control))
        {
            continue;
        }
        let expires = cookie["expires"].as_f64().filter(|e| *e > 0.0).map_or(0, |e| e as i64);
        let flag = |on: bool| if on { "TRUE" } else { "FALSE" };
        out.push_str(
            &[
                domain,
                flag(domain.starts_with('.')),
                text("path"),
                flag(cookie["secure"].as_bool().unwrap_or(false)),
                &expires.to_string(),
                text("name"),
                text("value"),
            ]
            .join("\t"),
        );
        out.push('\n');
    }
    out
}

/// A session cookie file that is deleted as soon as yt-dlp is done with it.
struct CookieFile(PathBuf);

impl CookieFile {
    fn create(dir: &Path, contents: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = dir.join(format!(
            "video-session-{}-{}.txt",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut handle = options
            .open(&path)
            .map_err(|e| format!("could not create a private video helper file: {e}"))?;
        let file = Self(path);
        handle
            .write_all(contents.as_bytes())
            .map_err(|e| format!("could not write the video helper file: {e}"))?;
        Ok(file)
    }
}

impl Drop for CookieFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn playable_url(success: bool, stdout: &str) -> Option<&str> {
    let mut lines = stdout.lines().map(str::trim).filter(|line| !line.is_empty());
    let url = lines.next()?;
    (success
        && lines.next().is_none()
        && url.starts_with("https://")
        && !url.chars().any(|c| c.is_control() || c.is_whitespace()))
    .then_some(url)
}

/// File redirection keeps a blocked child from filling a pipe and hanging the worker.
fn run_helper(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    let dir = std::env::temp_dir();
    let stdout = CookieFile::create(&dir, "")?;
    let stderr = CookieFile::create(&dir, "")?;
    let output_file = |file: &CookieFile| {
        fs::OpenOptions::new()
            .write(true)
            .open(&file.0)
            .map_err(|e| format!("could not capture the video helper output: {e}"))
    };
    command.stdin(Stdio::null()).stdout(output_file(&stdout)?).stderr(output_file(&stderr)?);
    let mut child =
        command.spawn().map_err(|e| format!("could not start the video helper: {e}"))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(25)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(error) => format!("could not wait for the video helper: {error}"),
                    _ => "the video helper timed out; try the video again".to_owned(),
                });
            }
        }
    };
    let read_output = |file: &CookieFile| -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        fs::File::open(&file.0)
            .and_then(|handle| handle.take(64 * 1024).read_to_end(&mut bytes))
            .map_err(|e| format!("could not read the video helper output: {e}"))?;
        Ok(bytes)
    };
    Ok(Output { status, stdout: read_output(&stdout)?, stderr: read_output(&stderr)? })
}

/// Resolve a Facebook video or reel to a stream URL for the player, with the session of the
/// browser tab `page` (the reader's own), so it is always the browser the backend really uses.
/// Blocking.
pub fn stream_url(fb: &Fb, page: &str, video: &str) -> Result<String, String> {
    if !is_facebook_url(video) {
        return Err("This video is not on Facebook, so BlindFB will not play it.".to_owned());
    }
    let program = ensure()?;
    let cookies: serde_json::Value = serde_json::from_str(&fb.run(&[
        "page",
        page,
        "cdp",
        "Network.getCookies",
        r#"{"urls":["https://www.facebook.com/"]}"#,
    ])?)
    .map_err(|_| "could not read your Facebook session for the video helper".to_owned())?;
    let dir = data_dir().ok_or("could not find BlindFB's data folder")?;
    let file = CookieFile::create(&dir, &netscape_cookies(&cookies["cookies"]))?;
    let mut command = Command::new(program);
    command
        .arg("--ignore-config")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .args(["--socket-timeout", "15", "--retries", "1", "--extractor-retries", "1"])
        .arg("--cookies")
        .arg(&file.0)
        .args(["-f", FORMAT, "-g", "--", video]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = run_helper(&mut command, Duration::from_secs(60))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(url) = playable_url(output.status.success(), &stdout) {
        return Ok(url.to_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason =
        stderr.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no video found");
    Err(format!("Facebook did not give a playable video. {}", reason.trim_start_matches("ERROR: ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_facebook_pages_reach_the_helper() {
        assert!(is_facebook_url("https://www.facebook.com/reel/1413444364110358/"));
        assert!(is_facebook_url("https://m.facebook.com/page/videos/1/"));
        assert!(!is_facebook_url("https://www.facebook.com.evil/reel/1"));
        assert!(!is_facebook_url("file:///etc/passwd"));
        assert!(!is_facebook_url("https://www.facebook.com/reel/1 --exec calc"));
    }

    #[test]
    fn writes_only_facebook_cookies_in_netscape_format() {
        let cookies = serde_json::json!([
            {"domain": ".facebook.com", "path": "/", "secure": true, "expires": 1893456000.5, "name": "c_user", "value": "1"},
            {"domain": "www.facebook.com", "path": "/", "secure": false, "expires": -1, "name": "wd", "value": "800x600"},
            {"domain": ".evil.com", "path": "/", "secure": true, "expires": 1, "name": "x", "value": "y"}
        ]);
        assert_eq!(
            netscape_cookies(&cookies),
            "# Netscape HTTP Cookie File\n.facebook.com\tTRUE\t/\tTRUE\t1893456000\tc_user\t1\nwww.facebook.com\tFALSE\t/\tFALSE\t0\twd\t800x600\n"
        );
    }

    #[test]
    fn has_a_helper_for_this_platform() {
        assert!(asset_name().is_some());
        assert!(managed_ytdlp().is_some());
    }

    #[test]
    fn cookie_fields_cannot_inject_another_cookie() {
        let cookies = serde_json::json!([
            {"domain": ".facebook.com", "path": "/", "name": "bad", "value": "x\n.evil.com\tTRUE\t/\tFALSE\t0\tx\ty"},
            {"domain": ".facebook.com", "path": "/", "name": "good", "value": "ok"}
        ]);
        assert_eq!(
            netscape_cookies(&cookies),
            "# Netscape HTTP Cookie File\n.facebook.com\tTRUE\t/\tFALSE\t0\tgood\tok\n"
        );
    }

    #[test]
    fn failed_extraction_never_returns_a_partial_stream() {
        assert!(playable_url(false, "https://video.fbcdn.net/video.mp4\n").is_none());
        assert_eq!(
            playable_url(true, "https://video.fbcdn.net/video.mp4\n"),
            Some("https://video.fbcdn.net/video.mp4")
        );
        assert!(playable_url(true, "https://video.fbcdn.net/ bad\n").is_none());
        assert!(playable_url(true, "https://one/video\nhttps://two/audio\n").is_none());
    }

    #[test]
    fn session_file_is_private_and_removed_on_drop() {
        let file = CookieFile::create(&std::env::temp_dir(), "cookies").unwrap();
        let path = file.0.clone();
        assert!(fs::OpenOptions::new().write(true).create_new(true).open(&path).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn stops_a_video_helper_that_never_finishes() {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 10"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "exec sleep 10"]);
            command
        };
        let start = std::time::Instant::now();
        assert!(
            run_helper(&mut command, Duration::from_millis(50)).unwrap_err().contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn captures_output_without_losing_a_failed_exit_status() {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/C", "echo https://video.fbcdn.net/video.mp4& exit /b 1"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "echo https://video.fbcdn.net/video.mp4; exit 1"]);
            command
        };
        let output = run_helper(&mut command, Duration::from_secs(5)).unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("https://video.fbcdn.net/video.mp4"));
    }
}
