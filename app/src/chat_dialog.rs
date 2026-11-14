//! The chat window: one conversation (a person's chat or a group chat) in
//! its own dialog, opened by activating a row in the Chats list — the same
//! shape as the group window. Messages load newest-last; the composer sends
//! with Enter or the Send button, files attach, and Load more walks back
//! through the history.
use super::{message_match_text, needs_messenger_unlock, Job, Tag, Task, Ui, REACTIONS};
use blindfb_core::fb::{msg_react_args, read_args, reply_args, send_args, Record};
use blindfb_core::{ntf, tf};
use patois::t;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use wxdragon::prelude::*;

const PAGE: u32 = 50;
const ID_MSG_REPLY: i32 = 13101;
const ID_MSG_COPY: i32 = 13102;
const ID_MSG_REACT: i32 = 13120; // + index into REACTIONS

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct ChatWindow {
    session: u64,
    /// The dialog itself, so popups from the results timer can be parented
    /// to it while it is up.
    dialog: Dialog,
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
    /// Text of the message the next send replies to (quotes).
    reply_to: Option<(String, usize)>,
    /// True when the next delivery should land on the newest message
    /// (open, refresh, after sending). Load more sets it false: the reader
    /// is walking history and must keep their place.
    follow_tail: bool,
}

/// Where the selection lands after the message list is replaced.
/// Following the tail (or no prior selection) means the newest message.
/// Otherwise the previously selected line is found again in the new list:
/// history loads prepend, so the same line sits further down by the growth,
/// and a message whose text repeats resolves to the occurrence nearest that
/// expected spot. A vanished line keeps its position, clamped to the list.
fn restored_selection(
    old: &[String],
    new: &[String],
    old_sel: Option<usize>,
    follow_tail: bool,
) -> Option<usize> {
    if new.is_empty() {
        return None;
    }
    let last = new.len() - 1;
    let Some(old_sel) = old_sel.filter(|_| !follow_tail) else {
        return Some(last);
    };
    let Some(line) = old.get(old_sel) else {
        return Some(last);
    };
    let expected = if new.len() >= old.len() {
        old_sel + (new.len() - old.len())
    } else {
        old_sel
    }
    .min(last);
    if new[expected] == *line {
        return Some(expected);
    }
    new.iter()
        .enumerate()
        .filter(|(_, candidate)| *candidate == line)
        .map(|(index, _)| (index, index.abs_diff(expected)))
        .min_by_key(|(_, distance)| *distance)
        .map_or(Some(expected), |(index, _)| Some(index))
}

/// The menu text of a reaction keyword; the keyword itself goes to the backend.
fn reaction_label(reaction: &str) -> String {
    match reaction {
        "like" => t("Like"),
        "love" => t("Love"),
        "care" => t("Care"),
        "haha" => t("Haha"),
        "wow" => t("Wow"),
        "sad" => t("Sad"),
        "angry" => t("Angry"),
        other => other[..1].to_uppercase() + &other[1..],
    }
}

/// The backend prints a date header (`# Sep 5, 2026`) before each day's
/// messages; the status line counts messages, so headers are not messages.
fn message_count(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| !line.trim_start().starts_with('#'))
        .count()
}

/// The conversation a chat row opens: its backend ref (field 0) is the
/// identity — names repeat and a name slot can hold a presence label — with
/// the display name in field 3 and the group flag in field 1. A row with
/// only a name opens by that name, as before.
fn chat_target(record: &Record) -> Option<(String, String, bool)> {
    let id = record.field(0);
    let name = record.field(3);
    let (id, name) = if !id.is_empty() {
        (
            id.to_owned(),
            if name.is_empty() {
                id.to_owned()
            } else {
                name.to_owned()
            },
        )
    } else if !name.is_empty() {
        (name.to_owned(), name.to_owned())
    } else {
        return None;
    };
    Some((id, name, record.field(1).contains('g')))
}

