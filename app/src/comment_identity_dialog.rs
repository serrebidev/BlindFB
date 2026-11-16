//! Native identity choices and nickname setup; changing identity never sends a comment.
use blindfb_core::posts::IdentityPanel;
use blindfb_core::tf;
use patois::t;
use serde_json::{json, Value};
use wx_utils::show_warning;
use wxdragon::prelude::*;

enum Answer {
    Text(TextCtrl),
    Choice(Choice),
    Check(CheckBox),
}

impl Answer {
    fn focus(&self) {
        match self {
            Self::Text(c) => c.set_focus(),
            Self::Choice(c) => c.set_focus(),
            Self::Check(c) => c.set_focus(),
        }
    }
}

pub fn show(parent: &Dialog, panel: &IdentityPanel) -> Option<(Option<String>, Value)> {
    let dialog = Dialog::builder(parent, &t("Comment identity"))
        .with_size(740, 650)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&dialog).with_label(&t("Identity &details")).build();
    let details = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly).build();
    details.set_name(&t("Identity details"));
    details.change_value(&panel.text);
    root.add(&label, 0, SizerFlag::All, 6);
    root.add(&details, 1, SizerFlag::Expand | SizerFlag::All, 6);
    let scroll = ScrolledWindow::builder(&dialog)
        .with_style(ScrolledWindowStyle::VScroll
            | ScrolledWindowStyle::from_bits_retain(PanelStyle::TabTraversal.bits()))
        .build();
    scroll.set_scroll_rate(0, 12);
    let fields = BoxSizer::builder(Orientation::Vertical).build();
    let enabled: Vec<_> = panel.options.iter().filter(|option| option.enabled).cloned().collect();
    let choice = if !enabled.is_empty() {
        let label = StaticText::builder(&scroll).with_label(&t("Identity or &action")).build();
        fields.add(&label, 0, SizerFlag::All, 6);
        let choice = Choice::builder(&scroll).build();
        choice.set_name(&t("Identity or action"));
        choice.append(&t("Choose an action"));
        for option in &enabled { choice.append(&option.label); }
        choice.set_selection(enabled.iter().position(|option| option.selected)
            .map_or(0, |index| index as u32 + 1));
        fields.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 6);
        Some(choice)
    } else { None };
    for option in panel.options.iter().filter(|option| !option.enabled) {
        let label = if option.selected {
            tf!("{identity} (current, unavailable)", identity = option.label)
        } else { tf!("{identity} (unavailable)", identity = option.label) };
        let label = StaticText::builder(&scroll).with_label(&label).build();
        fields.add(&label, 0, SizerFlag::All, 6);
    }
    let mut controls = Vec::new();
    for (index, field) in panel.fields.iter().enumerate() {
        let answer = panel.answers.get(index);
        if field.kind != "check" {
            let label = StaticText::builder(&scroll).with_label(&field.prompt).build();
            fields.add(&label, 0, SizerFlag::All, 6);
        }
        let control = match field.kind.as_str() {
            "choice" => {
                let choice = Choice::builder(&scroll).build();
                choice.set_name(&field.prompt);
                choice.append(&t("Choose an answer"));
                for option in &field.choices { choice.append(option); }
                choice.set_selection(answer.and_then(Value::as_u64).map_or(0, |i| i as u32 + 1));
                fields.add(&choice, 0, SizerFlag::Expand | SizerFlag::All, 6);
                Answer::Choice(choice)
            }
            "check" => {
                let check = CheckBox::builder(&scroll).with_label(&field.prompt).build();
                check.set_value(answer.and_then(Value::as_bool).unwrap_or(false));
                fields.add(&check, 0, SizerFlag::All, 6);
                Answer::Check(check)
            }
            _ => {
                let text = TextCtrl::builder(&scroll).build();
                text.set_name(&field.prompt);
                text.change_value(answer.and_then(Value::as_str).unwrap_or_default());
                fields.add(&text, 0, SizerFlag::Expand | SizerFlag::All, 6);
                Answer::Text(text)
            }
        };
        controls.push(control);
    }
    scroll.set_sizer(fields, true);
    root.add(&scroll, 2, SizerFlag::Expand | SizerFlag::All, 6);
    if !panel.error.is_empty() {
        let error = StaticText::builder(&dialog).with_label(&panel.error).build();
        root.add(&error, 0, SizerFlag::Expand | SizerFlag::All, 6);
    }
    let note = StaticText::builder(&dialog)
        .with_label(&t("Continue changes your Facebook identity or completes its setup. Your comment draft is kept."))
        .build();
    root.add(&note, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let submit = Button::builder(&dialog).with_id(ID_OK).with_label(&t("&Continue")).build();
    submit.enable(!enabled.is_empty() || (!panel.fields.is_empty() && panel.options.is_empty()));
    let cancel = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Cancel")).build();
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add(&submit, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::All, 6);
    dialog.set_sizer(root, true);
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let answers = result.clone();
    if let Some(choice) = choice { choice.set_focus(); }
    else if let Some(control) = controls.first() { control.focus(); }
    else { cancel.set_focus(); }
    submit.on_click(move |_| {
        let key = if let Some(choice) = choice {
            let selected = choice.get_selection().filter(|index| *index > 0)
                .and_then(|index| enabled.get(index as usize - 1));
            let Some(selected) = selected else {
                show_warning(&dialog, t("Choose an action before continuing."), &t("Comment identity"));
                choice.set_focus();
                return;
            };
            Some(selected.key.clone())
        } else { None };
        let values = controls.iter().map(|control| match control {
            Answer::Text(c) => json!(c.get_value()),
            Answer::Choice(c) => c.get_selection().filter(|i| *i > 0)
                .map_or(Value::Null, |i| json!(i - 1)),
            Answer::Check(c) => json!(c.get_value()),
        }).collect();
        *answers.borrow_mut() = Some((key, Value::Array(values)));
        dialog.end_modal(ID_OK);
    });
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    dialog.show_modal();
    dialog.destroy();
    result.take()
}
