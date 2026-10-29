//! Native post reader. The modal is entered by item activation, never the result timer.
use super::{Job, REACTIONS, Tag, Task, Ui, append_links_menu, open_url};
use blindfb_core::fb::react_args;
use blindfb_core::posts::{Comment, Link, parse_detail};
use super::player::{Player, PlayerEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
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
    links: ListBox,
    open_link: Button,
    media: Player,
    play: Button,
    stop: Button,
    back: Button,
    forward: Button,
    volume: Slider,
    composer_label: StaticText,
    composer: TextCtrl,
    status: TextCtrl,
    send: Button,
    share: Button,
    refresh: Button,
    cancel: Button,
    state: Rc<RefCell<ReaderState>>,
}

struct ReelPlayback {
    stream: Option<String>,
    preparing: bool,
}

struct ReaderState {
    target: String,
    comments: Vec<Comment>,
    links: Vec<Link>,
    video: Option<String>,
    video_requested: bool,
    video_request: u64,
    media_ready: bool,
    media_loading: Option<Instant>,
    parent: Option<String>,
    busy: bool,
    loaded: bool,
    /// Audience Facebook's share dialog reported; the reader's timer asks next.
    share_audience: Option<String>,
    share_requested: bool,
    action_requested: Option<i32>,
    playback: Option<ReelPlayback>,
}

impl ReaderState {
    fn take_cached_stream(&mut self) -> Option<String> {
        let stream = self.playback.as_mut()?.stream.take()?;
        self.playback = None;
        Some(stream)
    }

    fn take_autoplay_request(&mut self) -> bool {
        if self.loaded && !self.busy && self.playback.as_ref().is_some_and(|request| !request.preparing && request.stream.is_none()) {
            self.playback = None;
            true
        } else { false }
    }
    fn take_action_request(&mut self) -> Option<i32> {
        if self.loaded && !self.busy { self.action_requested.take() } else { None }
    }
    fn take_video_request(&mut self, request: u64) -> bool {
        request == self.video_request && std::mem::take(&mut self.video_requested)
    }

    fn take_share_request(&mut self) -> bool {
        self.loaded && !self.busy && std::mem::take(&mut self.share_requested)
    }
}

impl PostReader {
    fn message(&self, ui: &Ui, text: &str) {
        self.status.change_value(text);
        ui.status(text);
    }

    fn controls(&self) {
        let state = self.state.borrow();
        self.play.enable(
            state.video.is_some() && state.playback.is_none() && !state.video_requested && state.media_loading.is_none(),
        );
        self.stop
            .enable(state.playback.is_some() || state.video_requested || state.media_loading.is_some() || state.media_ready);
        self.back.enable(state.media_ready);
        self.forward.enable(state.media_ready);
        self.volume.enable(state.video.is_some());
        self.open_link.enable(self.links.get_selection().is_some());
        self.send.enable(state.loaded && !state.busy);
        self.share.enable(state.loaded && !state.busy);
        self.refresh.enable(!state.busy);
        self.cancel.enable(state.parent.is_some() && !state.busy);
        self.composer.set_editable(!state.busy);
    }

    fn open_link(&self) {
        if let Some(link) = self.links.get_selection().and_then(|index| {
            self.state.borrow().links.get(index as usize).map(|link| link.url.clone())
        }) {
            open_url(&link);
        }
    }

    fn play_label(&self, playing: bool) {
        let label = if playing { "&Pause video" } else { "&Play video" };
        if self.play.get_label() == label { return; }
        self.play.set_label(label);
        self.play.set_name(&label.replace('&', ""));
    }

    fn play(&self, ui: &Ui) {
        let mut state = self.state.borrow_mut();
        if state.video_requested || state.media_loading.is_some() {
            return;
        }
        if state.media_ready {
            drop(state);
            let (ok, message) = if self.media.get_state() == MediaState::Playing {
                (self.media.pause(), "Video paused.")
            } else {
                (self.media.play(), "Playing video.")
            };
            if !ok {
                self.state.borrow_mut().media_ready = false;
            }
            self.message(ui, if ok { message } else { "Playback failed. Press Play to retry." });
            self.play_label(ok && self.media.get_state() == MediaState::Playing);
            self.controls();
            return;
        }
        let Some(target) = state.video.clone() else { return };
        state.video_requested = true;
        state.video_request = state.video_request.wrapping_add(1);
        let request = state.video_request;
        drop(state);
        self.controls();
        ui.send(Job {
            task: Task::PostVideo { page: self.page.clone(), target },
            tag: Tag::PostVideo(self.session, request),
        });
        self.message(ui, "Preparing video…");
    }

