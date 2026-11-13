//! Native controls for Facebook's participation review; submission remains explicit.
use blindfb_core::posts::Participation;
use patois::t;
use serde_json::{Value, json};
use wx_utils::show_warning;
use wxdragon::prelude::*;

enum Answer {
    Text(TextCtrl),
    Choice(Choice),
    Check(CheckBox),
}

pub fn show(parent: &Dialog, form: &Participation) -> Option<Value> {
    let dialog = Dialog::builder(parent, &t("Participation questions"))
        .with_size(740, 650)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&dialog).with_label(&t("Questions and group &rules")).build();
    let rules = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    rules.set_name(&t("Questions and group rules"));
    rules.change_value(&form.text);
    root.add(&label, 0, SizerFlag::All, 6);
    root.add(&rules, 1, SizerFlag::Expand | SizerFlag::All, 6);
    // ScrolledWindow's default style omits the flag that lets Tab reach its children.
    let scroll = ScrolledWindow::builder(&dialog)
        .with_style(
            ScrolledWindowStyle::VScroll
                | ScrolledWindowStyle::from_bits_retain(PanelStyle::TabTraversal.bits()),
        )
        .build();
    scroll.set_scroll_rate(0, 12);
    let fields = BoxSizer::builder(Orientation::Vertical).build();
    let mut controls = Vec::new();
    for (index, field) in form.fields.iter().enumerate() {
        let answer = form.answers.get(index);
        if field.kind != "check" {
            let label = StaticText::builder(&scroll).with_label(&field.prompt).build();
            fields.add(&label, 0, SizerFlag::All, 6);
        }
        let control = match field.kind.as_str() {
            "choice" => {
                let choice = Choice::builder(&scroll).build();
                choice.set_name(&field.prompt);
                choice.append(&t("Choose an answer"));
                for option in &field.choices {
                    choice.append(option);
                }
                let selected = answer
                    .and_then(Value::as_u64)
                    .filter(|i| *i < field.choices.len() as u64)
                    .map_or(0, |i| i as u32 + 1);
                choice.set_selection(selected);
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
    if !form.error.is_empty() {
        let error = StaticText::builder(&dialog).with_label(&form.error).build();
        root.add(&error, 0, SizerFlag::Expand | SizerFlag::All, 6);
    }
    let note = StaticText::builder(&dialog)
        .with_label(&t("Submit sends these answers to the group admins. Your comment stays in its draft until confirmed."))
        .build();
    root.add(&note, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let submit = Button::builder(&dialog).with_id(ID_OK).with_label(&t("&Submit answers")).build();
    let cancel = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Cancel")).build();
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add(&submit, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    root.add_sizer(&buttons, 0, SizerFlag::All, 6);
    dialog.set_sizer(root, true);
    let answers = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result = answers.clone();
    match &controls[0] {
        Answer::Text(c) => c.set_focus(),
        Answer::Choice(c) => c.set_focus(),
        Answer::Check(c) => c.set_focus(),
    }
    submit.on_click(move |_| {
        let mut values = Vec::new();
        for control in &controls {
            let value = match control {
                Answer::Text(c) if !c.get_value().trim().is_empty() => json!(c.get_value()),
                Answer::Choice(c) if c.get_selection().is_some_and(|i| i > 0) => {
                    json!(c.get_selection().unwrap() - 1)
                }
                Answer::Check(c) => json!(c.get_value()),
                _ => {
                    show_warning(
                        &dialog,
                        t("Answer every question before submitting."),
                        &t("Participation questions"),
                    );
                    match control {
                        Answer::Text(c) => c.set_focus(),
                        Answer::Choice(c) => c.set_focus(),
                        Answer::Check(c) => c.set_focus(),
                    }
                    return;
                }
            };
            values.push(value);
        }
        *result.borrow_mut() = Some(Value::Array(values));
        dialog.end_modal(ID_OK);
    });
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    dialog.show_modal();
    dialog.destroy();
    answers.take()
}
