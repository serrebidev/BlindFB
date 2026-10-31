//! Pinned, app-managed Windows mpv bundle. Installation blocks; use a worker thread.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const RELEASE_URL: &str = "https://github.com/mpv-player/mpv/releases/download/v0.41.0/mpv-v0.41.0-x86_64-w64-mingw32.zip";
const RELEASE_SHA256: &str = "a49811c0752c108b8260636f9c6f6fcb97406641c98b30f1e7b500dfb20177de";
const BUNDLE_NAME: &str = "mpv-git-2025-12-21-41f6a64-x86_64.zip";
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// The pinned executable path, whether or not installation has completed.
pub fn managed_bin() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    crate::backend::data_dir().map(|dir| dir.join("bin/mpv-0.41.0/mpv.exe"))
}

/// Install the complete verified bundle on first use. Never replaces a running version.
pub fn ensure() -> Result<PathBuf, String> {
    let target = managed_bin()
        .ok_or("the managed mpv player is available only on Windows with a local data folder")?;
    let _lock = INSTALL_LOCK
        .lock()
        .map_err(|_| "the mpv installer lock was poisoned")?;
    if target.is_file() {
        return Ok(target);
    }
    let bytes = crate::backend::get(RELEASE_URL)?;
    install(&bytes, &target)?;
    Ok(target)
}

fn install(bytes: &[u8], target: &Path) -> Result<(), String> {
    // Authenticate the outer archive before interpreting either archive's contents.
    if crate::backend::sha256_hex(bytes) != RELEASE_SHA256 {
        return Err("the mpv download failed its checksum; it was not installed".to_owned());
    }
    let bundle = bundle_bytes(bytes)?;
    let destination = target.parent().ok_or("bad mpv install path")?;
    let parent = destination.parent().ok_or("bad mpv install path")?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("could not create the mpv install folder: {e}"))?;
    let stage = create_stage(parent)?;
    let result = (|| {
        extract_bundle(&bundle, &stage)?;
        if !stage.join("mpv.exe").is_file() {
            return Err("the mpv download did not contain mpv.exe".to_owned());
        }
        match fs::rename(&stage, destination) {
            Ok(()) => Ok(()),
            // Another app process may have completed the same pinned installation.
            Err(_) if target.is_file() => Ok(()),
            Err(e) => Err(format!("could not install mpv: {e}")),
        }
    })();
    // stage was created exclusively beneath parent and is never an existing install.
    if stage.parent() == Some(parent) && stage.is_dir() {
        fs::remove_dir_all(&stage)
            .map_err(|e| format!("could not remove the mpv staging folder: {e}"))?;
    }
    result
}

fn create_stage(parent: &Path) -> Result<PathBuf, String> {
    for attempt in 0..1000 {
        let stage = parent.join(format!(".mpv-stage-{}-{attempt}", std::process::id()));
        match fs::create_dir(&stage) {
            Ok(()) => return Ok(stage),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("could not create the mpv staging folder: {e}")),
        }
    }
    Err("could not find an unused mpv staging folder".to_owned())
}

fn bundle_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("could not open the mpv download: {e}"))?;
    if zip.len() != 1 {
        return Err("unexpected mpv release archive contents".to_owned());
    }
    let mut entry = zip
        .by_index(0)
        .map_err(|e| format!("could not read the mpv download: {e}"))?;
    if entry.name().map_err(|e| e.to_string())? != BUNDLE_NAME {
        return Err("unexpected mpv bundle name".to_owned());
    }
    let mut bundle = Vec::new();
    entry
        .read_to_end(&mut bundle)
        .map_err(|e| format!("could not read the mpv bundle: {e}"))?;
    Ok(bundle)
}

fn safe_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with(['/', '\\']) || name.contains(':') {
        return false;
    }
    name.trim_end_matches(['/', '\\'])
        .split(['/', '\\'])
        .all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "<>\"|?*".contains(c))
                && !matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                )
                && !(stem.chars().count() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.chars().last(), Some('1'..='9' | '¹' | '²' | '³')))
        })
}

fn extract_bundle(bytes: &[u8], stage: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("could not open the mpv bundle: {e}"))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("could not read the mpv bundle: {e}"))?;
        let name = entry
            .name()
            .map_err(|e| format!("bad mpv archive name: {e}"))?;
        if !safe_name(&name)
            || entry.unix_mode().is_some_and(|mode| {
                let kind = mode & 0o170000;
                kind != 0 && kind != 0o100000 && kind != 0o040000
            })
        {
            return Err(format!("unsafe path in the mpv bundle: {name}"));
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or("unsafe path in the mpv bundle")?;
        let path = stage.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&path)
                .map_err(|e| format!("could not create an mpv folder: {e}"))?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("could not create an mpv folder: {e}"))?;
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| format!("could not create an mpv file: {e}"))?;
            std::io::copy(&mut entry, &mut file)
                .map_err(|e| format!("could not unpack mpv: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_windows_path_aliases_and_traversal() {
        for name in [
            "../escape",
            "a/../escape",
            "/root",
            "\\root",
            "C:drive",
            "a:stream",
            "a\\..\\escape",
            "CON.txt",
            "nul",
            "a/LPT1",
            "a/COM²",
            "a./file",
            "a /file",
            "a//file",
            "a\0file",
            "a?file",
            "./file",
        ] {
            assert!(!safe_name(name), "accepted {name:?}");
        }
        for name in ["mpv.exe", "licenses/COPYING.txt", "dir/", "libstdc++-6.dll"] {
            assert!(safe_name(name), "rejected {name:?}");
        }
    }

    #[test]
    fn checksum_failure_creates_no_install_folder() {
        let parent =
            std::env::temp_dir().join(format!("blindfb-mpv-bad-hash-{}", std::process::id()));
        assert!(!parent.exists());
        assert!(install(b"untrusted archive", &parent.join("mpv.exe"))
            .unwrap_err()
            .contains("checksum"));
        assert!(!parent.exists());
    }

    #[test]
    fn extracts_bundle_files_and_rejects_symlinks() {
        let parent = std::env::temp_dir();
        let stage = create_stage(&parent).unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["mpv.exe", "codec.dll", "licenses/COPYING"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"test contents").unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        extract_bundle(&bytes, &stage).unwrap();
        assert_eq!(
            fs::read(stage.join("licenses/COPYING")).unwrap(),
            b"test contents"
        );
        assert_eq!(fs::read(stage.join("codec.dll")).unwrap(), b"test contents");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.add_symlink(
            "link",
            "../outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        assert!(extract_bundle(&zip.finish().unwrap().into_inner(), &stage).is_err());
        assert!(!stage.join("link").exists());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("../escape", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"must not escape").unwrap();
        assert!(extract_bundle(&zip.finish().unwrap().into_inner(), &stage).is_err());
        assert_eq!(stage.parent(), Some(parent.as_path()));
        fs::remove_dir_all(stage).unwrap();
    }
}
