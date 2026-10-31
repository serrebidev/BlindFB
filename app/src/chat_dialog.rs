//! The chat window: one conversation (a person's chat or a group chat) in
//! its own dialog, opened by activating a row in the Chats list — the same
//! shape as the group window. Messages load newest-last; the composer sends
//! with Enter or the Send button, files attach, and Load more walks back
//! through the history.
use super::{Job, Tag, Task, Ui, needs_messenger_unlock};
use blindfb_core::fb::{Record, read_args, send_args};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use wxdragon::prelude::*;

const PAGE: u32 = 50;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct ChatWindow {
    session: u64,
    messages: ListBox,
    compose: TextCtrl,
    status: TextCtrl,
    state: Arc<Mutex<ChatState>>,
}

struct ChatState {
    id: String,
    name: String,
    count: u32,
    files: Vec<String>,
}

/// The conversation a chat row opens: its backend ref (field 0) is the
/// identity — names repeat and a name slot can hold a presence label — with
/// the display name in field 3 and the group flag in field 1. A row with
/// only a name opens by that name, as before.
fn chat_target(record: &Record) -> Option<(String, String, bool)> {
    let id = record.field(0);
    let name = record.field(3);
    let (id, name) = if !id.is_empty() {
        (id.to_owned(), if name.is_empty() { id.to_owned() } else { name.to_owned() })
    } else if !name.is_empty() {
        (name.to_owned(), name.to_owned())
    } else {
        return None;
    };
    Some((id, name, record.field(1).contains('g')))
}

impl ChatWindow {
    fn note(&self, text: &str) {
        self.status.change_value(text);
    }

    fn load(&self, ui: &Ui) {
        let (id, count) = {
            let state = self.state.lock().unwrap();
            (state.id.clone(), state.count)
        };
        ui.send(Job {
            task: Task::Fb(read_args(&id, count), None),
            tag: Tag::ChatMessages(self.session),
        });
    }

    fn send_message(&self, ui: &Ui) {
        let text = self.compose.get_value();
        let (id, files) = {
            let state = self.state.lock().unwrap();
            (state.id.clone(), state.files.clone())
        };
        if text.trim().is_empty() && files.is_empty() {
            return;
        }
        self.note("Sending…");
        ui.send(Job {
            task: Task::Fb(send_args(&id, &text, &files), None),
            tag: Tag::ChatWindowSent(self.session),
        });
    }
}

