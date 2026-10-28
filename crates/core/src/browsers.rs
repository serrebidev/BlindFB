//! Sign-in browsers: import the user's existing Facebook session from the
//! browser they actually use, not just Chrome.
//!
//! How each family works:
//! - Chromium (Chrome, Chromium, Brave, Edge): every Chromium browser can
//!   expose the same remote-debugging endpoint `fb` already uses for Chrome
//!   (a `DevToolsActivePort` file in its user-data dir, gated by the user's
//!   "Allow remote debugging" consent). `fb` only enumerates Chrome and
//!   Chromium, so for Brave and Edge BlindFB performs fb's own steps —
//!   enable the setting in `Local State`, open the inspect page, wait for
//!   the endpoint — and then hands the port to `fb login --from <port>`.
//! - Firefox: no debugging-endpoint import exists, so BlindFB reads the
//!   Facebook cookies out of the Firefox profile's `cookies.sqlite`
//!   (values are stored in clear text) and sets them in the backend's own
//!   browser over CDP (`Storage.setCookies`, the same call fb's importer
//!   makes), first through fb's page escape hatch, then directly against
//!   the backend browser's endpoint.
//!
//! Missing browsers are installable: [`install`] downloads the vendor's
//! official installer (Windows, macOS). That also covers the backend's own
//! hard requirement — fb cannot run at all without a Chrome/Chromium.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::fb::Fb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Chrome,
    Chromium,
    Brave,
    Edge,
    Firefox,
}

impl BrowserKind {
    pub fn name(self) -> &'static str {
        match self {
            BrowserKind::Chrome => "Google Chrome",
            BrowserKind::Chromium => "Chromium",
            BrowserKind::Brave => "Brave",
            BrowserKind::Edge => "Microsoft Edge",
            BrowserKind::Firefox => "Firefox",
        }
    }

    pub fn is_chromium(self) -> bool {
        !matches!(self, BrowserKind::Firefox)
    }

    /// The browser's own inspect page, for ticking "Allow remote debugging".
    fn inspect_url(self) -> &'static str {
        match self {
            BrowserKind::Brave => "brave://inspect/#remote-debugging",
            BrowserKind::Edge => "edge://inspect/#remote-debugging",
            _ => "chrome://inspect/#remote-debugging",
        }
    }

    /// Process image names (Windows) / `pgrep -x` names (Unix) for "is it
    /// running" checks.
    fn process_names(self) -> &'static [&'static str] {
        match self {
            BrowserKind::Chrome => &["chrome.exe", "chrome"],
            BrowserKind::Chromium => &["chromium.exe", "chromium"],
            BrowserKind::Brave => &["brave.exe", "brave"],
            BrowserKind::Edge => &["msedge.exe", "msedge", "microsoft-edge"],
            BrowserKind::Firefox => &["firefox.exe", "firefox", "firefox-esr"],
        }
    }
}

/// A browser found on this machine.
#[derive(Debug, Clone)]
pub struct Browser {
    pub kind: BrowserKind,
    /// User-data dir (Chromium kinds) or the profile root (Firefox).
    pub data_dir: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub is_default: bool,
}

impl Browser {
    pub fn name(&self) -> &'static str {
        self.kind.name()
    }
}

fn home() -> PathBuf {
    std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from)
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var).ok().filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Locate an executable on PATH (a tiny `which`, so core stays dep-light).
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Candidate locations for one browser kind on this OS:
/// (user-data dir, Firefox profile root, executable paths in order).
struct Spec {
    kind: BrowserKind,
    data_dir: Option<PathBuf>,
    exe_candidates: Vec<PathBuf>,
    exe_names: Vec<&'static str>,
}

