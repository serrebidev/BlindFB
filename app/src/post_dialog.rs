//! Native post reader. The modal is entered by item activation, never the result timer.
use super::{Job, REACTIONS, Tag, Task, Ui, open_url};
use blindfb_core::fb::react_args;
use blindfb_core::posts::{Comment, parse_detail};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use wxdragon::keycode::{WXK_NUMPAD_ENTER, WXK_RETURN};
use wxdragon::prelude::*;

const ID_REPLY: i32 = 12101;
const ID_COPY: i32 = 12102;
const ID_WEB: i32 = 12103;
const ID_LIKE: i32 = 12104;
const ID_POST_COMMENT: i32 = 12105;
const ID_SHARE: i32 = 12106;
const ID_COPY_POST: i32 = 12107;
const ID_COPY_POST_LINK: i32 = 12108;
const ID_WEB_POST: i32 = 12109;
const ID_REACT: i32 = 12120; // + index into REACTIONS
const COMPOSER: &str = "&Write a comment (Ctrl+Enter to send)";
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct PostReader {
    session: u64,
    page: String,
    url: String,
    post: TextCtrl,
    comments: ListBox,
    composer_label: StaticText,
    composer: TextCtrl,
    status: TextCtrl,
    send: Button,
    share: Button,
    refresh: Button,
    cancel: Button,
    state: Rc<RefCell<ReaderState>>,
}

struct ReaderState {
    comments: Vec<Comment>,
    parent: Option<String>,
    busy: bool,
    loaded: bool,
    /// Audience Facebook's share dialog reported; the reader's timer asks next.
    share_audience: Option<String>,
}

impl PostReader {
    fn message(&self, ui: &Ui, text: &str) {
        self.status.change_value(text);
        ui.status(text);
    }

    fn controls(&self) {
        let state = self.state.borrow();
        self.send.enable(state.loaded && !state.busy);
        self.share.enable(state.loaded && !state.busy);
        self.refresh.enable(!state.busy);
        self.cancel.enable(state.parent.is_some() && !state.busy);
        self.composer.set_editable(!state.busy);
    }

    fn cancel_reply(&self) {
        self.state.borrow_mut().parent = None;
        self.composer_label.set_label(COMPOSER);
        self.composer.set_name(&COMPOSER.replace('&', ""));
        self.controls();
    }

    /// Start a job on the reader's page unless one is already running.
    fn start(&self, ui: &Ui, task: Task, tag: Tag, message: &str) {
        {
            let mut state = self.state.borrow_mut();
            if state.busy || !state.loaded {
                return;
            }
            state.busy = true;
        }
        self.controls();
        ui.send(Job { task, tag });
        self.message(ui, message);
    }

    fn share_start(&self, ui: &Ui) {
        let page = self.page.clone();
        self.start(
            ui,
            Task::PostShareOpen { page },
            Tag::PostShareReady(self.session),
            "Opening Facebook's share dialog…",
        );
    }

    /// Ask for the share text once Facebook reported the audience; runs from the reader's timer.
    fn share_prompt(&self, ui: &Ui, dialog: &Dialog, audience: &str) {
        let entry = TextEntryDialog::builder(
            dialog,
            &format!("Share this post to your feed. Audience: {audience}.\nSay something about it (optional), then press OK to share:"),
            "Share post",
        )
        .build();
        let text = (entry.show_modal() == ID_OK).then(|| entry.get_value().unwrap_or_default());
        entry.destroy();
        let page = self.page.clone();
        match text {
            Some(text) => {
                ui.send(Job {
                    task: Task::PostShare { page, text },
                    tag: Tag::PostShared(self.session),
                });
                self.message(ui, "Sharing…");
            }
            None => {
                ui.send(Job {
                    task: Task::PostShareCancel { page },
                    tag: Tag::PostShareClosed(self.session),
                });
                self.message(ui, "Share cancelled. Nothing was shared.");
            }
        }
    }

    fn post_action(&self, ui: &Ui, action: i32) {
        match action {
            ID_POST_COMMENT => {
                self.cancel_reply();
                self.composer.set_focus();
            }
            ID_SHARE => self.share_start(ui),
            ID_COPY_POST => {
                let copied = Clipboard::get().set_text(&self.post.get_value());
                self.message(ui, if copied { "Post copied." } else { "Could not copy the post." });
            }
            ID_COPY_POST_LINK => {
                let copied = Clipboard::get().set_text(&self.url);
                self.message(ui, if copied { "Link copied." } else { "Could not copy the link." });
            }
            ID_WEB_POST => open_url(&self.url),
            _ if (ID_REACT..ID_REACT + REACTIONS.len() as i32).contains(&action) => {
                let reaction = REACTIONS[(action - ID_REACT) as usize];
                ui.send(Job {
                    task: Task::Fb(react_args(&self.url, reaction), None),
                    tag: Tag::PostNote(self.session),
                });
                self.message(ui, "Reacting…");
            }
            _ => {}
        }
    }

