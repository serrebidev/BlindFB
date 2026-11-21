//! The chat window: one conversation (a person's chat or a group chat) in
//! its own dialog, opened by activating a row in the Chats list — the same
//! shape as the group window. Messages load newest-last; the composer sends
//! with Enter or the Send button, files attach, and Load more walks back
//! through the history.
use super::{Job, REACTIONS, Tag, Task, Ui, message_match_text, needs_messenger_unlock};
use blindfb_core::chat_voice::{self, Action, Session};
use blindfb_core::fb::{
    Record, chatmembers_args, msg_react_args, parse_records, reply_args, send_args,
};
use blindfb_core::{ntf, tf};
use patois::t;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use wxdragon::prelude::*;
use wxdragon::timer::Timer;

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
    voice: Arc<Mutex<Session>>,
    play_voice: Button,
    stop_voice: Button,
    record_voice: Button,
    send_voice: Button,
    discard_voice: Button,
    state: Arc<Mutex<ChatState>>,
}

struct ChatState {
    id: String,
    name: String,
    /// A group chat has a member list to view; a one-to-one chat does not.
    group: bool,
    count: u32,
    files: Vec<String>,
    /// Text of the message the next send replies to (quotes).
    reply_to: Option<(String, usize)>,
    /// True when the next delivery should land on the newest message
    /// (open, refresh, after sending). Load more sets it false: the reader
    /// is walking history and must keep their place.
    follow_tail: bool,
    voice_ready: bool,
    voice_jobs: usize,
    voice_action_pending: bool,
    recorder: Recorder,
    message_ids: Vec<String>,
    clips: Vec<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Recorder {
    Idle,
    Recording,
    Ready,
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

/// A full-history read ends with a marker line from the backend: `# all`
/// when the start of the conversation was reached, `# more` when it had
/// to stop early. Split it off the display lines and report which it was.
fn split_history_marker(lines: Vec<String>) -> (Vec<String>, Option<bool>) {
    match lines.last().map(String::as_str) {
        Some("# all") => (lines[..lines.len() - 1].to_vec(), Some(true)),
        Some("# more") => (lines[..lines.len() - 1].to_vec(), Some(false)),
        _ => (lines, None),
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
        (id.to_owned(), if name.is_empty() { id.to_owned() } else { name.to_owned() })
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
        let count = self.state.lock().unwrap().count;
        self.voice_action(ui, Action::Read(count), String::new());
    }

    fn voice_action(&self, ui: &Ui, action: Action, clip: String) {
        {
            let mut state = self.state.lock().unwrap();
            if action != Action::Close {
                match action {
                    Action::Pulse if !state.voice_ready || state.voice_jobs != 0 => return,
                    Action::Read(_) | Action::Pulse => {}
                    _ if !state.voice_ready || state.voice_action_pending => return,
                    _ => state.voice_action_pending = true,
                }
                state.voice_jobs += 1;
            }
        }
        let _ = ui.jobs.send(Job {
            task: Task::ChatVoice { session: self.voice.clone(), action, clip },
            tag: Tag::ChatVoice(self.session, action),
        });
    }

    fn voice_controls(&self) {
        let state = self.state.lock().unwrap();
        let idle = state.recorder == Recorder::Idle;
        self.play_voice.enable(state.voice_ready && idle);
        self.stop_voice.enable(state.voice_ready);
        self.record_voice.enable(state.voice_ready && state.recorder != Recorder::Ready);
        self.send_voice.enable(state.voice_ready && state.recorder == Recorder::Ready);
        self.discard_voice.enable(state.voice_ready && !idle);
        self.record_voice.set_label(&if state.recorder == Recorder::Recording {
            t("Stop recordin&g")
        } else {
            t("Record voice messa&ge")
        });
    }

    fn play_selected(&self, ui: &Ui, report_missing: bool) {
        let clip = self.messages.get_selection().and_then(|index| {
            let state = self.state.lock().unwrap();
            let index = index as usize;
            state
                .clips
                .get(index)
                .copied()
                .unwrap_or(false)
                .then(|| state.message_ids[index].clone())
        });
        if let Some(clip) = clip {
            self.voice_action(ui, Action::Play, clip);
        } else if report_missing {
            self.note(&t("Select a voice message in the message list first."));
        }
    }

    /// The member list of a group chat (issue #20, item 4): the backend
    /// reads it from the thread's details panel.
    fn load_members(&self, ui: &Ui) {
        let (id, group) = {
            let state = self.state.lock().unwrap();
            (state.id.clone(), state.group)
        };
        if !group {
            return;
        }
        self.note(&t("Loading members…"));
        ui.send(Job {
            task: Task::Fb(chatmembers_args(&id), None),
            tag: Tag::ChatMembers(self.session),
        });
    }

    /// The whole history in one read (issue #20, item 1): the backend
    /// scrolls to the start of the conversation instead of paging.
    fn load_all(&self, ui: &Ui) {
        self.voice_action(ui, Action::Read(1_000_000), String::new());
    }

    fn send_message(&self, ui: &Ui) {
        let text = self.compose.get_value();
        let (id, files, reply_to) = {
            let state = self.state.lock().unwrap();
            (state.id.clone(), state.files.clone(), state.reply_to.clone())
        };
        if text.trim().is_empty() && files.is_empty() {
            return;
        }
        self.note(&t("Sending…"));
        let args = match &reply_to {
            Some((target, nth)) => reply_args(&id, &text, &files, target, *nth),
            None => send_args(&id, &text, &files),
        };
        ui.send(Job { task: Task::Fb(args, None), tag: Tag::ChatWindowSent(self.session) });
    }

    fn selected_match(&self) -> Option<(String, usize)> {
        let index = self.messages.get_selection()? as usize;
        let state = self.state.lock().unwrap();
        let mut texts = Vec::new();
        let mut i = 0;
        while let Some(line) = self.messages.get_string(i) {
            texts.push(if state.clips.get(i as usize) == Some(&true) {
                String::new()
            } else {
                message_match_text(&line)
            });
            i += 1;
        }
        super::match_nth(&texts, index)
    }

    fn menu_action(&self, ui: &Ui, action: i32) {
        match action {
            ID_MSG_COPY => {
                if let Some(index) = self.messages.get_selection() {
                    if let Some(line) = self.messages.get_string(index) {
                        let done = crate::copy_to_clipboard(&line);
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
    let load_all = Button::builder(&dialog)
        .with_label(&t("Load &all messages"))
        .build();
    let refresh = Button::builder(&dialog).with_label(&t("&Refresh")).build();
    let members = Button::builder(&dialog).with_label(&t("&Members…")).build();
    members.set_name(&t("Members. Show who is in this group chat"));
    members.enable(group);
    let compose_label = StaticText::builder(&dialog).with_label(&t("Me&ssage")).build();
    let compose = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    compose.set_name(&tf!("Message to {name}. Enter sends", name = name));
    let attach = Button::builder(&dialog).with_label(&t("Attach &files")).build();
    let manage = Button::builder(&dialog).with_label(&t("A&ttachments…")).build();
    manage.set_name(&t("Attachments. Review or remove attached files"));
    let send = Button::builder(&dialog).with_label(&t("&Send")).build();
    let play_voice =
        Button::builder(&dialog).with_label(&t("&Play selected voice message")).build();
    let stop_voice = Button::builder(&dialog).with_label(&t("Stop voice playbac&k")).build();
    let record_voice = Button::builder(&dialog).with_label(&t("Record voice messa&ge")).build();
    let send_voice = Button::builder(&dialog).with_label(&t("Send &voice message")).build();
    let discard_voice = Button::builder(&dialog).with_label(&t("&Discard recording")).build();
    let status_label = StaticText::builder(&dialog).with_label(&t("Chat status")).build();
    let status = TextCtrl::builder(&dialog).with_style(TextCtrlStyle::ReadOnly).build();
    status.set_name(&t("Chat status"));
    status.enable_focus_from_keyboard();
    status.change_value(&t("Loading messages…"));
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    actions.add(&more, 0, SizerFlag::All, 4);
    actions.add(&load_all, 0, SizerFlag::All, 4);
    actions.add(&refresh, 0, SizerFlag::All, 4);
    actions.add(&members, 0, SizerFlag::All, 4);
    actions.add(&attach, 0, SizerFlag::All, 4);
    actions.add(&manage, 0, SizerFlag::All, 4);
    actions.add(&send, 0, SizerFlag::All, 4);
    actions.add(&close, 0, SizerFlag::All, 4);
    let voice_actions = BoxSizer::builder(Orientation::Horizontal).build();
    voice_actions.add(&play_voice, 0, SizerFlag::All, 4);
    voice_actions.add(&stop_voice, 0, SizerFlag::All, 4);
    voice_actions.add(&record_voice, 0, SizerFlag::All, 4);
    voice_actions.add(&send_voice, 0, SizerFlag::All, 4);
    voice_actions.add(&discard_voice, 0, SizerFlag::All, 4);
    root.add(&messages_label, 0, SizerFlag::All, 4);
    root.add(&messages, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&compose_label, 0, SizerFlag::All, 4);
    root.add(&compose, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 4);
    root.add_sizer(&voice_actions, 0, SizerFlag::Expand, 4);
    root.add(&status_label, 0, SizerFlag::All, 4);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 4);
    dialog.set_sizer(root, true);

    let window = ChatWindow {
        session,
        dialog,
        messages,
        compose,
        status,
        voice: Arc::new(Mutex::new(Session::new(session, id.clone()))),
        play_voice,
        stop_voice,
        record_voice,
        send_voice,
        discard_voice,
        state: Arc::new(Mutex::new(ChatState {
            id,
            name: name.clone(),
            group,
            count: PAGE,
            files: Vec::new(),
            reply_to: None,
            follow_tail: true,
            voice_ready: false,
            voice_jobs: 0,
            voice_action_pending: false,
            recorder: Recorder::Idle,
            message_ids: Vec::new(),
            clips: Vec::new(),
        })),
    };
    *ui.chat_window.lock().unwrap() = Some(window.clone());
    window.voice_controls();
    window.load(ui);

    let timer = Timer::new(&dialog);
    let (w, u) = (window.clone(), ui.clone());
    timer.on_tick(move |_| w.voice_action(&u, Action::Pulse, String::new()));
    timer.start(1200, false);
    let (w, u) = (window.clone(), ui.clone());
    play_voice.on_click(move |_| w.play_selected(&u, true));
    let (w, u) = (window.clone(), ui.clone());
    messages.on_item_double_clicked(move |_| w.play_selected(&u, false));
    let (w, u) = (window.clone(), ui.clone());
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if w.messages.has_focus()
                && matches!(
                    key.get_key_code(),
                    Some(wxdragon::keycode::WXK_RETURN | wxdragon::keycode::WXK_NUMPAD_ENTER)
                )
            {
                event.skip(false);
                w.play_selected(&u, false);
                return;
            }
        }
        event.skip(true);
    });
    let (w, u) = (window.clone(), ui.clone());
    stop_voice.on_click(move |_| w.voice_action(&u, Action::StopPlayback, String::new()));
    let (w, u) = (window.clone(), ui.clone());
    record_voice.on_click(move |_| {
        let recording = w.state.lock().unwrap().recorder == Recorder::Recording;
        let action = if recording { Action::Stop } else { Action::Start };
        w.voice_action(&u, action, String::new());
    });
    let (w, u) = (window.clone(), ui.clone());
    send_voice.on_click(move |_| w.voice_action(&u, Action::Send, String::new()));
    let (w, u) = (window.clone(), ui.clone());
    discard_voice.on_click(move |_| w.voice_action(&u, Action::Discard, String::new()));

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
            .append_item(ID_MSG_COPY, &t("Cop&y message"), &t("Copy the message text"))
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
    members.on_click(move |_| w.load_members(&u));
    let (w, u) = (window.clone(), ui.clone());
    more.on_click(move |_| {
        let mut state = w.state.lock().unwrap();
        state.count += PAGE;
        state.follow_tail = false;
        drop(state);
        w.note(&t("Loading more…"));
        w.load(&u);
    });
    let (w, u) = (window.clone(), ui.clone());
    load_all.on_click(move |_| {
        w.state.lock().unwrap().follow_tail = false;
        w.note(&t("Loading all messages…"));
        w.load_all(&u);
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
    drop(timer);
    ui.chat_window.lock().unwrap().take();
    window.voice_action(ui, Action::Close, String::new());
    dialog.destroy();
    ui.items.set_focus();
}

fn voice_line(row: &Value) -> String {
    let field = |key: &str| row[key].as_str().unwrap_or("");
    if let Some(date) = row["date"].as_str() {
        return format!("# {date}");
    }
    let text = field("text").replace('\n', " ⏎ ");
    let attachment = field("attachment");
    let mut body = if row["voice"].as_bool() == Some(true) {
        format!("[{}] {text}", t("Voice message")).trim_end().to_owned()
    } else if attachment.is_empty() {
        text
    } else {
        format!("[{attachment}] {text}").trim_end().to_owned()
    };
    if !field("reactions").is_empty() {
        body.push_str(&format!(" {{{}}}", field("reactions")));
    }
    format!("{} {}: {body}", field("time"), field("sender"))
}

pub(super) fn deliver_voice(ui: &Ui, session: u64, action: Action, result: Result<String, String>) {
    let window = ui.chat_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    if action == Action::Close {
        return;
    }
    let reading = matches!(action, Action::Read(_));
    {
        let mut state = window.state.lock().unwrap();
        state.voice_jobs -= 1;
        if !reading && action != Action::Pulse {
            state.voice_action_pending = false;
        }
    }
    let mut payload = None;
    let return_focus = match action {
        Action::Stop => window.record_voice.has_focus(),
        Action::Send => window.send_voice.has_focus(),
        Action::Discard => window.discard_voice.has_focus(),
        _ => false,
    };
    let result = result.and_then(|raw| match serde_json::from_str::<Value>(&raw) {
        Ok(data) if data.is_object() => {
            let changed = {
                let mut state = window.state.lock().unwrap();
                let before = (state.voice_ready, state.recorder);
                if reading && data["rows"].is_array() {
                    state.voice_ready = true;
                }
                state.recorder = match data["recorder"].as_str() {
                    Some("recording") => Recorder::Recording,
                    Some("ready") => Recorder::Ready,
                    Some("idle") => Recorder::Idle,
                    _ => state.recorder,
                };
                before != (state.voice_ready, state.recorder)
            };
            if changed {
                window.voice_controls();
            }
            payload = Some(data);
            Ok(raw)
        }
        _ if reading => {
            window.state.lock().unwrap().voice_ready = false;
            window.voice_controls();
            Ok(raw)
        }
        _ => Err(t("Messenger returned an unreadable response. Refresh the chat to reconnect.")),
    });
    if !reading {
        let data = payload.as_ref();
        if let Some(rows) = data.and_then(|data| data["rows"].as_array()) {
            let mut state = window.state.lock().unwrap();
            for row in rows {
                let id = row["id"].as_str().unwrap_or_default();
                let clip = row["voice"].as_bool() == Some(true);
                if let Some(index) = state.message_ids.iter().position(|existing| existing == id) {
                    window.messages.set_string(index as u32, &voice_line(row));
                    state.clips[index] = clip;
                } else {
                    window.messages.append(&voice_line(row));
                    state.message_ids.push(id.to_owned());
                    state.clips.push(clip);
                }
            }
        }
        if let Err(error) = result {
            {
                let mut state = window.state.lock().unwrap();
                state.voice_ready = false;
                state.recorder = Recorder::Idle;
            }
            window.voice_controls();
            window.note(&error);
        } else if let Some(error) = data.and_then(|data| data["error"].as_str()) {
            if error == "thread" {
                {
                    let mut state = window.state.lock().unwrap();
                    state.voice_ready = false;
                    state.recorder = Recorder::Idle;
                }
                window.voice_controls();
            }
            window.note(&chat_voice::message(error));
        } else {
            match action {
                Action::Start => {
                    window.note(&t("Recording. Activate Stop recording when finished."))
                }
                Action::Stop => {
                    window.note(&t(
                        "Recording ready. Activate Send voice message or Discard recording.",
                    ));
                    if return_focus {
                        window.send_voice.set_focus();
                    }
                }
                Action::Discard => {
                    window.note(&t("Recording discarded."));
                    if return_focus {
                        window.record_voice.set_focus();
                    }
                }
                Action::Send => {
                    window.note(&t("Voice message sent."));
                    if return_focus {
                        window.record_voice.set_focus();
                    }
                }
                _ => {}
            }
        }
        return;
    }
    match result {
        Ok(raw) => {
            if let Some(error) = payload.as_ref().and_then(|data| data["error"].as_str()) {
                window.note(&chat_voice::message(error));
                return;
            }
            let (lines, history) = if let Some((data, rows)) =
                payload.as_ref().and_then(|data| data["rows"].as_array().map(|rows| (data, rows)))
            {
                let mut state = window.state.lock().unwrap();
                state.message_ids = rows
                    .iter()
                    .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
                    .collect();
                state.clips = rows.iter().map(|row| row["voice"].as_bool() == Some(true)).collect();
                (rows.iter().map(voice_line).collect(), data["complete"].as_bool())
            } else {
                let mut state = window.state.lock().unwrap();
                state.message_ids.clear();
                state.clips.clear();
                let raw_lines =
                    raw.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect();
                split_history_marker(raw_lines)
            };
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
            let counted = ntf!(
                "{name}: {n} message.",
                "{name}: {n} messages.",
                message_count(&lines),
                name = name
            );
            window.note(&match history {
                Some(true) => format!("{counted} {}", t("All messages are loaded.")),
                Some(false) => format!(
                    "{counted} {}",
                    t("Loading stopped before the start of the conversation.")
                ),
                None => counted,
            });
        }
        Err(e) => {
            {
                let mut state = window.state.lock().unwrap();
                state.voice_ready = false;
                state.recorder = Recorder::Idle;
            }
            window.voice_controls();
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

/// One member's line in the members list: the name, with the admin
/// marker the backend reports in the role field.
fn member_line(record: &Record) -> String {
    let name = record.field(0);
    if record.field(2) == "admin" {
        tf!("{name} (admin)", name = name)
    } else {
        name.to_owned()
    }
}

/// The members answer for the chat window (issue #20, item 4): a modal
/// list over the chat window, or the failure in the window's status.
pub(super) fn deliver_members(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.chat_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(raw) => {
            let records = parse_records(&raw);
            let name = window.state.lock().unwrap().name.clone();
            let dialog = Dialog::builder(&window.dialog(), &tf!("Members of {name}", name = name))
                .with_size(520, 560)
                .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
                .build();
            dialog.set_escape_id(ID_CANCEL);
            let root = BoxSizer::builder(Orientation::Vertical).build();
            let label = StaticText::builder(&dialog)
                .with_label(&t("&Members"))
                .build();
            let list = ListBox::builder(&dialog).build();
            list.set_name(&t("Members of this group chat"));
            for record in &records {
                list.append(&member_line(record));
            }
            if !records.is_empty() {
                list.set_selection(0, true);
            }
            let close = Button::builder(&dialog)
                .with_id(ID_CANCEL)
                .with_label(&t("C&lose"))
                .build();
            root.add(&label, 0, SizerFlag::All, 6);
            root.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 6);
            root.add(&close, 0, SizerFlag::All, 6);
            dialog.set_sizer(root, true);
            let d = dialog;
            close.on_click(move |_| d.end_modal(ID_CANCEL));
            window.note(&tf!("{count} members.", count = records.len()));
            dialog.show_modal();
        }
        Err(e) => {
            window.note(&tf!("Members could not load: {error}", error = e));
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

    #[test]
    fn a_member_line_marks_only_admins() {
        let plain = parse_records("Ann Example\tu123\t\n");
        assert_eq!(member_line(&plain[0]), "Ann Example");
        let admin = parse_records("Bob Example\tu456\tadmin\n");
        assert_eq!(member_line(&admin[0]), "Bob Example (admin)");
    }

    #[test]
    fn a_full_history_marker_is_split_off_the_display_lines() {
        let lines = vec!["# Sep 5, 2026".to_string(), "09:00 Ann: hi".to_string()];
        let (shown, history) = split_history_marker(lines.clone());
        assert_eq!(shown, lines);
        assert_eq!(history, None);

        let mut all = lines.clone();
        all.push("# all".to_string());
        let (shown, history) = split_history_marker(all);
        assert_eq!(shown, lines);
        assert_eq!(history, Some(true));

        let mut more = lines;
        more.push("# more".to_string());
        let (_, history) = split_history_marker(more);
        assert_eq!(history, Some(false));
    }

    fn record(fields: &[&str]) -> Record {
        Record {
            author_ref: None,
            reaction: None,
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
