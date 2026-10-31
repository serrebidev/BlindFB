//! Automatic backend install: download the matching `fb` (facebook-cli)
//! release from GitHub, verify its SHA-256 against the release's
//! SHA256SUMS, and unpack it into BlindFB's managed bin directory.
//!
//! The user never needs a terminal or a Rust toolchain: the app installs
//! the backend on first run and can update it from the Backend menu.

use std::fs;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// The backend build BlindFB ships: upstream facebook-cli plus fixes still in review upstream
/// (feed links/text, empty editors and the Messenger restore prompt). Back to upstream
/// once those merge.
pub const BACKEND_VERSION: &str = "0.1.5-blindfb.11";
const RELEASE_URL: &str =
    "https://api.github.com/repos/serrebidev/facebook-cli/releases/tags/v0.1.5-blindfb.11";

/// The managed backend exists but is not the build this BlindFB ships.
pub fn managed_outdated() -> bool {
    let Some(bin) = managed_bin().filter(|b| b.is_file()) else {
        return false;
    };
    let mut command = std::process::Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.arg("--version").output().map_or(true, |out| {
        version_outdated(out.status.success(), &out.stdout)
    })
}

fn version_outdated(success: bool, stdout: &[u8]) -> bool {
    !success || String::from_utf8_lossy(stdout).trim() != format!("fb {BACKEND_VERSION}")
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "fb.exe"
    } else {
        "fb"
    }
}

/// BlindFB's per-user data directory: %LOCALAPPDATA%\BlindFB on Windows,
/// ~/Library/Application Support/BlindFB on macOS, $XDG_DATA_HOME/BlindFB
/// (or ~/.local/share/BlindFB) on Linux.
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("BlindFB"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BlindFB")
        })
    } else {
        match std::env::var_os("XDG_DATA_HOME") {
            Some(base) if !base.is_empty() => Some(PathBuf::from(base).join("BlindFB")),
            _ => std::env::var_os("HOME").map(|home| {
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("BlindFB")
            }),
        }
    }
}

/// Where the app-managed `fb` binary lives (whether or not it exists yet).
pub fn managed_bin() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("bin").join(exe_name()))
}

/// The release asset for this OS and CPU, as named by facebook-cli.
/// Windows on ARM runs the x86_64 build under emulation.
pub fn asset_name() -> Option<&'static str> {
    let arch = std::env::consts::ARCH;
    if cfg!(windows) {
        Some("fb-x86_64-windows.zip")
    } else if cfg!(target_os = "macos") {
        match arch {
            "aarch64" => Some("fb-aarch64-darwin.tar.gz"),
            _ => Some("fb-x86_64-darwin.tar.gz"),
        }
    } else if cfg!(target_os = "linux") {
        match arch {
            "aarch64" => Some("fb-aarch64-linux.tar.gz"),
            "x86_64" => Some("fb-x86_64-linux.tar.gz"),
            _ => None,
        }
    } else {
        None
    }
}

pub(crate) fn get(url: &str) -> Result<Vec<u8>, String> {
    let mut response = crate::http::agent()
        .get(url)
        .header("User-Agent", concat!("BlindFB/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    crate::http::download_body(response.body_mut()).map_err(|e| format!("download failed: {e}"))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Find the expected hash for `asset` in SHA256SUMS content.
pub fn expected_hash(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset).then(|| hash.to_lowercase())
    })
}