fn specs() -> Vec<Spec> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        let la = env_path("LOCALAPPDATA").unwrap_or_default();
        let ad = env_path("APPDATA").unwrap_or_default();
        let pf = env_path("ProgramFiles").unwrap_or_default();
        let pf86 = env_path("ProgramFiles(x86)").unwrap_or_default();
        out.push(Spec {
            kind: BrowserKind::Chrome,
            data_dir: Some(la.join("Google/Chrome/User Data")),
            exe_candidates: vec![
                pf.join("Google/Chrome/Application/chrome.exe"),
                pf86.join("Google/Chrome/Application/chrome.exe"),
                la.join("Google/Chrome/Application/chrome.exe"),
            ],
            exe_names: vec!["chrome.exe"],
        });
        out.push(Spec {
            kind: BrowserKind::Brave,
            data_dir: Some(la.join("BraveSoftware/Brave-Browser/User Data")),
            exe_candidates: vec![
                la.join("BraveSoftware/Brave-Browser/Application/brave.exe"),
                pf.join("BraveSoftware/Brave-Browser/Application/brave.exe"),
                pf86.join("BraveSoftware/Brave-Browser/Application/brave.exe"),
            ],
            exe_names: vec!["brave.exe"],
        });
        out.push(Spec {
            kind: BrowserKind::Edge,
            data_dir: Some(la.join("Microsoft/Edge/User Data")),
            exe_candidates: vec![
                pf86.join("Microsoft/Edge/Application/msedge.exe"),
                pf.join("Microsoft/Edge/Application/msedge.exe"),
            ],
            exe_names: vec!["msedge.exe"],
        });
        out.push(Spec {
            kind: BrowserKind::Firefox,
            data_dir: Some(ad.join("Mozilla/Firefox")),
            exe_candidates: vec![
                pf.join("Mozilla Firefox/firefox.exe"),
                pf86.join("Mozilla Firefox/firefox.exe"),
            ],
            exe_names: vec!["firefox.exe"],
        });
    }
    #[cfg(target_os = "macos")]
    {
        let support = home().join("Library/Application Support");
        let app = |name: &str, bin: &str| -> Vec<PathBuf> {
            vec![
                PathBuf::from(format!("/Applications/{name}.app/Contents/MacOS/{bin}")),
                home().join(format!("Applications/{name}.app/Contents/MacOS/{bin}")),
            ]
        };
        out.push(Spec {
            kind: BrowserKind::Chrome,
            data_dir: Some(support.join("Google/Chrome")),
            exe_candidates: app("Google Chrome", "Google Chrome"),
            exe_names: vec![],
        });
        out.push(Spec {
            kind: BrowserKind::Brave,
            data_dir: Some(support.join("BraveSoftware/Brave-Browser")),
            exe_candidates: app("Brave Browser", "Brave Browser"),
            exe_names: vec![],
        });
        out.push(Spec {
            kind: BrowserKind::Edge,
            data_dir: Some(support.join("Microsoft Edge")),
            exe_candidates: app("Microsoft Edge", "Microsoft Edge"),
            exe_names: vec![],
        });
        out.push(Spec {
            kind: BrowserKind::Chromium,
            data_dir: Some(support.join("Chromium")),
            exe_candidates: app("Chromium", "Chromium"),
            exe_names: vec![],
        });
        out.push(Spec {
            kind: BrowserKind::Firefox,
            data_dir: Some(support.join("Firefox")),
            exe_candidates: app("Firefox", "Firefox"),
            exe_names: vec![],
        });
    }
    #[cfg(target_os = "linux")]
    {
        let config = env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"));
        let push = |out: &mut Vec<Spec>, kind, dirs: Vec<PathBuf>, names: Vec<&'static str>| {
            out.push(Spec { kind, data_dir: dirs.into_iter().next(), exe_candidates: vec![], exe_names: names });
        };
        push(
            &mut out,
            BrowserKind::Chrome,
            vec![config.join("google-chrome")],
            vec!["google-chrome", "google-chrome-stable"],
        );
        push(
            &mut out,
            BrowserKind::Brave,
            vec![config.join("BraveSoftware/Brave-Browser")],
            vec!["brave", "brave-browser"],
        );
        push(
            &mut out,
            BrowserKind::Edge,
            vec![config.join("microsoft-edge")],
            vec!["microsoft-edge", "microsoft-edge-stable"],
        );
        push(
            &mut out,
            BrowserKind::Chromium,
            vec![config.join("chromium")],
            vec!["chromium", "chromium-browser"],
        );
        out.push(Spec {
            kind: BrowserKind::Firefox,
            data_dir: Some(home().join(".mozilla/firefox")),
            exe_candidates: vec![],
            exe_names: vec!["firefox", "firefox-esr"],
        });
    }
    out
}