    fn load_stream(&self, ui: &Ui, data: &str) {
        let info = match blindfb_core::media::parse_stream(data) {
            Ok(info) => info,
            Err(error) => { self.message(ui, &error); return; }
        };
        self.state.borrow_mut().media_loading = Some(Instant::now());
        self.message(ui, "Loading video…");
        if !self.media.load_uri(&info.url) {
            self.state.borrow_mut().media_loading = None;
            self.message(ui, "Could not load video. Press Play to retry.");
        }
        self.controls();
    }

    fn stop(&self, ui: &Ui) {
        let was_ready = self.state.borrow().media_ready;
        let stopped = self.media.stop();
        let mut state = self.state.borrow_mut();
        state.video_request = state.video_request.wrapping_add(1);
        state.video_requested = false;
        state.playback = None;
        state.media_loading = None;
        drop(state);
        self.play_label(!stopped && self.media.get_state() == MediaState::Playing);
        self.controls();
        self.message(
            ui,
            if was_ready && !stopped {
                "Could not stop video. Try Pause or close the reader."
            } else {
                "Video stopped."
            },
        );
    }

    fn seek(&self, ui: &Ui, offset: i64) {
        let position = seek_position(self.media.tell(), self.media.length(), offset);
        if self.media.seek(position) < 0 {
            self.message(ui, "This video could not seek.");
        } else {
            self.message(ui, &format!("Video position: {} seconds.", position / 1000));
        }
    }

