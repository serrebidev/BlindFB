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
        "serrebi/BlindFB",
        "BlindFB",
        "BlindFB",
        PUBLIC_KEY,
        env!("CARGO_PKG_VERSION"),
    )
    .with_commit(env!("BLINDFB_COMMIT_HASH"))
    .with_install_kind(InstallKind::Portable)
    .with_api_base_url("https://git.serrebiradio.com/api/v1");
    // macOS publishes one disk image per architecture, so the name carries the suffix there.
    #[cfg(target_os = "macos")]
    let config = config.with_asset_suffix(if cfg!(target_arch = "aarch64") {
        "-arm64"
    } else {
        "-x64"
    });
    ui::run_update_check(Arc::new(config), parent, UpdateChannel::Stable, trigger);
}

/// Texts that ship-shape (the update dialogs) and wx-utils (Yes/No/Cancel) translate
/// through BlindFB's catalog. Never called: it only lists them for `cargo xtask i18n`,
/// which reads `t!` calls. `%s` is filled in by ship-shape after translation.
#[allow(dead_code)]
fn library_messages() -> Vec<String> {
    use patois::t;
    vec![
        t("Update to %s"),
        t("A new version of %s is available. Here's what's new:"),
        t("&Yes"),
        t("&No"),
        t("Cancel"),
        t("No release notes provided."),
        t("%s Update"),
        t("Downloading update..."),
        t("Preparing update..."),
        t("Error"),
        t("Update Ready"),
        t("No updates available."),
        t("No updates available. Latest version: %s"),
        t("Info"),
        t("Security verification failed. The update might have been tampered with: %s"),
        t("Security Error"),
        t("The update server returned HTTP error %s."),
        t("Could not reach the update server: %s"),
        t("Update cancelled."),
        t("Current version is not a valid semver."),
        t("Latest release tag is not a valid semver."),
        t("Latest release does not include downloadable assets."),
        t("Update is available but no matching download asset or signature was found."),
        t("Failed to parse release JSON"),
        t("Invalid public key"),
        t("Invalid signature"),
        t("Failed to read signature"),
        t("Signature verification failed"),
        t("Failed to create private download directory"),
        t("Failed to create file"),
        t("Failed to write file"),
        t("Failed to read file for verification"),
        t("Failed to rename verified file"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compiled_key_is_a_minisign_public_key() {
        assert!(PUBLIC_KEY.starts_with("RW"), "got {PUBLIC_KEY:?}");
    }
}