    fn send(&self, ui: &Ui) {
        let text = self.composer.get_value();
        if text.trim().is_empty() {
            self.message(ui, "Write a comment or reply first.");
            self.composer.set_focus();
            return;
        }
        let parent = {
            let mut state = self.state.borrow_mut();
            if state.busy || !state.loaded {
                return;
            }
            state.busy = true;
            state.parent.clone()
        };
        self.controls();
        ui.send(Job {
            task: Task::PostSend { page: self.page.clone(), parent, text },
            tag: Tag::PostSent(self.session),
        });
        self.message(ui, "Sending comment…");
    }

    fn menu_action(&self, ui: &Ui, action: i32) {
        let selected = self.comments.get_selection().and_then(|index| {
            self.state.borrow().comments.get(index as usize).map(|comment| {
                (
                    comment.id.clone(),
                    comment.author.clone(),
                    comment.text.clone(),
                    comment.url.clone(),
                )
            })
        });
        let Some((id, author, text, url)) = selected else {
            return;
        };
        match action {
            ID_REPLY => {
                let mut state = self.state.borrow_mut();
                if state.busy || id.is_empty() {
                    return;
                }
                state.parent = Some(id);
                drop(state);
                let label = format!("Reply to {author} (Ctrl+Enter to send)");
                self.composer_label.set_label(&label);
                self.composer.set_name(&label);
                self.controls();
                self.composer.set_focus();
                self.message(ui, &format!("Replying to {author}."));
            }
            ID_COPY => {
                let message = if Clipboard::get().set_text(&format!("{author}: {text}")) {
                    "Comment copied."
                } else {
                    "Could not copy the comment."
                };
                self.message(ui, message);
            }
            ID_WEB => {
                if let Some(url) = url {
                    open_url(&url);
                }
            }
            ID_LIKE => {
                let page = self.page.clone();
                self.start(
                    ui,
                    Task::PostLike { page, id },
                    Tag::PostLiked(self.session),
                    &format!("Changing your Like on {author}'s comment…"),
                );
            }
            _ => {}
        }
    }
}

