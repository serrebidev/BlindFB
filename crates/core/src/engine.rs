//! A separate Chromium profile for backends that cannot launch the installed browser.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::browsers::{cdp_call, devtools_endpoint, Browser};

fn profile() -> Result<PathBuf, String> {
    crate::backend::data_dir()
        .map(|dir| dir.join("browser"))
        .ok_or_else(|| "cannot find BlindFB's browser data folder".to_owned())
}

pub(crate) fn endpoint() -> Option<(u16, String)> {
    let endpoint = devtools_endpoint(&profile().ok()?)?;
    if endpoint.0 == 0 || !endpoint.1.starts_with("/devtools/browser/") {
        return None;
    }
    cdp_call(&endpoint, "Browser.getVersion", &serde_json::json!({})).ok()?;
    Some(endpoint)
}

/// Reuse the dedicated browser, or launch one and report that the daemon must reconnect.
pub(crate) fn ensure(browser: &Browser, visible: bool) -> Result<((u16, String), bool), String> {
    if let Some(endpoint) = endpoint() {
        return Ok((endpoint, false));
    }
    let profile = profile()?;
    std::fs::create_dir_all(&profile)
        .map_err(|e| format!("cannot create the browser profile: {e}"))?;
    let active_port = profile.join("DevToolsActivePort");
    match std::fs::remove_file(active_port) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("cannot clear the stopped browser endpoint: {e}")),
    }
    let exe = browser
        .exe
        .as_ref()
        .ok_or("the selected browser is not installed")?;
    let mut command = Command::new(exe);
    command
        .arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--remote-debugging-port=0",
            "--remote-debugging-address=127.0.0.1",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-mode",
        ]);
    if !visible {
        command.arg("--headless=new");
    }
    command
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // No console; visible browser windows still work.
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", browser.name()))?;
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if let Some(endpoint) = endpoint() {
            // Reap the launcher after the browser exits without blocking the worker.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok((endpoint, true));
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "{} exited before its browser connection was ready: {status}",
                browser.name()
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(format!(
        "{} did not start its dedicated BlindFB browser within 40 seconds",
        browser.name()
    ))
}

pub(crate) fn close() -> Result<(), String> {
    let Some(endpoint) = endpoint() else {
        return Ok(());
    };
    // Browser.close may close the socket before returning its response.
    let _ = cdp_call(&endpoint, "Browser.close", &serde_json::json!({}));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if self::endpoint().is_none() {
            // Chromium releases its profile lock shortly after closing its debugging socket.
            std::thread::sleep(Duration::from_millis(500));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("BlindFB's dedicated browser did not close; close its window and retry sign-in".to_owned())
}