/// Every browser installed on this machine, default browser flagged.
pub fn detect_browsers() -> Vec<Browser> {
    let default = default_browser_kind();
    let mut found = Vec::new();
    for spec in specs() {
        let exe = spec
            .exe_candidates
            .iter()
            .find(|p| p.is_file())
            .cloned()
            .or_else(|| spec.exe_names.iter().find_map(|name| on_path(name)));
        // A Chromium data dir counts once it holds a Local State file (the
        // browser ran at least once); Firefox once its profile root exists.
        let data_dir = spec.data_dir.filter(|dir| {
            if spec.kind == BrowserKind::Firefox {
                dir.is_dir()
            } else {
                dir.join("Local State").is_file()
            }
        });
        if exe.is_some() || data_dir.is_some() {
            found.push(Browser {
                is_default: default == Some(spec.kind),
                kind: spec.kind,
                data_dir,
                exe,
            });
        }
    }
    found
}

/// Map a Windows https-handler ProgId to a browser kind.
#[cfg(any(windows, test))]
fn kind_from_progid(progid: &str) -> Option<BrowserKind> {
    let p = progid.trim();
    if p.starts_with("Brave") {
        Some(BrowserKind::Brave)
    } else if p.starts_with("MSEdge") {
        Some(BrowserKind::Edge)
    } else if p.starts_with("Firefox") {
        Some(BrowserKind::Firefox)
    } else if p.starts_with("Chromium") {
        Some(BrowserKind::Chromium)
    } else if p.starts_with("Chrome") {
        Some(BrowserKind::Chrome)
    } else {
        None
    }
}

/// Map a desktop-file id or bundle id fragment to a browser kind. Checks run
/// most specific first ("microsoft-edge" before "chrome", etc.).
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn kind_from_hint(hint: &str) -> Option<BrowserKind> {
    let h = hint.to_lowercase();
    if h.contains("brave") {
        Some(BrowserKind::Brave)
    } else if h.contains("edge") {
        Some(BrowserKind::Edge)
    } else if h.contains("firefox") {
        Some(BrowserKind::Firefox)
    } else if h.contains("chromium") {
        Some(BrowserKind::Chromium)
    } else if h.contains("chrome") {
        Some(BrowserKind::Chrome)
    } else {
        None
    }
}

/// The operating system's default browser, when we can tell.
pub fn default_browser_kind() -> Option<BrowserKind> {
    #[cfg(windows)]
    {
        let output = Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
                "/v",
                "ProgId",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let progid = text.lines().find_map(|line| {
            let mut parts = line.split("REG_SZ");
            parts.next()?;
            parts.next().map(str::trim)
        })?;
        return kind_from_progid(progid);
    }
    #[cfg(target_os = "macos")]
    {
        let plist = home().join("Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist");
        let output = Command::new("plutil").args(["-convert", "xml1", "-o", "-", plist.to_str()?]).output().ok()?;
        let xml = String::from_utf8_lossy(&output.stdout);
        // Find the handler dict for the https scheme and read the bundle id
        // following its LSHandlerRoleAll key.
        for dict in xml.split("<dict>") {
            if dict.contains("LSHandlerURLScheme") && dict.contains("<string>https</string>") {
                if let Some(pos) = dict.find("LSHandlerRoleAll") {
                    let rest = &dict[pos..];
                    if let Some(start) = rest.find("<string>") {
                        let after = &rest[start + "<string>".len()..];
                        if let Some(end) = after.find("</string>") {
                            return kind_from_hint(&after[..end]);
                        }
                    }
                }
            }
        }
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("xdg-settings").args(["get", "default-web-browser"]).output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        return kind_from_hint(text.trim());
    }
    #[allow(unreachable_code)]
    None
}

/// Parse a `DevToolsActivePort` file: port on line 1, ws path on line 2.
/// This is the file every Chromium browser writes while its remote
/// debugging endpoint is live.
pub fn parse_active_port(text: &str) -> Option<(u16, String)> {
    let mut lines = text.lines();
    let port: u16 = lines.next()?.trim().parse().ok()?;
    let path = lines.next().unwrap_or("").trim().to_owned();
    Some((port, path))
}

pub fn devtools_endpoint(data_dir: &Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(data_dir.join("DevToolsActivePort")).ok()?;
    parse_active_port(&text)
}

fn browser_running(kind: BrowserKind) -> bool {
    for name in kind.process_names() {
        #[cfg(windows)]
        {
            if let Ok(output) = Command::new("tasklist")
                .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
                .output()
            {
                if String::from_utf8_lossy(&output.stdout).contains(name) {
                    return true;
                }
            }
        }
        #[cfg(unix)]
        {
            if Command::new("pgrep").args(["-x", name]).output().is_ok_and(|o| o.status.success()) {
                return true;
            }
        }
    }
    false
}