pub(super) fn open(ui: &Ui, record: &Record) {
    if ui.chat_window.lock().unwrap().is_some() {
        return;
    }
    let Some((id, name, group)) = chat_target(record) else {
        ui.status("This chat row has no name or link, so its window cannot open.");
        return;
    };
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let title = if group { format!("Group chat: {name}") } else { format!("Chat: {name}") };
    let dialog = Dialog::builder(&ui.frame, &title)
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let messages_label = StaticText::builder(&dialog).with_label("&Messages").build();
    let messages = ListBox::builder(&dialog).build();
    messages.set_name("Messages in this chat, oldest first");
    let more = Button::builder(&dialog).with_label("Load &more").build();
    let refresh = Button::builder(&dialog).with_label("&Refresh").build();
    let compose_label = StaticText::builder(&dialog).with_label("Me&ssage").build();
    let compose = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    compose.set_name(&format!("Message to {name}. Enter sends"));
    let attach = Button::builder(&dialog).with_label("Attach &files").build();
    let send = Button::builder(&dialog).with_label("&Send").build();
    let status_label = StaticText::builder(&dialog).with_label("Chat status").build();
    let status = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_name("Chat status");
    status.enable_focus_from_keyboard();
    status.change_value("Loading messages…");
    let close = Button::builder(&dialog).with_id(ID_CANCEL).with_label("C&lose").build();
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    actions.add(&more, 0, SizerFlag::All, 4);
    actions.add(&refresh, 0, SizerFlag::All, 4);
    actions.add(&attach, 0, SizerFlag::All, 4);
    actions.add(&send, 0, SizerFlag::All, 4);
    actions.add(&close, 0, SizerFlag::All, 4);
    root.add(&messages_label, 0, SizerFlag::All, 4);
    root.add(&messages, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&compose_label, 0, SizerFlag::All, 4);
    root.add(&compose, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 4);
    root.add(&status_label, 0, SizerFlag::All, 4);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 4);
    dialog.set_sizer(root, true);

    let window = ChatWindow {
        session,
        messages,
        compose,
        status,
        state: Arc::new(Mutex::new(ChatState {
            id,
            name: name.clone(),
            count: PAGE,
            files: Vec::new(),
        })),
    };
    *ui.chat_window.lock().unwrap() = Some(window.clone());
    window.load(ui);

    let (w, u) = (window.clone(), ui.clone());
    compose.on_text_enter(move |_| w.send_message(&u));
    let (w, u) = (window.clone(), ui.clone());
    send.on_click(move |_| w.send_message(&u));
    let (w, u) = (window.clone(), ui.clone());
    refresh.on_click(move |_| {
        w.note("Refreshing…");
        w.load(&u);
    });
    let (w, u) = (window.clone(), ui.clone());
    more.on_click(move |_| {
        w.state.lock().unwrap().count += PAGE;
        w.note("Loading more…");
        w.load(&u);
    });
    let (w, u) = (window.clone(), ui.clone());
    attach.on_click(move |_| {
        let dialog = FileDialog::builder(&u.frame).build();
        if dialog.show_modal() == ID_OK {
            if let Some(path) = dialog.get_path() {
                let mut state = w.state.lock().unwrap();
                state.files.push(path);
                w.note(&format!("{} file(s) attached", state.files.len()));
            }
        }
    });
    close.on_click(move |_| dialog.end_modal(ID_CANCEL));
    let d = dialog;
    dialog.bind_internal(EventType::CLOSE_WINDOW, move |event| {
        event.skip(false);
        d.end_modal(ID_CANCEL);
    });

    messages.set_focus();
    dialog.show_modal();
    ui.chat_window.lock().unwrap().take();
    dialog.destroy();
    ui.items.set_focus();
}

pub(super) fn deliver_messages(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.chat_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(raw) => {
            let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
            window.messages.clear();
            for line in &lines {
                window.messages.append(line);
            }
            if !lines.is_empty() {
                window.messages.set_selection((lines.len() - 1) as u32, true);
            }
            let name = window.state.lock().unwrap().name.clone();
            window.note(&format!("{name}: {} message(s).", lines.len()));
        }
        Err(e) => {
            if needs_messenger_unlock(&e) {
                window.note(
                    "Messenger is locked. Close this window and use Chat, Unlock encrypted chats, then reopen the chat.",
                );
            } else {
                window.note(&format!("Messages could not load: {e}"));
            }
        }
    }
}

pub(super) fn deliver_sent(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.chat_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(_) => {
            window.compose.clear();
            window.state.lock().unwrap().files.clear();
            window.note("Message sent.");
            window.load(ui);
        }
        Err(e) => window.note(&format!("Could not send: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[&str]) -> Record {
        Record {
            fields: fields.iter().map(|s| s.to_string()).collect(),
            raw: fields.join("\t"),
        }
    }

    #[test]
    fn a_chat_row_opens_by_ref_with_its_name_and_kind() {
        let (id, name, group) =
            chat_target(&record(&["/messages/t/42", "g", "now", "The Group"])).unwrap();
        assert_eq!(id, "/messages/t/42");
        assert_eq!(name, "The Group");
        assert!(group);
        let (id, name, group) =
            chat_target(&record(&["/messages/t/7", "!", "1h", "Alice"])).unwrap();
        assert_eq!(id, "/messages/t/7");
        assert_eq!(name, "Alice");
        assert!(!group);
    }

    #[test]
    fn a_chat_row_with_only_a_name_opens_by_name() {
        let (id, name, _) = chat_target(&record(&["", "", "", "Bob"])).unwrap();
        assert_eq!(id, "Bob");
        assert_eq!(name, "Bob");
    }

    #[test]
    fn a_chat_row_with_nothing_opens_nothing() {
        assert!(chat_target(&record(&["", "", "", ""])).is_none());
        assert!(chat_target(&record(&[])).is_none());
    }
}