impl ChatWindow {
    /// The dialog, so the frame's results timer can parent popups to it
    /// while it is up.
    pub(super) fn dialog(&self) -> Dialog {
        self.dialog
    }

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
        let (id, files, reply_to) = {
            let state = self.state.lock().unwrap();
            (
                state.id.clone(),
                state.files.clone(),
                state.reply_to.clone(),
            )
        };
        if text.trim().is_empty() && files.is_empty() {
            return;
        }
        self.note(&t("Sending…"));
        let args = match &reply_to {
            Some((target, nth)) => reply_args(&id, &text, &files, target, *nth),
            None => send_args(&id, &text, &files),
        };
        ui.send(Job {
            task: Task::Fb(args, None),
            tag: Tag::ChatWindowSent(self.session),
        });
    }

    fn selected_match(&self) -> Option<(String, usize)> {
        let index = self.messages.get_selection()? as usize;
        let mut texts = Vec::new();
        let mut i = 0;
        while let Some(line) = self.messages.get_string(i) {
            texts.push(message_match_text(&line));
            i += 1;
        }
        super::match_nth(&texts, index)
    }

    fn menu_action(&self, ui: &Ui, action: i32) {
        match action {
            ID_MSG_COPY => {
                if let Some(index) = self.messages.get_selection() {
                    if let Some(line) = self.messages.get_string(index) {
                        let done = Clipboard::get().set_text(&line);
                        self.note(&if done {
                            t("Message copied.")
                        } else {
                            t("Could not copy.")
                        });
                    }
                }
            }
            ID_MSG_REPLY => {
                if self.state.lock().unwrap().reply_to.is_some() {
                    self.state.lock().unwrap().reply_to = None;
                    self.note(&t("Reply cancelled. Your next message sends normally."));
                    return;
                }
                let Some((text, nth)) = self.selected_match() else {
                    self.note(&t("That message has no text to reply to."));
                    return;
                };
                self.state.lock().unwrap().reply_to = Some((text.clone(), nth));
                self.note(&tf!(
                    "Replying to: {text}. Write your reply and send as usual.",
                    text = text
                ));
                self.compose.set_focus();
            }
            id if (ID_MSG_REACT..ID_MSG_REACT + REACTIONS.len() as i32).contains(&id) => {
                let reaction = REACTIONS[(id - ID_MSG_REACT) as usize];
                if reaction == "none" {
                    return;
                }
                let Some((text, nth)) = self.selected_match() else {
                    self.note(&t("That message has no text to react to."));
                    return;
                };
                let id = self.state.lock().unwrap().id.clone();
                ui.send(Job {
                    task: Task::Fb(msg_react_args(&id, reaction, Some(&text), nth), None),
                    tag: Tag::Text,
                });
                // TRANSLATORS: {reaction} is a reaction name in lower case (like, love…)
                self.note(&tf!(
                    "Reacting {reaction} to: {text}",
                    reaction = reaction_label(reaction).to_lowercase(),
                    text = text
                ));
            }
            _ => {}
        }
    }
}