/// Set `devtools.remote_debugging.user-enabled` in a Chromium browser's
/// `Local State`, backing the file up first — the same careful edit fb
/// makes for Chrome. Only takes effect on the browser's next start.
pub fn enable_remote_debugging(data_dir: &Path) -> Result<(), String> {
    let path = data_dir.join("Local State");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    root["devtools"]["remote_debugging"]["user-enabled"] = serde_json::Value::Bool(true);
    std::fs::write(data_dir.join("Local State.blindfb-backup"), &text)
        .map_err(|e| format!("cannot back up Local State: {e}"))?;
    let tmp = data_dir.join("Local State.blindfb-tmp");
    let write = std::fs::write(&tmp, serde_json::to_string(&root).map_err(|e| e.to_string())?)
        .and_then(|()| std::fs::rename(&tmp, &path));
    write.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot update Local State: {e}")
    })
}

fn launch_url(browser: &Browser, url: &str) -> Result<(), String> {
    let exe = browser.exe.clone().ok_or_else(|| format!("{} is not installed", browser.name()))?;
    Command::new(&exe)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start {}: {e}", browser.name()))
}

fn wait_endpoint(data_dir: &Path, secs: u64) -> Option<(u16, String)> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if let Some(endpoint) = devtools_endpoint(data_dir) {
            return Some(endpoint);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    None
}

pub fn signed_in(status: &str) -> bool {
    status.contains("logged in") && !status.contains("not logged in")
}

fn status(fb: &Fb) -> Result<String, String> {
    fb.run(&["status"])
}

/// Import the Facebook session from a Chromium browser fb does not
/// enumerate itself (Brave, Edge; also works for Chrome/Chromium data
/// dirs). Mirrors fb's guided Chrome flow, then `fb login --from <port>`.
pub fn import_chromium(fb: &Fb, kind: BrowserKind) -> Result<String, String> {
    let browsers = detect_browsers();
    let browser = browsers
        .iter()
        .find(|b| b.kind == kind)
        .ok_or_else(|| format!("{} is not installed", kind.name()))?;
    let data_dir = browser
        .data_dir
        .clone()
        .ok_or_else(|| format!("{} has not run yet — open it once, sign in to Facebook there, then try again", kind.name()))?;

    let endpoint = if let Some(endpoint) = devtools_endpoint(&data_dir) {
        Some(endpoint)
    } else if browser_running(kind) {
        // Running without an endpoint: open the inspect page so the user
        // can tick "Allow remote debugging for this browser instance",
        // then wait for the endpoint file to appear (fb waits 240s too).
        launch_url(browser, kind.inspect_url())?;
        wait_endpoint(&data_dir, 240)
    } else {
        // Not running: switch the setting on in Local State, launch the
        // browser, and wait for the endpoint (fb waits 40s here).
        enable_remote_debugging(&data_dir)?;
        launch_url(browser, "about:blank")?;
        wait_endpoint(&data_dir, 40)
    };
    let (port, _) = endpoint.ok_or_else(|| {
        format!(
            "{} did not offer a debugging endpoint — in {}, tick \"Allow remote debugging for this browser instance\", then try again",
            kind.name(),
            kind.inspect_url()
        )
    })?;
    let port = port.to_string();
    let _ = fb.run(&["login", "--from", &port]);
    let status = status(fb)?;
    if signed_in(&status) {
        Ok(status)
    } else {
        Err(format!("{} is not signed in to Facebook — sign in there first, then try again", kind.name()))
    }
}

// ---------------------------------------------------------------- Firefox

/// One cookie row from Firefox's `moz_cookies`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfCookie {
    pub name: String,
    pub value: String,
    pub host: String,
    pub path: String,
    pub expiry: i64,
    pub secure: bool,
    pub http_only: bool,
}