pub(super) fn open(ui: &Ui, target: &str) {
    if ui.post_reader.borrow().is_some() {
        return;
    }
    let selection = ui.items.get_selection();
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let dialog = Dialog::builder(&ui.frame, "Post and comments")
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let post_label = StaticText::builder(&dialog).with_label("&Full post").build();
    let post = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    post.set_name("Full post. Applications or Shift+F10 for comment, react, share and copy");
    let comments_label = StaticText::builder(&dialog).with_label("&Comments").build();
    let comments = ListBox::builder(&dialog).build();
    comments
        .set_name("Comments. Applications or Shift+F10 for reply, like, copy and Facebook actions");
    let composer_label = StaticText::builder(&dialog).with_label(COMPOSER).build();
    let composer = TextCtrl::builder(&dialog).with_style(TextCtrlStyle::MultiLine).build();
    composer.set_name(&COMPOSER.replace('&', ""));
    let status_label = StaticText::builder(&dialog).with_label("Reader status").build();
    let status = TextCtrl::builder(&dialog).with_style(TextCtrlStyle::ReadOnly).build();
    status.set_name("Reader status");
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let send = Button::builder(&dialog).with_label("&Send (Ctrl+Enter)").build();
    let share = Button::builder(&dialog).with_label("S&hare…").build();
    let refresh = Button::builder(&dialog).with_label("&Refresh").build();
    let cancel = Button::builder(&dialog).with_label("Ca&ncel reply").build();
    let close = Button::builder(&dialog).with_id(ID_CANCEL).with_label("C&lose").build();
    for button in [&send, &share, &refresh, &cancel, &close] {
        button.set_name(&button.get_label());
        buttons.add(button, 0, SizerFlag::All, 4);
    }
    root.add(&post_label, 0, SizerFlag::All, 4);
    root.add(&post, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&comments_label, 0, SizerFlag::All, 4);
    root.add(&comments, 3, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&composer_label, 0, SizerFlag::All, 4);
    root.add(&composer, 1, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&status_label, 0, SizerFlag::All, 4);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 0);
    dialog.set_sizer(root, true);

    let reader = PostReader {
        session,
        page: format!("blindfb-post-{}-{session}", std::process::id()),
        url: target.to_owned(),
        post,
        comments,
        composer_label,
        composer,
        status,
        send,
        share,
        refresh,
        cancel,
        state: Rc::new(RefCell::new(ReaderState {
            comments: Vec::new(),
            parent: None,
            busy: true,
            loaded: false,
            share_audience: None,
        })),
    };
    reader.controls();
    *ui.post_reader.borrow_mut() = Some(reader.clone());
    let (r, u) = (reader.clone(), ui.clone());
    send.on_click(move |_| r.send(&u));
    let (r, u) = (reader.clone(), ui.clone());
    composer.bind_internal(EventType::KEY_DOWN, move |event| {
        if event.control_down()
            && matches!(event.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER))
        {
            event.skip(false);
            r.send(&u);
        } else {
            event.skip(true);
        }
    });
    let r = reader.clone();
    cancel.on_click(move |_| {
        r.cancel_reply();
        r.composer.set_focus();
    });
    let (r, u) = (reader.clone(), ui.clone());
    refresh.on_click(move |_| {
        {
            let mut state = r.state.borrow_mut();
            if state.busy {
                return;
            }
            state.busy = true;
        }
        r.controls();
        u.send(Job {
            task: Task::PostRefresh { page: r.page.clone() },
            tag: Tag::PostLoaded(r.session),
        });
        r.message(&u, "Refreshing comments…");
    });
    close.on_click(move |_| dialog.end_modal(ID_CANCEL));
    dialog.bind_internal(EventType::CLOSE_WINDOW, move |event| {
        event.skip(false);
        dialog.end_modal(ID_CANCEL);
    });
    let r = reader.clone();
    comments.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        let available = r.comments.get_selection().and_then(|i| {
            let state = r.state.borrow();
            state
                .comments
                .get(i as usize)
                .map(|c| (!state.busy && !c.id.is_empty(), c.url.is_some()))
        });
        let Some((reply, web)) = available else {
            return;
        };
        let mut menu = Menu::builder()
            .append_item(ID_REPLY, "&Reply", "Reply to this exact comment")
            .append_item(ID_LIKE, "&Like / unlike", "Like this comment, or remove your Like")
            .append_item(ID_COPY, "&Copy comment", "Copy the full comment")
            .append_item(ID_WEB, "Open comment on &Facebook", "Open the comment permalink")
            .append_separator()
            .append_item(ID_POST_COMMENT, "Comment on the &post", "Write a new top-level comment")
            .build();
        menu.enable_item(ID_REPLY, reply);
        menu.enable_item(ID_LIKE, reply);
        menu.enable_item(ID_WEB, web);
        r.comments.popup_menu(&mut menu, None);
    });
    let r = reader.clone();
    post.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        let ready = {
            let state = r.state.borrow();
            state.loaded && !state.busy
        };
        let mut menu = Menu::builder()
            .append_item(ID_POST_COMMENT, "&Comment…", "Write a comment on this post")
            .build();
        let react = Menu::builder().build();
        for (i, reaction) in REACTIONS.iter().enumerate() {
            let label = if *reaction == "none" {
                "Remove reaction".to_owned()
            } else {
                reaction[..1].to_uppercase() + &reaction[1..]
            };
            react.append(ID_REACT + i as i32, &label, "", ItemKind::Normal);
        }
        menu.append_submenu(react, "&React", "");
        menu.append(
            ID_SHARE,
            "S&hare…",
            "Share with Facebook's own share dialog",
            ItemKind::Normal,
        );
        menu.append_separator();
        menu.append(ID_COPY_POST, "Cop&y post text", "", ItemKind::Normal);
        menu.append(ID_COPY_POST_LINK, "Copy &link", "", ItemKind::Normal);
        menu.append(ID_WEB_POST, "Open on &Facebook (web)", "", ItemKind::Normal);
        menu.enable_item(ID_SHARE, ready);
        r.post.popup_menu(&mut menu, None);
    });
    let (r, u) = (reader.clone(), ui.clone());
    share.on_click(move |_| r.share_start(&u));
    let (r, u) = (reader.clone(), ui.clone());
    dialog.bind_internal(EventType::MENU, move |event| match event.get_id() {
        ID_REPLY | ID_COPY | ID_WEB | ID_LIKE => {
            event.skip(false);
            r.menu_action(&u, event.get_id());
        }
        id if (ID_POST_COMMENT..=ID_WEB_POST).contains(&id)
            || (ID_REACT..ID_REACT + REACTIONS.len() as i32).contains(&id) =>
        {
            event.skip(false);
            r.post_action(&u, id);
        }
        _ => event.skip(true),
    });
    // The share prompt opens from here, never from the frame's results timer.
    let share_timer = Timer::new(&dialog);
    let (r, u) = (reader.clone(), ui.clone());
    share_timer.on_tick(move |_| {
        let audience = r.state.borrow_mut().share_audience.take();
        if let Some(audience) = audience {
            r.share_prompt(&u, &dialog, &audience);
        }
    });
    share_timer.start(150, false);
    ui.send(Job {
        task: Task::PostLoad { page: reader.page.clone(), target: target.to_owned() },
        tag: Tag::PostLoaded(session),
    });
    reader.message(ui, "Loading post and comments…");
    comments.set_focus();
    dialog.show_modal();
    share_timer.stop();
    // Remove the session before destroying controls, so queued completions cannot touch them.
    ui.post_reader.borrow_mut().take();
    let _ =
        ui.jobs.send(Job { task: Task::PostClose { page: reader.page.clone() }, tag: Tag::Silent });
    dialog.destroy();
    if let Some(index) = selection.filter(|i| *i < ui.items.get_count()) {
        ui.items.set_selection(index, true);
    }
    ui.items.set_focus();
}

