//! Packaging for BlindFB releases, via shipfitter.
//!
//! `cargo xtask package` builds the release binary and writes
//! `dist/BlindFB-<version>-<os>-<arch>.zip` (Windows) or `.tar.gz`
//! (Linux/macOS, with a .app bundle inside on macOS).

use std::env;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;

use shipfitter::macos::MacApp;

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
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    if !Command::new(cargo).current_dir(&root)
        .args(["build", "--release", "--locked", "-p", "blindfb"]).status()?.success() {
        return Err("cargo build failed for blindfb".into());
    }
    let version = env!("CARGO_PKG_VERSION");
    let target_dir = root.join("target").join("release");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist)?;
    let suffix = shipfitter::host_arch_suffix();
    let exe_name = if cfg!(windows) { "blindfb.exe" } else { "blindfb" };
    let binary = target_dir.join(exe_name);
    let readme = root.join("README.md");

    if cfg!(windows) {
        let out = dist.join(format!("BlindFB-{version}-{suffix}.zip"));
        let mut zip = zip::ZipWriter::new(File::create(&out)?);
        for (path, name) in [(&binary, "blindfb.exe"), (&readme, "README.md")] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated))?;
            std::io::copy(&mut File::open(path)?, &mut zip)?;
        }
        zip.finish()?;
        println!("wrote {}", out.display());
    } else if cfg!(target_os = "macos") {
        let app = MacApp {
            name: "BlindFB",
            identifier: "com.serrebidev.blindfb",
            executable: "blindfb",
            version,
            icon: None,
            extra: "",
        };
        let bundle = app.bundle(&dist, &binary, &[], &[])?;
        let out = dist.join(format!("BlindFB-{version}-{suffix}.tar.gz"));
        let gzip = flate2::write::GzEncoder::new(File::create(&out)?, flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        tar.append_dir_all("BlindFB.app", &bundle)?;
        tar.append_path_with_name(&readme, "README.md")?;
        tar.into_inner()?.finish()?;
        println!("wrote {}", out.display());
    } else {
        let out = dist.join(format!("BlindFB-{version}-{suffix}.tar.gz"));
        let gzip = flate2::write::GzEncoder::new(File::create(&out)?, flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        tar.append_path_with_name(&binary, "blindfb")?;
        tar.append_path_with_name(&readme, "README.md")?;
        tar.into_inner()?.finish()?;
        println!("wrote {}", out.display());
    }
    Ok(())
}
