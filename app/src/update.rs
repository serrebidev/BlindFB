//! In-app updates: ship-shape against this repository's GitHub Releases, verified with
//! minisign.
//!
//! The public key below is the one `cargo xtask minisign-key` made; its secret half signs
//! every release asset and lives in the `MINISIGN_KEY` secret, never in git.

use std::sync::Arc;

use ship_shape::ui::{self, CheckTrigger};
use ship_shape::{InstallKind, UpdateChannel, UpdaterConfig};
use wxdragon::prelude::WxWidget;

/// The second line of `assets/minisign.pub`, read in by build.rs.
const PUBLIC_KEY: &str = env!("BLINDFB_MINISIGN_PUBLIC_KEY");

pub fn check(parent: &dyn WxWidget, trigger: CheckTrigger) {
    let config = UpdaterConfig::new(
        "serrebidev/BlindFB",
        "BlindFB",
        "BlindFB",
        PUBLIC_KEY,
        env!("CARGO_PKG_VERSION"),
    )
    .with_commit(env!("BLINDFB_COMMIT_HASH"))
    .with_install_kind(InstallKind::Portable);
    // macOS publishes one disk image per architecture, so the name carries the suffix there.
    #[cfg(target_os = "macos")]
    let config = config.with_asset_suffix(if cfg!(target_arch = "aarch64") {
        "-arm64"
    } else {
        "-x64"
    });
    ui::run_update_check(Arc::new(config), parent, UpdateChannel::Stable, trigger);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_key_is_a_minisign_public_key() {
        assert!(PUBLIC_KEY.starts_with("RW"), "got {PUBLIC_KEY:?}");
    }
}