/// The default Firefox profile dir inside the profile root, following
/// installs.ini (per-install default) then profiles.ini (Default=1),
/// then any profile dir holding a cookies database.
pub fn firefox_profile(root: &Path) -> Option<PathBuf> {
    let resolve = |raw: &str| -> Option<PathBuf> {
        let rel = raw.trim().replace('\\', "/");
        let candidate = if Path::new(&rel).is_absolute() { PathBuf::from(&rel) } else { root.join(&rel) };
        if candidate.is_dir() { Some(candidate) } else { None }
    };
    if let Ok(text) = std::fs::read_to_string(root.join("installs.ini")) {
        let mut is_relative = true;
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("IsRelative=") {
                is_relative = value != "0";
            }
            if let Some(value) = line.strip_prefix("Default=") {
                if is_relative {
                    if let Some(found) = resolve(value) {
                        return Some(found);
                    }
                } else if let Some(found) = resolve(value) {
                    return Some(found);
                }
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("profiles.ini")) {
        let mut current: Option<String> = None;
        let mut current_default = false;
        let mut fallback: Option<PathBuf> = None;
        for line in text.lines().chain(std::iter::once("")) {
            let line = line.trim();
            if line.starts_with('[') {
                if current_default {
                    if let Some(path) = current.take().and_then(|p| resolve(&p)) {
                        return Some(path);
                    }
                }
                current = None;
                current_default = false;
            } else if let Some(value) = line.strip_prefix("Path=") {
                current = Some(value.to_owned());
                if fallback.is_none() {
                    fallback = current.clone().and_then(|p| resolve(&p));
                }
            } else if line == "Default=1" {
                current_default = true;
            }
        }
        if let Some(found) = fallback {
            return Some(found);
        }
    }
    // Last resort: a profile-shaped dir with a cookie database.
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("cookies.sqlite").is_file())
        .collect();
    entries.sort();
    entries.into_iter().next()
}

/// Read the Facebook cookies out of a Firefox profile. The database is
/// copied first: Firefox keeps it locked (WAL) while running.
pub fn extract_firefox_cookies(profile: &Path) -> Result<Vec<FfCookie>, String> {
    let source = profile.join("cookies.sqlite");
    if !source.is_file() {
        return Err("this Firefox profile has no cookie database — sign in to Facebook in Firefox first".to_owned());
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let copy = std::env::temp_dir().join(format!("blindfb-cookies-{stamp}.sqlite"));
    let cleanup = |copy: &Path| {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", copy.display()));
        }
    };
    std::fs::copy(&source, &copy).map_err(|e| format!("cannot read the Firefox cookie database: {e}"))?;
    for suffix in ["-wal", "-shm"] {
        let side = profile.join(format!("cookies.sqlite{suffix}"));
        if side.is_file() {
            let _ = std::fs::copy(&side, format!("{}{suffix}", copy.display()));
        }
    }
    let result = read_cookie_db(&copy);
    cleanup(&copy);
    result
}

fn read_cookie_db(db: &Path) -> Result<Vec<FfCookie>, String> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("cannot open the Firefox cookie database: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT name, value, host, path, expiry, isSecure, isHttpOnly, originAttributes \
             FROM moz_cookies WHERE host LIKE '%facebook.com'",
        )
        .map_err(|e| format!("cannot query the Firefox cookie database: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                FfCookie {
                    name: row.get(0)?,
                    value: row.get(1)?,
                    host: row.get(2)?,
                    path: row.get(3)?,
                    expiry: row.get(4)?,
                    secure: row.get::<_, i64>(5)? != 0,
                    http_only: row.get::<_, i64>(6)? != 0,
                },
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| format!("cannot read the Firefox cookies: {e}"))?;
    let mut groups: Vec<(String, Vec<FfCookie>)> = Vec::new();
    for row in rows {
        let (cookie, attributes) = row.map_err(|e| format!("cannot read the Firefox cookies: {e}"))?;
        match groups.iter_mut().find(|(key, _)| *key == attributes) {
            Some((_, list)) => list.push(cookie),
            None => groups.push((attributes, vec![cookie])),
        }
    }
    // Firefox containers keep separate cookie sets per originAttributes;
    // the signed-in session is the set holding both c_user and xs.
    let has = |list: &[FfCookie], name: &str| list.iter().any(|c| c.name == name);
    let pick = groups
        .iter()
        .find(|(attributes, list)| attributes.is_empty() && has(list, "c_user") && has(list, "xs"))
        .or_else(|| groups.iter().find(|(_, list)| has(list, "c_user") && has(list, "xs")))
        .or_else(|| groups.iter().find(|(_, list)| has(list, "c_user")));
    match pick {
        Some((_, list)) => Ok(list.clone()),
        None => Err("Firefox is not signed in to Facebook — sign in there first, then try again".to_owned()),
    }
}