fn selected_index(comments: &[Comment], id: Option<&str>) -> Option<u32> {
    id.and_then(|id| comments.iter().position(|comment| comment.id == id)).map(|i| i as u32)
}

/// The open reader, if it is still the one a completion belongs to.
fn active(ui: &Ui, session: u64) -> Option<PostReader> {
    ui.post_reader.borrow().as_ref().filter(|r| r.session == session).cloned()
}

/// Facebook's share dialog is open: hand the audience to the reader's timer to ask.
pub(super) fn share_ready(ui: &Ui, session: u64, result: Result<String, String>) {
    let Some(reader) = active(ui, session) else {
        return;
    };
    match result {
        Ok(audience) => reader.state.borrow_mut().share_audience = Some(audience),
        Err(error) => note(ui, session, Err(error), true),
    }
}

/// Show an action's outcome; `release` ends the reader's busy state.
pub(super) fn note(ui: &Ui, session: u64, result: Result<String, String>, release: bool) {
    let Some(reader) = active(ui, session) else {
        return;
    };
    if release {
        reader.state.borrow_mut().busy = false;
        reader.controls();
    }
    match result {
        Ok(text) => reader.message(ui, &text),
        Err(error) => reader.message(ui, &error),
    }
}

pub(super) fn apply(ui: &Ui, session: u64, result: Result<String, String>, done: &str, sent: bool) {
    let Some(reader) = active(ui, session) else {
        return;
    };
    reader.state.borrow_mut().busy = false;
    match result.and_then(|json| parse_detail(&json)) {
        Err(error) => {
            reader.message(ui, &format!("{error} Your draft has been kept."));
        }
        Ok(detail) => {
            let selected_id = reader.comments.get_selection().and_then(|index| {
                reader.state.borrow().comments.get(index as usize).map(|c| c.id.clone())
            });
            let text = format!("{}\n\n{}\n\n{}", detail.author, detail.text, detail.time);
            if reader.post.get_value() != text {
                reader.post.change_value(&text);
            }
            for (index, comment) in detail.comments.iter().enumerate() {
                let row = format!("{}: {} — {}", comment.author, comment.text, comment.time);
                if index < reader.comments.get_count() as usize {
                    if reader.comments.get_string(index as u32).as_deref() != Some(&row) {
                        reader.comments.set_string(index as u32, &row);
                    }
                } else {
                    reader.comments.append(&row);
                }
            }
            while reader.comments.get_count() as usize > detail.comments.len() {
                reader.comments.delete(reader.comments.get_count() - 1);
            }
            if let Some(index) = selected_index(&detail.comments, selected_id.as_deref()) {
                if reader.comments.get_selection() != Some(index) {
                    reader.comments.set_selection(index, true);
                }
            } else if let Some(index) = reader.comments.get_selection() {
                reader.comments.set_selection(index, false);
            }
            let count = detail.comments.len();
            {
                let mut state = reader.state.borrow_mut();
                state.comments = detail.comments;
                state.loaded = true;
            }
            if sent {
                reader.composer.change_value("");
                reader.cancel_reply();
            }
            reader.message(ui, &format!("{done} {count} comments loaded."));
        }
    }
    reader.controls();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_restores_identity_instead_of_row_number() {
        let comments = parse_detail(r#"{"url":"https://www.facebook.com/a/posts/1","author":"A","time":"now","text":"Post","comments":[{"id":"second","author":"B","time":"now","text":"Second","parent_id":null,"url":null},{"id":"first","author":"A","time":"now","text":"First","parent_id":null,"url":null}]}"#).unwrap().comments;
        assert_eq!(selected_index(&comments, Some("first")), Some(1));
        assert_eq!(selected_index(&comments, Some("removed")), None);
        assert_eq!(selected_index(&comments, None), None);
    }
}
