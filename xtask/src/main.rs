//! Packaging for BlindFB releases, via shipfitter.
//!
//! `cargo xtask package` builds the release binary and writes
//! `dist/BlindFB-<version>-<os>-<arch>.zip` (Windows) or `.tar.gz`
//! (Linux/macOS, with a .app bundle inside on macOS).

use std::env;
use std::error::Error;
use std::path::PathBuf;

use shipfitter::macos::MacApp;
use shipfitter::package::{TarGz, Zip, cargo_build_release, crate_version};

fn main() -> Result<(), Box<dyn Error>> {
    match env::args().nth(1).as_deref() {
        Some("package") => package(),
        _ => {
            eprintln!("usage: cargo xtask package");
            std::process::exit(2);
        }
    }
}

fn package() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf();
    cargo_build_release(&root, &["blindfb"])?;
    let version = crate_version(&root, "blindfb")?;
    let target_dir = root.join("target").join("release");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist)?;
    let suffix = shipfitter::host_arch_suffix();
    let exe_name = if cfg!(windows) { "blindfb.exe" } else { "blindfb" };
    let binary = target_dir.join(exe_name);
    let readme = root.join("README.md");

    if cfg!(windows) {
        let out = dist.join(format!("BlindFB-{version}-{suffix}.zip"));
        let mut zip = Zip::create(&out)?;
        zip.file(&binary, "blindfb.exe")?.file(&readme, "README.md")?;
        zip.finish()?;
        println!("wrote {}", out.display());
    } else if cfg!(target_os = "macos") {
        let app = MacApp {
            name: "BlindFB",
            identifier: "com.serrebidev.blindfb",
            executable: "blindfb",
            version: &version,
            icon: None,
            extra: "",
        };
        let bundle = app.bundle(&dist, &binary, &[], &[])?;
        let out = dist.join(format!("BlindFB-{version}-{suffix}.tar.gz"));
        let mut tar = TarGz::create(&out)?;
        tar.dir(&bundle, "BlindFB.app")?.file(&readme, "README.md")?;
        tar.finish()?;
        println!("wrote {}", out.display());
    } else {
        let out = dist.join(format!("BlindFB-{version}-{suffix}.tar.gz"));
        let mut tar = TarGz::create(&out)?;
        tar.file(&binary, "blindfb")?.file(&readme, "README.md")?;
        tar.finish()?;
        println!("wrote {}", out.display());
    }
    Ok(())
}