/// The `Storage.setCookies` params JSON for a set of Firefox cookies —
/// the same shape fb's own importer sends.
pub fn cookies_param_json(cookies: &[FfCookie]) -> String {
    let list: Vec<serde_json::Value> = cookies
        .iter()
        .map(|c| {
            let mut object = serde_json::json!({
                "name": c.name,
                "value": c.value,
                "domain": c.host,
                "path": c.path,
                "secure": c.secure,
                "httpOnly": c.http_only,
            });
            if c.expiry > 0 {
                object["expires"] = serde_json::json!(c.expiry as f64);
            }
            object
        })
        .collect();
    serde_json::json!({ "cookies": list }).to_string()
}

/// The backend browser's own debugging endpoint (its profile dir holds a
/// DevToolsActivePort file while the daemon runs).
pub fn fb_browser_endpoint() -> Option<(u16, String)> {
    let profile = if let Some(dir) = env_path("FB_PROFILE") {
        dir
    } else if cfg!(windows) {
        env_path("LOCALAPPDATA")?.join("fb/chrome")
    } else {
        home().join(".cache/fb/chrome")
    };
    devtools_endpoint(&profile)
}

/// Send one CDP command to a browser endpoint and return its `result`.
fn cdp_call(endpoint: &(u16, String), method: &str, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    let (port, path) = endpoint;
    let url = format!("ws://127.0.0.1:{port}{path}");
    let (mut socket, _) = tungstenite::connect(url.as_str()).map_err(|e| format!("cannot reach the backend browser: {e}"))?;
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    }
    let request = serde_json::json!({ "id": 1, "method": method, "params": params }).to_string();
    socket.send(tungstenite::Message::Text(request)).map_err(|e| format!("cannot talk to the backend browser: {e}"))?;
    loop {
        let message = socket.read().map_err(|e| format!("the backend browser did not answer: {e}"))?;
        let tungstenite::Message::Text(text) = message else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if value["id"].as_i64() != Some(1) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("the backend browser refused {method}: {error}"));
        }
        return Ok(value["result"].clone());
    }
}

/// Import the Facebook session from Firefox: extract its cookies, set
/// them in the backend's browser, and verify with `fb status`.
pub fn import_firefox(fb: &Fb) -> Result<String, String> {
    let browsers = detect_browsers();
    let firefox = browsers
        .iter()
        .find(|b| b.kind == BrowserKind::Firefox)
        .ok_or("Firefox is not installed")?;
    let root = firefox.data_dir.clone().ok_or("Firefox has not run yet — open it once, sign in to Facebook there, then try again")?;
    let profile = firefox_profile(&root).ok_or("no Firefox profile found — sign in to Facebook in Firefox first")?;
    let cookies = extract_firefox_cookies(&profile)?;
    let params = cookies_param_json(&cookies);
    let params_value: serde_json::Value = serde_json::from_str(&params).unwrap_or_default();

    // Any fb command starts the daemon (and its browser).
    let _ = status(fb);

    // Route 1: through fb's named-page CDP escape hatch.
    let _ = fb.run(&["page", "blindfb-login", "open", "https://www.facebook.com"]);
    let _ = fb.run(&["page", "blindfb-login", "cdp", "Storage.setCookies", &params]);
    let _ = fb.run(&["page", "blindfb-login", "close"]);
    if let Ok(current) = status(fb) {
        if signed_in(&current) {
            return Ok(current);
        }
    }

    // Route 2: straight to the backend browser's own endpoint.
    let endpoint = fb_browser_endpoint().ok_or("the backend browser is not running")?;
    cdp_call(&endpoint, "Storage.setCookies", &params_value)?;
    let current = status(fb)?;
    if signed_in(&current) {
        Ok(current)
    } else {
        Err("Facebook did not accept the session copied from Firefox — it may have been signed out there; sign in to Facebook in Firefox again, then retry".to_owned())
    }
}

// ---------------------------------------------------------------- install

/// Download a file with the shared HTTP agent (OS cert store, like the
/// backend installer).
#[cfg(any(windows, target_os = "macos"))]
fn download(url: &str) -> Result<Vec<u8>, String> {
    let mut response = crate::http::agent()
        .get(url)
        .call()
        .map_err(|e| format!("download of {url} failed: {e}"))?;
    response.body_mut().read_to_vec().map_err(|e| format!("download of {url} failed: {e}"))
}

#[cfg(windows)]
fn winget_available() -> bool {
    on_path("winget.exe").is_some() || on_path("winget").is_some()
}