    fn poll_player(&self, ui: &Ui) {
        for event in self.media.poll() {
            match event {
                PlayerEvent::Loaded => {
                    if self.state.borrow_mut().media_loading.take().is_none() { continue; }
                    self.media.set_volume(self.volume.value() as f64 / 100.0);
                    let ok = self.media.play();
                    self.state.borrow_mut().media_ready = ok;
                    self.play_label(ok);
                    self.controls();
                    self.message(ui, if ok { "Playing video." } else { "Playback failed. Press Play to retry." });
                }
                PlayerEvent::StateChanged => self.play_label(self.media.get_state() == MediaState::Playing),
                PlayerEvent::Finished => {
                    self.play_label(false);
                    self.message(ui, "Video finished.");
                }
                PlayerEvent::Error(error) => {
                    let mut state = self.state.borrow_mut();
                    state.media_loading = None;
                    state.media_ready = false;
                    drop(state);
                    self.play_label(false);
                    self.controls();
                    self.message(ui, &format!("{error} Press Play to retry."));
                }
            }
        }
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
                let copied = Clipboard::get().set_text(&self.state.borrow().target);
                self.message(ui, if copied { "Link copied." } else { "Could not copy the link." });
            }
            ID_WEB_POST => open_url(&self.state.borrow().target),
            _ if (ID_REACT..ID_REACT + REACTIONS.len() as i32).contains(&action) => {
                let reaction = REACTIONS[(action - ID_REACT) as usize];
                let target = self.state.borrow().target.clone();
                if is_video_page(&target) {
                    self.start(
                        ui,
                        Task::PostReelReact {
                            page: self.page.clone(),
                            reaction: reaction.to_owned(),
                        },
                        Tag::PostReelReacted(self.session),
                        "Reacting…",
                    );
                    return;
                }
                ui.send(Job {
                    task: Task::Fb(react_args(&target, reaction), None),
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

pub(super) fn open(ui: &Ui, target: &str, share_requested: bool) {
    open_requested(ui, target, share_requested, None, None);
}

pub(super) fn open_reel(ui: &Ui, target: &str, stream: Option<String>, preparing: bool) {
    open_requested(ui, target, false, None, Some(ReelPlayback {stream, preparing}));
}

pub(super) fn react(ui: &Ui, target: &str, reaction: &str) {
    if let Some(index) = REACTIONS.iter().position(|kind| *kind == reaction) {
        open_requested(ui, target, false, Some(ID_REACT + index as i32), None);
    }
}

pub(super) fn comment(ui: &Ui, target: &str) {
    open_requested(ui, target, false, Some(ID_POST_COMMENT), None);
}

fn open_requested(ui: &Ui, target: &str, share_requested: bool, action_requested: Option<i32>, playback: Option<ReelPlayback>) {
    if ui.post_reader.borrow().is_some() {
        return;
    }
    let selection = ui.items.get_selection();
    let preview = super::selected(ui).filter(|(_, record)| {
        super::post_target(&ui.state.borrow().view, record)
            .and_then(blindfb_core::posts::reader_url).as_deref() == Some(target)
    }).map(|_| ui.details.get_value());
    let initial_links = blindfb_core::posts::ordered_links(
        if preview.is_some() { ui.link_targets.borrow().clone() } else { Vec::new() }, target);
    let autoplay = playback.is_some();
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let dialog = Dialog::builder(&ui.frame, "Post and comments")
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let links_label = StaticText::builder(&dialog).with_label("Post lin&ks").build();
    let links = ListBox::builder(&dialog).build();
    links.set_name("Post links. Enter opens the selected link in your browser");
    for link in &initial_links { links.append(&format!("{} — {}", link.label, link.url)); }
    links.set_selection(0, true);
    let open_link = Button::builder(&dialog).with_label("&Open link").build();
    let post_label = StaticText::builder(&dialog).with_label("&Full post").build();
    let post = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    post.set_name("Full post. Applications or Shift+F10 for links, comment, react, share and copy");
    post.change_value(preview.as_deref().unwrap_or("Loading post and comments…"));
    let media_label = StaticText::builder(&dialog).with_label("Video player").build();
    let media = Player::new(&dialog);
    let player_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let play = Button::builder(&dialog).with_label("&Play video").build();
    let stop = Button::builder(&dialog).with_label("S&top video").build();
    let back = Button::builder(&dialog).with_label("Back 10 seconds").build();
    let forward = Button::builder(&dialog).with_label("Forward 10 seconds").build();
    for button in [&play, &stop, &back, &forward] {
        button.set_name(&button.get_label());
        player_buttons.add(button, 0, SizerFlag::All, 4);
    }
    let volume_label = StaticText::builder(&dialog).with_label("Video &volume (percent)").build();
    let volume =
        Slider::builder(&dialog).with_value(100).with_min_value(0).with_max_value(100).build();
    volume.set_name("Video volume, percent");
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
    status.enable_focus_from_keyboard();
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
    root.add(&links_label, 0, SizerFlag::All, 4);
    root.add(&links, 1, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&open_link, 0, SizerFlag::All, 4);
    root.add(&post_label, 0, SizerFlag::All, 4);
    root.add(&post, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&media_label, 0, SizerFlag::All, 4);
    root.add(&media.surface(), 0, SizerFlag::All, 4);
    root.add_sizer(&player_buttons, 0, SizerFlag::Expand, 0);
    root.add(&volume_label, 0, SizerFlag::All, 4);
    root.add(&volume, 0, SizerFlag::Expand | SizerFlag::All, 4);
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
        links,
        open_link,
        media,
        play,
        stop,
        back,
        forward,
        volume,
        composer_label,
        composer,
        status,
        send,
        share,
        refresh,
        cancel,
        state: Rc::new(RefCell::new(ReaderState {
            target: target.to_owned(),
            comments: Vec::new(),
            links: initial_links,
            video: autoplay.then(|| target.to_owned()),
            video_requested: false,
            video_request: 0,
            media_ready: false,
            media_loading: None,
            parent: None,
            busy: true,
            loaded: false,
            share_audience: None,
            share_requested,
            action_requested,
            playback,
        })),
    };
    reader.controls();
    *ui.post_reader.borrow_mut() = Some(reader.clone());
    let r = reader.clone();
    open_link.on_click(move |_| r.open_link());
    let r = reader.clone();
    links.on_selection_changed(move |_| r.controls());
    let r = reader.clone();
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if r.links.has_focus()
                && matches!(key.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER))
            {
                event.skip(false);
                r.open_link();
                return;
            }
        }
        event.skip(true);
    });
    let (r, u) = (reader.clone(), ui.clone());
    play.on_click(move |_| r.play(&u));
    let (r, u) = (reader.clone(), ui.clone());
    stop.on_click(move |_| r.stop(&u));
    let (r, u) = (reader.clone(), ui.clone());
    back.on_click(move |_| r.seek(&u, -10_000));
    let (r, u) = (reader.clone(), ui.clone());
    forward.on_click(move |_| r.seek(&u, 10_000));
    let (r, u) = (reader.clone(), ui.clone());
    volume.on_slider(move |_| {
        if r.state.borrow().media_ready && !r.media.set_volume(r.volume.value() as f64 / 100.0) {
            r.message(&u, "Could not set video volume.");
        }
    });
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
        let loaded = {
            let mut state = r.state.borrow_mut();
            if state.busy {
                return;
            }
            state.busy = true;
            state.loaded
        };
        r.controls();
        u.send(Job {
            task: if loaded {
                Task::PostRefresh { page: r.page.clone() }
            } else {
                Task::PostLoad { page: r.page.clone(), target: r.url.clone() }
            },
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
            .append_item(ID_SHARE, "S&hare…", "Share with Facebook's own share dialog")
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
        append_links_menu(&menu, &r.state.borrow().links);
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
        r.poll_player(&u);
        let cached = r.state.borrow_mut().take_cached_stream();
        if let Some(stream) = cached {
            r.load_stream(&u, &stream);
        } else {
            let resolve = r.state.borrow_mut().take_autoplay_request();
            if resolve { r.play(&u); }
        }
        let timed_out = r
            .state
            .borrow()
            .media_loading
            .is_some_and(|start| start.elapsed() > Duration::from_secs(30));
        if timed_out {
            r.media.stop();
            r.state.borrow_mut().media_loading = None;
            r.controls();
            r.message(&u, "Video loading timed out. Press Play to retry.");
        }
        let requested = r.state.borrow_mut().take_share_request();
        if requested {
            r.share_start(&u);
        }
        let action = r.state.borrow_mut().take_action_request();
        if let Some(action) = action {
            r.post_action(&u, action);
        }
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
    links.set_focus();
    dialog.show_modal();
    share_timer.stop();
    reader.media.shutdown();
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

pub(super) fn is_video_page(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else { return false };
    let Some((_, path)) = rest.split_once('/') else { return false };
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    path.starts_with("reel/")
        || path.contains("/videos/")
        || path.starts_with("videos/")
        || (matches!(path, "watch" | "watch/" | "watch/live" | "watch/live/")
            && query.split('&').any(|pair| pair.starts_with("v=")))
}

fn seek_position(current: i64, length: i64, offset: i64) -> i64 {
    let target = current.saturating_add(offset).max(0);
    if length > 0 { target.min(length) } else { target }
}

pub(super) fn stream(ui: &Ui, session: u64, request: u64, result: Result<String, String>) {
    let Some(reader) = active(ui, session) else { return };
    if !reader.state.borrow_mut().take_video_request(request) {
        reader.controls();
        return;
    }
    match result {
        Ok(url) => reader.load_stream(ui, &url),
        Err(error) => reader.message(ui, &format!("{error} Press Play to retry.")),
    }
    reader.controls();
}

pub(super) fn prepared(ui: &Ui, target: &str, result: Result<String, String>) {
    let reader = ui.post_reader.borrow().as_ref().filter(|reader| reader.url == target).cloned();
    let Some(reader) = reader else { return };
    let mut state = reader.state.borrow_mut();
    if let Some(request) = state.playback.as_mut() {
        request.preparing = false;
        request.stream = result.ok();
    }
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
            if reader.state.borrow().links != detail.links {
                let selected_url = reader.links.get_selection().and_then(|index| {
                    let state = reader.state.borrow();
                    state.loaded.then(|| state.links.get(index as usize).map(|link| link.url.clone())).flatten()
                });
                for (index, link) in detail.links.iter().enumerate() {
                    let row = format!("{} — {}", link.label, link.url);
                    if index < reader.links.get_count() as usize {
                        if reader.links.get_string(index as u32).as_deref() != Some(&row) {
                            reader.links.set_string(index as u32, &row);
                        }
                    } else {
                        reader.links.append(&row);
                    }
                }
                while reader.links.get_count() as usize > detail.links.len() {
                    reader.links.delete(reader.links.get_count() - 1);
                }
                if !detail.links.is_empty() {
                    let index = selected_url.and_then(|url| detail.links.iter().position(|link| link.url == url)).unwrap_or(0);
                    reader.links.set_selection(index as u32, true);
                }
            }
            let count = detail.comments.len();
            {
                let mut state = reader.state.borrow_mut();
                state.comments = detail.comments;
                state.target = detail.url;
                state.links = detail.links;
                state.video = detail.video;
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
    fn video_routes_and_seek_bounds() {
        assert!(is_video_page("https://www.facebook.com/reel/123"));
        assert!(is_video_page("https://www.facebook.com/author/videos/123"));
        assert!(is_video_page("https://www.facebook.com/watch/?v=123"));
        assert!(is_video_page("https://www.facebook.com/watch/live/?v=123"));
        assert!(!is_video_page("https://www.facebook.com/a/posts/123?next=/reel/123"));
        assert!(!is_video_page("https://www.facebook.com/watch/?next=v=123"));
        assert_eq!(seek_position(5_000, 60_000, -10_000), 0);
        assert_eq!(seek_position(55_000, 60_000, 10_000), 60_000);
        assert_eq!(seek_position(55_000, 0, 10_000), 65_000);
    }

    #[test]
    fn requested_share_waits_for_load_and_starts_once() {
        let mut state = ReaderState {
            target: "https://www.facebook.com/a/posts/123".to_owned(),
            comments: Vec::new(),
            links: Vec::new(),
            video: None,
            video_requested: false,
            video_request: 0,
            media_ready: false,
            media_loading: None,
            parent: None,
            busy: true,
            loaded: false,
            share_audience: None,
            share_requested: true,
            action_requested: Some(ID_POST_COMMENT),
            playback: None,
        };
        state.video_requested = true;
        state.video_request = 2;
        assert!(!state.take_video_request(1));
        assert!(state.video_requested);
        assert!(state.take_video_request(2));
        assert!(!state.take_video_request(2));
        assert!(!state.take_share_request());
        assert!(state.take_action_request().is_none());
        state.busy = false;
        assert!(!state.take_share_request());
        state.loaded = true;
        state.busy = true;
        assert!(!state.take_share_request());
        state.busy = false;
        assert!(state.take_share_request());
        assert!(!state.take_share_request());
        assert_eq!(state.take_action_request(), Some(ID_POST_COMMENT));
        assert_eq!(state.take_action_request(), None);
        state.loaded = false;
        state.playback = Some(ReelPlayback {stream:Some("https://video.example/cached".into()),preparing:false});
        assert_eq!(state.take_cached_stream().as_deref(), Some("https://video.example/cached"));
        assert!(state.take_cached_stream().is_none());
        state.playback = Some(ReelPlayback {stream:None,preparing:true});
        state.loaded = true;
        assert!(!state.take_autoplay_request());
        state.playback.as_mut().unwrap().preparing = false;
        state.busy = true;
        assert!(!state.take_autoplay_request());
        state.busy = false;
        assert!(state.take_autoplay_request());
        assert!(!state.take_autoplay_request());
    }

    #[test]
    fn refresh_restores_identity_instead_of_row_number() {
        let comments = parse_detail(r#"{"url":"https://www.facebook.com/a/posts/1","author":"A","time":"now","text":"Post","comments":[{"id":"second","author":"B","time":"now","text":"Second","parent_id":null,"url":null},{"id":"first","author":"A","time":"now","text":"First","parent_id":null,"url":null}]}"#).unwrap().comments;
        assert_eq!(selected_index(&comments, Some("first")), Some(1));
        assert_eq!(selected_index(&comments, Some("removed")), None);
        assert_eq!(selected_index(&comments, None), None);
    }
}