fn extract(asset: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let want = exe_name();
    if asset.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| format!("could not open the backend download: {e}"))?;
        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| format!("could not read the backend download: {e}"))?;
            let name = entry
                .name()
                .map_err(|e| format!("bad backend download: {e}"))?
                .replace('\\', "/");
            if name == want || name.ends_with(&format!("/{want}")) {
                let mut out = Vec::new();
                entry
                    .read_to_end(&mut out)
                    .map_err(|e| format!("could not unpack the backend: {e}"))?;
                return Ok(out);
            }
        }
        Err(format!("the backend download did not contain {want}"))
    } else {
        let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
        let mut tar = tar::Archive::new(decoder);
        for entry in tar
            .entries()
            .map_err(|e| format!("could not open the backend download: {e}"))?
        {
            let mut entry =
                entry.map_err(|e| format!("could not read the backend download: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("bad backend download: {e}"))?;
            let name = path.to_string_lossy().replace('\\', "/");
            if name == want || name.ends_with(&format!("/{want}")) {
                let mut out = Vec::new();
                entry
                    .read_to_end(&mut out)
                    .map_err(|e| format!("could not unpack the backend: {e}"))?;
                return Ok(out);
            }
        }
        Err(format!("the backend download did not contain {want}"))
    }
}

/// Download BlindFB's pinned facebook-cli release and install it as the managed
/// backend. Stops a running managed daemon first so the binary can be
/// replaced. Blocking; call from a worker thread. Returns a summary line.
pub fn install_or_update() -> Result<String, String> {
    let asset = asset_name().ok_or("no prebuilt backend for this platform")?;
    let target = managed_bin().ok_or("could not find a place to install the backend")?;

    let json: serde_json::Value =
        serde_json::from_slice(&get(RELEASE_URL)?).map_err(|e| format!("download failed: {e}"))?;
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("download failed: no tag in the release response")?
        .to_owned();
    let assets = json
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or("download failed: no assets in the release response")?;
    let url_of = |name: &str| -> Option<String> {
        assets.iter().find_map(|a| {
            (a.get("name").and_then(|v| v.as_str()) == Some(name))
                .then(|| {
                    a.get("browser_download_url")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .flatten()
        })
    };
    let asset_url =
        url_of(asset).ok_or("download failed: release has no build for this platform")?;
    let sums_url = url_of("SHA256SUMS").ok_or("download failed: release has no checksums file")?;

    let bytes = get(&asset_url)?;
    let sums = String::from_utf8_lossy(&get(&sums_url)?).into_owned();
    let expected =
        expected_hash(&sums, asset).ok_or("download failed: no checksum for this build")?;
    if sha256_hex(&bytes) != expected {
        return Err("the backend download failed its checksum — not installing it".to_owned());
    }
    let binary = extract(asset, &bytes)?;

    if target.exists() {
        // Replacing a running daemon's binary fails on Windows; stop it first.
        let _ = crate::fb::Fb::with_program(target.clone()).run(&["quit"]);
    }
    let dir = target.parent().ok_or("bad install path")?;
    fs::create_dir_all(dir).map_err(|e| format!("could not create the install folder: {e}"))?;
    let tmp = dir.join(format!("{}.new", exe_name()));
    fs::write(&tmp, &binary).map_err(|e| format!("could not write the backend: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    replace_binary(&tmp, &target, Duration::from_secs(5))
        .map_err(|e| format!("could not install the backend: {e}"))?;
    Ok(format!(
        "Installed the Facebook backend {tag} to {}",
        target.display()
    ))
}

fn replace_binary(
    source: &std::path::Path,
    target: &std::path::Path,
    timeout: Duration,
) -> std::io::Result<()> {
    let start = Instant::now();
    loop {
        match fs::rename(source, target) {
            Ok(()) => return Ok(()),
            // A daemon can return from quit just before Windows releases its executable.
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && start.elapsed() < timeout =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn backend_replacement_waits_for_a_closing_process_file_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("blindfb-backend-lock-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("fb.exe");
        let source = dir.join("fb.exe.new");
        fs::write(&target, b"old").unwrap();
        fs::write(&source, b"new").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(fs::rename(&source, &target).is_err());
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });
        replace_binary(&source, &target, Duration::from_secs(2)).unwrap();
        release.join().unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extracts_only_the_backend_binary_from_both_archive_formats() {
        use std::io::Write;
        let contents = b"backend executable";
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            format!("release/{}", exe_name()),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(contents).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(extract("backend.zip", &bytes).unwrap(), contents);

        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(
            &mut header,
            format!("release/{}", exe_name()),
            &contents[..],
        )
        .unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        assert_eq!(extract("backend.tar.gz", &bytes).unwrap(), contents);
    }

    #[test]
    fn only_a_successful_shipped_version_is_current() {
        let current = format!("fb {BACKEND_VERSION}\r\n");
        assert!(!version_outdated(true, current.as_bytes()));
        assert!(version_outdated(false, current.as_bytes()));
        assert!(version_outdated(true, b"fb 0.1.5\n"));
        assert!(version_outdated(true, b""));
    }

    #[test]
    fn finds_expected_hash() {
        let sums = "abc123  fb-x86_64-linux.tar.gz\ndef456 *fb-x86_64-windows.zip\n";
        assert_eq!(
            expected_hash(sums, "fb-x86_64-windows.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_hash(sums, "fb-aarch64-linux.tar.gz"), None);
    }

    #[test]
    fn has_an_asset_and_a_managed_path() {
        assert!(asset_name().is_some());
        assert!(managed_bin().is_some());
    }
}
