//! Help, About: the description as text, then every project link as its own
//! button, so Tab reaches each one and Enter or Space opens it.
use patois::t;
use wxdragon::prelude::*;

const FORGEJO: &str = "https://git.serrebiradio.com/serrebi/BlindFB";

/// About's links, in the order Tab reaches them.
pub(crate) fn links() -> Vec<(String, &'static str)> {
    vec![
        (t("GitHub: serrebidev/BlindFB"), "https://github.com/serrebidev/BlindFB"),
        (t("Forgejo: serrebi/BlindFB"), FORGEJO),
        (t("Projects: projects.serrebiradio.com"), "https://projects.serrebiradio.com"),
        (t("Telegram: SerrebiProjects"), "https://t.me/SerrebiProjects"),
        (
            t("Full credits"),
            "https://git.serrebiradio.com/serrebi/BlindFB/src/branch/main/CREDITS.md",
        ),
    ]
}

pub(super) fn show(frame: &Frame) {
    let dialog = Dialog::builder(frame, &t("About BlindFB")).build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let text = StaticText::builder(&dialog)
        .with_label(&format!(
            "BlindFB {}\n\n{}\n\n{}\n{}",
            env!("CARGO_PKG_VERSION"),
            t("An accessible Facebook and Messenger client for screen reader users.\nBuilt with wxDragon on the facebook-cli (fb) backend."),
            t("Author: serrebidev."),
            t("Copyright (c) 2026 Brandon Bracey. MIT licence."),
        ))
        .build();
    text.wrap(470);
    root.add(&text, 0, SizerFlag::Expand | SizerFlag::All, 10);
    for (label, url) in links() {
        let link = Button::builder(&dialog).with_label(&label).build();
        link.on_click(move |_| super::open_url(url));
        root.add(&link, 0, SizerFlag::Expand | SizerFlag::All, 3);
    }
    let close = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Close")).build();
    close.set_default();
    root.add(&close, 0, SizerFlag::AlignRight | SizerFlag::All, 10);
    dialog.set_sizer_and_fit(root, true);
    dialog.centre();
    close.set_focus();
    dialog.show_modal();
    dialog.destroy();
}

#[cfg(test)]
mod tests {
    #[test]
    fn about_has_every_project_link() {
        let urls: Vec<&str> = super::links().into_iter().map(|(_, url)| url).collect();
        assert_eq!(
            urls,
            [
                "https://github.com/serrebidev/BlindFB",
                "https://git.serrebiradio.com/serrebi/BlindFB",
                "https://projects.serrebiradio.com",
                "https://t.me/SerrebiProjects",
                "https://git.serrebiradio.com/serrebi/BlindFB/src/branch/main/CREDITS.md",
            ]
        );
    }
}
