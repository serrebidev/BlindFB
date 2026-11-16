//! Invite friends to a group by exact name, one name per line. Names are
//! matched exactly by the backend; a fuzzy or missing name sends nothing.
use patois::t;
use std::cell::RefCell;
use std::rc::Rc;
use wx_utils::show_warning;
use wxdragon::prelude::*;

/// One friend per line, trimmed, blanks dropped, duplicates removed
/// (case-insensitively, keeping the first spelling).
pub(crate) fn friend_names(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.chars().any(char::is_control))
        .filter(|name| seen.insert(name.to_lowercase()))
        .map(str::to_owned)
        .collect()
}

pub(super) fn show(ui: &super::Ui, group: &str) -> Option<Vec<String>> {
    let dialog = Dialog::builder(&ui.frame, &t("Invite friends"))
        .with_size(520, 340)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let intro = StaticText::builder(&dialog)
        .with_label(&t(
            "Enter the full name of each friend to invite, one per line.",
        ))
        .build();
    intro.wrap(470);
    let names = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine)
        .build();
    names.set_name(&t("Friends to invite, one per line"));
    let note = StaticText::builder(&dialog)
        .with_label(&t("Names are matched exactly. If a name does not match exactly one friend, no invitations are sent."))
        .build();
    note.wrap(470);
    let send = Button::builder(&dialog)
        .with_id(ID_OK)
        .with_label(&t("&Send invitations"))
        .build();
    let cancel = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("Cancel"))
        .build();
    send.set_default();
    root.add(&intro, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&names, 1, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&note, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add(&send, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::AlignRight | SizerFlag::All, 6);
    dialog.set_sizer_and_fit(root, true);

    let picked: Rc<RefCell<Option<Vec<String>>>> = Rc::new(RefCell::new(None));
    let picked_clone = Rc::clone(&picked);
    send.on_click(move |_| {
        let list = friend_names(&names.get_value());
        if list.is_empty() {
            show_warning(
                &dialog,
                t("Enter at least one friend name, one per line."),
                &t("Invite friends"),
            );
            names.set_focus();
            return;
        }
        if list.len() > 50 {
            show_warning(
                &dialog,
                t("Invite at most 50 friends at once; split the list into smaller batches."),
                &t("Invite friends"),
            );
            names.set_focus();
            return;
        }
        *picked_clone.borrow_mut() = Some(list);
        dialog.end_modal(ID_OK);
    });
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    *ui.group_modal.borrow_mut() = Some(dialog);
    names.set_focus();
    dialog.show_modal();
    ui.group_modal.borrow_mut().take();
    dialog.destroy();
    let _ = group;
    picked.take()
}

#[cfg(test)]
mod tests {
    use super::friend_names;

    #[test]
    fn names_are_one_per_line_trimmed_and_deduplicated() {
        assert_eq!(
            friend_names("Alice Friend\nBob Friend"),
            ["Alice Friend", "Bob Friend"]
        );
        assert!(friend_names("  \n\n").is_empty());
        assert_eq!(
            friend_names(" Alice Friend \nAlice Friend\nalice friend\nBob Friend"),
            ["Alice Friend", "Bob Friend"]
        );
        assert_eq!(friend_names("A\nB\r\nC"), ["A", "B", "C"]);
    }

    #[test]
    fn control_characters_in_a_name_are_dropped() {
        assert_eq!(friend_names("Good Name\nBad\tName"), ["Good Name"]);
    }
}