#[cfg(windows)]
fn winget_install(id: &str) -> Result<String, String> {
    let status = Command::new("winget")
        .args([
            "install",
            "--exact",
            "--id",
            id,
            "--accept-source-agreements",
            "--accept-package-agreements",
        ])
        .status()
        .map_err(|e| format!("cannot run winget: {e}"))?;
    if status.success() {
        Ok(format!("installed via winget ({id})"))
    } else {
        Err(format!("winget could not install {id} (it may have asked for administrator approval)"))
    }
}

#[cfg(windows)]
fn download_and_run(url: &str, file_name: &str) -> Result<String, String> {
    let bytes = download(url)?;
    let path = std::env::temp_dir().join(file_name);
    std::fs::write(&path, &bytes).map_err(|e| format!("cannot save the installer: {e}"))?;
    Command::new(&path)
        .stdin(Stdio::null())
        .spawn()
        .map(|_| "installer downloaded and started — follow its prompts, then try again".to_owned())
        .map_err(|e| format!("cannot start the installer: {e}"))
}

#[cfg(target_os = "macos")]
fn install_from_dmg(kind: BrowserKind, url: &str, app_name: &str) -> Result<String, String> {
    let bytes = download(url)?;
    let dmg = std::env::temp_dir().join(format!("blindfb-{}.dmg", kind.name()));
    std::fs::write(&dmg, &bytes).map_err(|e| format!("cannot save the installer: {e}"))?;
    let mount = std::env::temp_dir().join(format!("blindfb-mount-{}", kind.name()));
    let _ = std::fs::create_dir_all(&mount);
    let attached = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(&dmg)
        .status()
        .map_err(|e| format!("cannot open the installer image: {e}"))?;
    if !attached.success() {
        return Err("the installer image did not open".to_owned());
    }
    let apps = home().join("Applications");
    let _ = std::fs::create_dir_all(&apps);
    let copied = Command::new("cp").arg("-R").arg(mount.join(app_name)).arg(&apps).status();
    let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    match copied {
        Ok(status) if status.success() => Ok(format!("{} installed to your Applications folder", kind.name())),
        _ => Err(format!("could not copy {} into your Applications folder", kind.name())),
    }
}

