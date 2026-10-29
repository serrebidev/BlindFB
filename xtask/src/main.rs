//! Packaging for BlindFB releases, via shipfitter, and the updater's helpers.
//!
//! `cargo xtask package` builds the release binary and writes the one asset each
//! platform's updater (ship-shape) looks for: `dist/BlindFB.zip` on Windows,
//! `dist/BlindFB.tar.gz` on Linux, and `dist/BlindFB-<arch>.dmg` on macOS.
//! Each asset is signed with minisign when `MINISIGN_KEY` is set.

use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use shipfitter::macos::MacApp;
use shipfitter::package::{TarGz, Zip};

const APP: &str = "BlindFB";

fn main() -> Result<(), Box<dyn Error>> {
    match env::args().nth(1).as_deref() {
        Some("package") => package(),
        Some("sign") => sign(),
        Some("minisign-key") => minisign_key(),
        Some("update-check") => update_check(),
        _ => {
            eprintln!("usage: cargo xtask package|sign <file>...|minisign-key <dir>|update-check [version]");
            std::process::exit(2);
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

fn package() -> Result<(), Box<dyn Error>> {
    let root = root();
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    if !Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "--locked", "-p", "blindfb"])
        .status()?
        .success()
    {
        return Err("cargo build failed for blindfb".into());
    }
    let version = env!("CARGO_PKG_VERSION");
    let dist = root.join("dist");
    fs::create_dir_all(&dist)?;
    let exe_name = if cfg!(windows) { "blindfb.exe" } else { "blindfb" };
    let binary = root.join("target").join("release").join(exe_name);
    let readme = root.join("README.md");

    let asset = if cfg!(windows) {
        let out = dist.join(format!("{APP}.zip"));
        let mut zip = Zip::create(&out)?;
        zip.file(&binary, "blindfb.exe")?.file(&readme, "README.md")?;
        zip.finish()?;
        out
    } else if cfg!(target_os = "macos") {
        let app = MacApp {
            name: APP,
            identifier: "com.serrebidev.blindfb",
            executable: "blindfb",
            version,
            icon: None,
            extra: "",
        };
        let bundle = app.bundle(&dist, &binary, &[], &[])?;
        // Ad-hoc: BlindFB is distributed unsigned, so the updater offers the image
        // to drag into Applications instead of replacing the bundle itself.
        shipfitter::macos::sign(&bundle, &[])?;
        let out = dist.join(format!("{APP}-{}.dmg", shipfitter::host_arch_suffix()));
        shipfitter::macos::dmg(&bundle, &out)?;
        out
    } else {
        let out = dist.join(format!("{APP}.tar.gz"));
        let mut tar = TarGz::create(&out)?;
        tar.file(&binary, "blindfb")?;
        tar.file(&readme, "README.md")?;
        tar.finish()?;
        out
    };

    println!("wrote {}", asset.display());
    sign_asset(&asset)
}

/// Writes `<file>.minisig` beside the release asset; that signature is what ship-shape checks.
/// `MINISIGN_KEY` holds the key, `MINISIGN_PASSWORD` unlocks it if it is encrypted.
fn sign_asset(path: &Path) -> Result<(), Box<dyn Error>> {
    let Ok(key) = env::var("MINISIGN_KEY") else {
        eprintln!(
            "xtask: MINISIGN_KEY is not set, so {} is unsigned and the updater will reject it",
            path.display()
        );
        return Ok(());
    };
    let password = env::var("MINISIGN_PASSWORD").ok();
    sign_with(path, &key, password.as_deref())
}

fn sign_with(path: &Path, key: &str, password: Option<&str>) -> Result<(), Box<dyn Error>> {
    let boxed = minisign::SecretKeyBox::from_string(key)?;
    let key = match password {
        Some(password) => boxed.into_secret_key(Some(password.to_owned())),
        None => boxed.into_unencrypted_secret_key(),
    }?;
    let name = path.file_name().ok_or("path has no file name")?.to_string_lossy();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let trusted = format!("timestamp:{timestamp}\tfile:{name}\thashed");
    let reader = std::io::BufReader::new(File::open(path)?);
    let signature = minisign::sign(None, &key, reader, Some(&trusted), None)?;
    let mut out = path.as_os_str().to_owned();
    out.push(".minisig");
    fs::write(&out, signature.into_string())?;
    println!("signed {}", Path::new(&out).display());
    Ok(())
}

fn sign() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = env::args().skip(2).collect();
    if files.is_empty() {
        return Err("sign needs at least one file".into());
    }
    if env::var("MINISIGN_KEY").is_err() {
        return Err("MINISIGN_KEY is not set".into());
    }
    for file in files {
        sign_asset(Path::new(&file))?;
    }
    Ok(())
}

