//! Native group creation form; only its explicit Create action submits anything.
use patois::t;
use std::cell::RefCell;
use std::rc::Rc;
use wx_utils::show_warning;
use wxdragon::prelude::*;

fn form_values(name: &str, privacy: Option<u32>) -> Option<(String, &'static str)> {
    let name = name.trim();
    if name.is_empty() || name.encode_utf16().count() > 75 || name.chars().any(char::is_control) {
        return None;
    }
    let privacy = match privacy? {
        0 => "public",
        1 => "private",
        _ => return None,
    };
    Some((name.to_owned(), privacy))
}

pub(super) fn show(ui: &super::Ui) -> Option<(String, &'static str, bool)> {
    let dialog = Dialog::builder(&ui.frame, &t("New group"))
        .with_size(500, 320)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let name_label = StaticText::builder(&dialog).with_label(&t("Group &name")).build();
    let name = TextCtrl::builder(&dialog).build();
    name.set_name(&t("Group name"));
    name.set_max_length(75);
    let privacy_label = StaticText::builder(&dialog).with_label(&t("Group &privacy")).build();
    let privacy = Choice::builder(&dialog).build();
    privacy.set_name(&t("Group privacy"));
    privacy.append(&t("Public"));
    privacy.append(&t("Private"));
    privacy.set_selection(1);
    let visibility_label = StaticText::builder(&dialog)
        .with_label(&t("Private group &visibility")).build();
    let visibility = Choice::builder(&dialog).build();
    visibility.set_name(&t("Private group visibility"));
    visibility.append(&t("Visible"));
    visibility.append(&t("Hidden"));
    visibility.set_selection(0);
    let note = StaticText::builder(&dialog)
        .with_label(&t("Visible groups can be found in search. Hidden groups can only be found by members and invited people."))
        .build();
    note.wrap(460);
    let create = Button::builder(&dialog).with_id(ID_OK).with_label(&t("&Create group")).build();
    let cancel = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Cancel")).build();
    create.set_default();
    root.add(&name_label, 0, SizerFlag::All, 6);
    root.add(&name, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&privacy_label, 0, SizerFlag::All, 6);
    root.add(&privacy, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&visibility_label, 0, SizerFlag::All, 6);
    root.add(&visibility, 0, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&note, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add(&create, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::All, 6);
    dialog.set_sizer_and_fit(root, true);
    privacy.on_selection_changed(move |_| {
        let private = privacy.get_selection() == Some(1);
        visibility.enable(private);
        if !private { visibility.set_selection(0); }
    });
    let picked = Rc::new(RefCell::new(None));
    let result = picked.clone();
    create.on_click(move |_| {
        let Some((name_value, privacy_value)) = form_values(&name.get_value(), privacy.get_selection())
        else {
            show_warning(&dialog,
                t("Enter a group name of up to 75 characters and choose its privacy."),
                &t("New group"));
            name.set_focus();
            return;
        };
        let hidden = privacy_value == "private" && visibility.get_selection() == Some(1);
        *result.borrow_mut() = Some((name_value, privacy_value, hidden));
        dialog.end_modal(ID_OK);
    });
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    *ui.group_modal.borrow_mut() = Some(dialog);
    name.set_focus();
    dialog.show_modal();
    ui.group_modal.borrow_mut().take();
    dialog.destroy();
    picked.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_form_requires_a_name_and_known_privacy() {
        assert_eq!(form_values("  ", Some(1)), None);
        assert_eq!(form_values("A group", None), None);
        assert_eq!(form_values("A group", Some(2)), None);
        assert_eq!(form_values("A\ngroup", Some(1)), None);
        assert_eq!(form_values("  A group  ", Some(1)), Some(("A group".into(), "private")));
        assert_eq!(form_values("A group", Some(0)), Some(("A group".into(), "public")));
    }

    #[test]
    fn name_limit_matches_facebook_utf16_length() {
        assert!(form_values(&"a".repeat(75), Some(1)).is_some());
        assert_eq!(form_values(&"a".repeat(76), Some(1)), None);
        assert!(form_values(&"😀".repeat(37), Some(1)).is_some());
        assert_eq!(form_values(&"😀".repeat(38), Some(1)), None);
    }
}