pub(super) fn open(ui: &Ui, record: &Record) {
    if ui.chat_window.lock().unwrap().is_some() {
        ui.status(&t("A chat window is already open. Close it to open another chat."));
        return;
    }
    let Some((id, name, group)) = chat_target(record) else {
        ui.status(&t("This chat row has no name or link, so its window cannot open."));
        return;
    };
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let title = if group {
        tf!("Group chat: {name}", name = name)
    } else {
        tf!("Chat: {name}", name = name)
    };
    let dialog = Dialog::builder(&ui.frame, &title)
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let messages_label = StaticText::builder(&dialog).with_label(&t("&Messages")).build();
    let messages = ListBox::builder(&dialog).build();
    messages.set_name(&t("Messages in this chat, oldest first"));
    let more = Button::builder(&dialog).with_label(&t("Load &more")).build();
    let refresh = Button::builder(&dialog).with_label(&t("&Refresh")).build();
    let compose_label = StaticText::builder(&dialog).with_label(&t("Me&ssage")).build();
    let compose = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    compose.set_name(&tf!("Message to {name}. Enter sends", name = name));
    let attach = Button::builder(&dialog).with_label(&t("Attach &files")).build();
    let manage = Button::builder(&dialog).with_label(&t("A&ttachments…")).build();
    manage.set_name(&t("Attachments. Review or remove attached files"));
    let send = Button::builder(&dialog).with_label(&t("&Send")).build();
    let status_label = StaticText::builder(&dialog)
        .with_label(&t("Chat status"))
        .build();
    let status = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_name(&t("Chat status"));
    status.enable_focus_from_keyboard();
    status.change_value(&t("Loading messages…"));
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    actions.add(&more, 0, SizerFlag::All, 4);
    actions.add(&refresh, 0, SizerFlag::All, 4);
    actions.add(&attach, 0, SizerFlag::All, 4);
    actions.add(&manage, 0, SizerFlag::All, 4);
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
        dialog,
        messages,
        compose,
        status,
        state: Arc::new(Mutex::new(ChatState {
            id,
            name: name.clone(),
            count: PAGE,
            files: Vec::new(),
            reply_to: None,
            follow_tail: true,
        })),
    };
    *ui.chat_window.lock().unwrap() = Some(window.clone());
    window.load(ui);

    let w = window.clone();
    messages.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        if w.messages.get_selection().is_none() {
            return;
        }
        let replying = w.state.lock().unwrap().reply_to.is_some();
        let mut menu = Menu::builder()
            .append_item(
                ID_MSG_REPLY,
                &if replying {
                    t("Cancel &reply")
                } else {
                    t("&Reply")
                },
                &t("Quote this message in your next send"),
            )
            .append_item(
                ID_MSG_COPY,
                &t("Cop&y message"),
                &t("Copy the message text"),
            )
            .build();
        let react = Menu::builder().build();
        for (i, reaction) in REACTIONS.iter().enumerate() {
            if *reaction == "none" {
                continue;
            }
            let label = reaction_label(reaction);
            react.append(ID_MSG_REACT + i as i32, &label, "", ItemKind::Normal);
        }
        menu.append_submenu(react, &t("Re&act"), &t("React to this message"));
        w.messages.popup_menu(&mut menu, None);
    });
    let (w, u) = (window.clone(), ui.clone());
    dialog.bind_internal(EventType::MENU, move |event| {
        let id = event.get_id();
        if id == ID_MSG_REPLY
            || id == ID_MSG_COPY
            || (ID_MSG_REACT..ID_MSG_REACT + REACTIONS.len() as i32).contains(&id)
        {
            event.skip(false);
            w.menu_action(&u, id);
        } else {
            event.skip(true);
        }
    });
    let (w, u) = (window.clone(), ui.clone());
    compose.on_text_enter(move |_| w.send_message(&u));
    let (w, u) = (window.clone(), ui.clone());
    send.on_click(move |_| w.send_message(&u));
    let (w, u) = (window.clone(), ui.clone());
    refresh.on_click(move |_| {
        w.state.lock().unwrap().follow_tail = true;
        w.note(&t("Refreshing…"));
        w.load(&u);
    });
    let (w, u) = (window.clone(), ui.clone());
    more.on_click(move |_| {
        let mut state = w.state.lock().unwrap();
        state.count += PAGE;
        state.follow_tail = false;
        drop(state);
        w.note(&t("Loading more…"));
        w.load(&u);
    });
    let (w, parent) = (window.clone(), dialog);
    manage.on_click(move |_| {
        let mut files = w.state.lock().unwrap().files.clone();
        if files.is_empty() {
            w.note(&t("No files are attached."));
            return;
        }
        super::manage_files_dialog(&parent, &mut files);
        let count = files.len();
        w.state.lock().unwrap().files = files;
        w.note(&ntf!("{n} file attached", "{n} files attached", count));
    });
    let (w, parent) = (window.clone(), dialog);
    attach.on_click(move |_| {
        // Parented to this dialog, not the main frame: a file dialog owned by
        // the frame can open behind this modal window and strand the user.
        let dialog = FileDialog::builder(&parent).build();
        if dialog.show_modal() == ID_OK {
            if let Some(path) = dialog.get_path() {
                let mut state = w.state.lock().unwrap();
                state.files.push(path);
                w.note(&ntf!("{n} file attached", "{n} files attached", state.files.len()));
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
            let lines: Vec<String> = raw
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect();
            let old: Vec<String> = (0..window.messages.get_count())
                .filter_map(|i| window.messages.get_string(i))
                .collect();
            let old_sel = window.messages.get_selection().map(|i| i as usize);
            let follow_tail = window.state.lock().unwrap().follow_tail;
            if old != lines {
                // Update in place like the main window: rows that did not
                // change are not touched, so a refresh with nothing new
                // costs nothing and never moves the reader.
                for (index, line) in lines.iter().enumerate() {
                    if index >= window.messages.get_count() as usize {
                        window.messages.append(line);
                    } else if old.get(index) != Some(line) {
                        window.messages.set_string(index as u32, line);
                    }
                }
                while window.messages.get_count() as usize > lines.len() {
                    window.messages.delete(window.messages.get_count() - 1);
                }
                if let Some(selected) = restored_selection(&old, &lines, old_sel, follow_tail) {
                    window.messages.set_selection(selected as u32, true);
                }
            }
            let name = window.state.lock().unwrap().name.clone();
            window.note(&ntf!(
                "{name}: {n} message.",
                "{name}: {n} messages.",
                message_count(&lines),
                name = name
            ));
        }
        Err(e) => {
            if needs_messenger_unlock(&e) {
                // TRANSLATORS: "Chat" and "Unlock encrypted chats" are the menu and its item
                window.note(&t(
                    "Messenger is locked. Close this window and use Chat, Unlock encrypted chats, then reopen the chat."
                ));
            } else {
                window.note(&tf!("Messages could not load: {error}", error = e));
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
            let mut state = window.state.lock().unwrap();
            state.files.clear();
            state.reply_to = None;
            state.follow_tail = true;
            drop(state);
            window.note(&t("Message sent."));
            window.load(ui);
        }
        Err(e) => {
            let kept = window.state.lock().unwrap().files.len();
            if kept > 0 {
                window.note(&tf!(
                    "Could not send: {error} Your attachments are kept. Use the Attachments button to remove a file, for example one over Messenger's 25 MB limit.",
                    error = e
                ));
            } else {
                window.note(&tf!("Could not send: {error}", error = e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[&str]) -> Record {
        Record {
            author_ref: None,
            links: Vec::new(),
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

    fn lines(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn history_load_keeps_the_message_being_read() {
        let old = lines(&["a", "b", "c"]);
        let new = lines(&["x", "y", "a", "b", "c"]);
        assert_eq!(restored_selection(&old, &new, Some(1), false), Some(3));
        // Repeated text resolves to the occurrence at the expected offset.
        let new = lines(&["b", "a", "b", "c"]);
        let old = lines(&["a", "b", "c"]);
        assert_eq!(restored_selection(&old, &new, Some(1), false), Some(2));
        // A vanished line keeps its position, clamped.
        let new = lines(&["a", "c"]);
        assert_eq!(restored_selection(&old, &new, Some(1), false), Some(1));
        assert_eq!(restored_selection(&old, &[], Some(1), false), None);
    }

    #[test]
    fn date_headers_are_not_counted_as_messages() {
        let rows = lines(&[
            "# Sep 5, 2026",
            "11:50 PM Richard: hi",
            "# Sep 6, 2026",
            "9:01 AM You: there",
        ]);
        assert_eq!(message_count(&rows), 2);
        assert_eq!(message_count(&[]), 0);
    }

    #[test]
    fn the_tail_wins_when_following_or_unselected() {
        let old = lines(&["a", "b"]);
        let new = lines(&["x", "a", "b", "c"]);
        assert_eq!(restored_selection(&old, &new, Some(0), true), Some(3));
        assert_eq!(restored_selection(&old, &new, None, false), Some(3));
        assert_eq!(restored_selection(&[], &new, None, false), Some(3));
    }
}