/// Writes an unencrypted key pair; the secret half goes in the `MINISIGN_KEY` secret and
/// in Vaultwarden, never in git. The second line of the public half goes into the app.
fn minisign_key() -> Result<(), Box<dyn Error>> {
    let dir = env::args()
        .nth(2)
        .ok_or("minisign-key needs a folder, such as ~/.minisign/blindfb")?;
    let pair = minisign::KeyPair::generate_unencrypted_keypair()?;
    fs::create_dir_all(&dir)?;
    fs::write(
        Path::new(&dir).join("minisign.key"),
        pair.sk.to_box(None)?.into_string(),
    )?;
    let public = pair.pk.to_box()?.into_string();
    fs::write(Path::new(&dir).join("minisign.pub"), &public)?;
    println!("{public}");
    Ok(())
}

/// Asks GitHub what a copy running `version` would be offered, downloads that asset and
/// verifies it, exactly as the app does.
fn update_check() -> Result<(), Box<dyn Error>> {
    let current = env::args()
        .nth(2)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    let key = fs::read_to_string(root().join("app/assets/minisign.pub"))?;
    let key = key
        .lines()
        .nth(1)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let config = ship_shape::UpdaterConfig::new(
        "serrebidev/BlindFB",
        APP,
        APP,
        key,
        current.trim_start_matches('v'),
    )
    .with_install_kind(ship_shape::InstallKind::Portable);
    let config = if cfg!(target_os = "macos") {
        config.with_asset_suffix(if cfg!(target_arch = "aarch64") {
            "-arm64"
        } else {
            "-x64"
        })
    } else {
        config
    };

    let update = match ship_shape::check_for_updates(&config, ship_shape::UpdateChannel::Stable)
        .map_err(|e| format!("check: {e}"))?
    {
        ship_shape::UpdateCheckOutcome::UpToDate(version) => {
            println!("up to date at {version}");
            return Ok(());
        }
        ship_shape::UpdateCheckOutcome::UpdateAvailable(update) => update,
    };
    println!(
        "{} would be offered {} from {}",
        config.current_version, update.latest_version, update.download_url
    );
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let path = ship_shape::download_update_file(
        &config,
        &update.download_url,
        &update.signature_url,
        &cancelled,
        |_, _| {},
    )
    .map_err(|e| format!("download: {e}"))?;
    println!("verified and saved to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signed_asset_verifies_and_a_tampered_one_does_not() {
        let dir = std::env::temp_dir().join(format!("blindfb-sign-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BlindFB.zip");
        fs::write(&file, b"release contents").unwrap();
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        sign_with(&file, &pair.sk.to_box(None).unwrap().into_string(), None).unwrap();

        let public = pair.pk.to_box().unwrap().into_string();
        let public = minisign_verify::PublicKey::from_base64(public.lines().nth(1).unwrap()).unwrap();
        let signature = minisign_verify::Signature::decode(
            &fs::read_to_string(dir.join("BlindFB.zip.minisig")).unwrap(),
        )
        .unwrap();
        public.verify(&fs::read(&file).unwrap(), &signature, false).unwrap();
        assert!(public.verify(b"tampered", &signature, false).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
