//! Build script, via shipfitter: embed the commit in the binary, and on
//! Windows embed the application manifest and the Explorer version resource.

fn main() {
    shipfitter::build::embed_commit_info("BLINDFB");
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
