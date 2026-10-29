//! Build script, via shipfitter: embed the commit in the binary, and on
//! Windows embed the application manifest and the Explorer version resource.

fn main() {
    // wxWidgets debug symbols exceed the default MSVC PDB page limit.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/PDBPAGESIZE:8192");
    }
    shipfitter::build::embed_commit_info("BLINDFB");
    // The updater verifies every download with this key; its secret half signs releases.
    println!("cargo:rerun-if-changed=assets/minisign.pub");
    let public = std::fs::read_to_string("assets/minisign.pub").expect("assets/minisign.pub is missing");
    println!(
        "cargo:rustc-env=BLINDFB_MINISIGN_PUBLIC_KEY={}",
        public.lines().nth(1).unwrap_or_default().trim()
    );
    shipfitter::windows::embed_manifest("BlindFB").expect("embed manifest");
    shipfitter::windows::VersionInfo {
        product_name: "BlindFB",
        company: "Brandon Bracey",
        copyright: "Copyright (c) 2026 Brandon Bracey",
        original_filename: "blindfb.exe",
        product_version: None,
        icon: None,
    }
    .embed()
    .expect("embed version info");
}