/// Install a missing browser from the vendor's official channel.
/// Windows prefers winget (built into Windows 10/11) and falls back to
/// downloading the vendor installer; macOS installs the vendor disk image
/// into the per-user Applications folder (no administrator password);
/// Linux package installs need the administrator, so the exact command is
/// reported instead.
pub fn install(kind: BrowserKind) -> Result<String, String> {
    #[cfg(windows)]
    {
        let (winget_id, url, file) = match kind {
            BrowserKind::Chrome => (
                "Google.Chrome",
                "https://dl.google.com/dl/chrome/install/googlechromestandaloneenterprise64.msi",
                "google-chrome.msi",
            ),
            BrowserKind::Brave => (
                "Brave.Brave",
                "https://laptop-updates.brave.com/latest/winx64",
                "brave-setup.exe",
            ),
            BrowserKind::Edge => (
                "Microsoft.Edge",
                "https://go.microsoft.com/fwlink/?linkid=2109047",
                "microsoft-edge.msi",
            ),
            BrowserKind::Firefox => (
                "Mozilla.Firefox",
                "https://download.mozilla.org/?product=firefox-latest-ssl&os=win64&lang=en-US",
                "firefox-setup.exe",
            ),
            BrowserKind::Chromium => return Err("Chromium has no official Windows installer — install Google Chrome instead".to_owned()),
        };
        if winget_available() {
            return winget_install(winget_id);
        }
        return download_and_run(url, file);
    }
    #[cfg(target_os = "macos")]
    {
        let (url, app) = match kind {
            BrowserKind::Chrome => ("https://dl.google.com/chrome/mac/universal/stable/GGRO/googlechrome.dmg", "Google Chrome.app"),
            BrowserKind::Brave => ("https://laptop-updates.brave.com/latest/osx", "Brave Browser.app"),
            BrowserKind::Edge => ("https://go.microsoft.com/fwlink/?linkid=2069148", "Microsoft Edge.app"),
            BrowserKind::Firefox => ("https://download.mozilla.org/?product=firefox-latest-ssl&os=osx&lang=en-US", "Firefox.app"),
            BrowserKind::Chromium => return Err("Chromium has no official macOS installer — install Google Chrome instead".to_owned()),
        };
        return install_from_dmg(kind, url, app);
    }
    #[cfg(target_os = "linux")]
    {
        let package = match kind {
            BrowserKind::Chrome => "google-chrome-stable (from Google's download page)",
            BrowserKind::Chromium => "chromium",
            BrowserKind::Brave => "brave-browser (from Brave's download page)",
            BrowserKind::Edge => "microsoft-edge-stable (from Microsoft's download page)",
            BrowserKind::Firefox => "firefox",
        };
        return Err(format!(
            "installing a browser on Linux needs the administrator — run your package manager to install {package}, then try again"
        ));
    }
    #[allow(unreachable_code)]
    Err("browser installs are not supported on this system".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_port_parses() {
        assert_eq!(parse_active_port("9222\n/devtools/browser/abc\n"), Some((9222, "/devtools/browser/abc".to_owned())));
        assert_eq!(parse_active_port("9222\n"), Some((9222, String::new())));
        assert_eq!(parse_active_port("nope\n"), None);
        assert_eq!(parse_active_port(""), None);
    }

    #[test]
    fn progids_map() {
        assert_eq!(kind_from_progid("ChromeHTML"), Some(BrowserKind::Chrome));
        assert_eq!(kind_from_progid("BraveHTML"), Some(BrowserKind::Brave));
        assert_eq!(kind_from_progid("MSEdgeHTM"), Some(BrowserKind::Edge));
        assert_eq!(kind_from_progid("FirefoxURL-308046B0AF4A39CB"), Some(BrowserKind::Firefox));
        assert_eq!(kind_from_progid("AppXsomething"), None);
    }

    #[test]
    fn hints_map() {
        assert_eq!(kind_from_hint("brave-browser.desktop"), Some(BrowserKind::Brave));
        assert_eq!(kind_from_hint("microsoft-edge.desktop"), Some(BrowserKind::Edge));
        assert_eq!(kind_from_hint("com.microsoft.edgemac"), Some(BrowserKind::Edge));
        assert_eq!(kind_from_hint("org.mozilla.firefox"), Some(BrowserKind::Firefox));
        assert_eq!(kind_from_hint("google-chrome.desktop"), Some(BrowserKind::Chrome));
        assert_eq!(kind_from_hint("chromium.desktop"), Some(BrowserKind::Chromium));
    }

    #[test]
    fn local_state_flip_preserves_other_keys() {
        let dir = std::env::temp_dir().join(format!("blindfb-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("Local State"), r#"{"devtools":{"other":1},"profile":{"last_used":"Default"}}"#).unwrap();
        enable_remote_debugging(&dir).unwrap();
        let after: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("Local State")).unwrap()).unwrap();
        assert_eq!(after["devtools"]["remote_debugging"]["user-enabled"], true);
        assert_eq!(after["devtools"]["other"], 1);
        assert_eq!(after["profile"]["last_used"], "Default");
        assert!(dir.join("Local State.blindfb-backup").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn firefox_profile_from_profiles_ini() {
        let root = std::env::temp_dir().join(format!("blindfb-ff-{}", std::process::id()));
        let profile = root.join("abcd.default-release");
        let _ = std::fs::create_dir_all(&profile);
        std::fs::write(
            root.join("profiles.ini"),
            "[Profile0]\nName=default-release\nIsRelative=1\nPath=abcd.default-release\nDefault=1\n",
        )
        .unwrap();
        assert_eq!(firefox_profile(&root), Some(profile));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn firefox_cookies_round_trip() {
        let dir = std::env::temp_dir().join(format!("blindfb-ffdb-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db = dir.join("cookies.sqlite");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE moz_cookies (id INTEGER PRIMARY KEY, originAttributes TEXT NOT NULL DEFAULT '', \
                 name TEXT, value TEXT, host TEXT, path TEXT, expiry INTEGER, isSecure INTEGER, isHttpOnly INTEGER);",
            )
            .unwrap();
            let insert = "INSERT INTO moz_cookies (originAttributes, name, value, host, path, expiry, isSecure, isHttpOnly) \
                            VALUES (?1, ?2, ?3, ?4, '/', 1893456000, 1, 1)";
            conn.execute(insert, rusqlite::params!["", "c_user", "12345", ".facebook.com"]).unwrap();
            conn.execute(insert, rusqlite::params!["", "xs", "secret", ".facebook.com"]).unwrap();
            conn.execute(insert, rusqlite::params!["", "fr", "x", ".example.com"]).unwrap();
        }
        let cookies = extract_firefox_cookies(&dir).unwrap();
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|c| c.name == "c_user" && c.value == "12345"));
        let json = cookies_param_json(&cookies);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["cookies"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
