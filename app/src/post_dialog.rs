//! Native post reader. The modal is entered by item activation, never the result timer.
use super::player::{Player, PlayerEvent};
use super::shortcuts_dialog::{post_key, COMMENT_COPY, COMMENT_COPY_ALL, COMMENT_OPEN_LINK};
use super::{append_links_menu, open_url, Job, Tag, Task, Ui, REACTIONS};
use blindfb_core::fb::{friend_add_args, react_args};
use blindfb_core::{ntf, tf};
use blindfb_core::posts::{parse_detail, Comment, Link};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use wxdragon::keycode::{WXK_NUMPAD_ENTER, WXK_RETURN};
use patois::t;
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
const ID_COMMENT_PROFILE: i32 = 12110;
const ID_COMMENT_FRIEND: i32 = 12111;
const ID_COPY_ALL: i32 = 12112;
const ID_TRANSLATE: i32 = 12113;
fn composer_label() -> String {
    t("&Write a comment (Ctrl+Enter to send)")
}

/// The display label of a reaction keyword; the keyword itself stays English.
fn reaction_label(reaction: &str) -> String {
    match reaction {
        "like" => t("Like"),
        "love" => t("Love"),
        "care" => t("Care"),
        "haha" => t("Haha"),
        "wow" => t("Wow"),
        "sad" => t("Sad"),
        "angry" => t("Angry"),
        "none" => t("Remove reaction"),
        other => other[..1].to_uppercase() + &other[1..],
    }
}
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct PostReader {
    session: u64,
    page: String,
    url: String,
    /// The reader dialog itself, so popups from the results timer can be
    /// parented to it while it is up (an otherwise parentless modal can
    /// freeze the app behind this one).
    dialog: Dialog,
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
    react: Button,
    refresh: Button,
    translate: Button,
    cancel: Button,
    state: Rc<RefCell<ReaderState>>,
}

struct ReelPlayback {
    stream: Option<String>,
    preparing: bool,
}

struct ReaderState {
    target: String,
    reaction: Option<String>,
    comments: Vec<Comment>,
    /// The links list's current contents: the post's links, or the selected
    /// comment's while `links_for` names it.
    links: Vec<Link>,
    /// The post's own links, restored when no comment with links is selected.
    post_links: Vec<Link>,
    /// The comment whose links are showing, when the list is a comment's.
    links_for: Option<String>,
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
    participation: Option<blindfb_core::posts::Participation>,
    share_requested: bool,
    action_requested: Option<i32>,
    playback: Option<ReelPlayback>,
    /// Set from a comment's context menu (View profile): the reader closes
    /// and the main window opens this profile.
    pending_profile: Option<String>,
    profile_close_sent: bool,
}

impl ReaderState {
    fn take_cached_stream(&mut self) -> Option<String> {
        let stream = self.playback.as_mut()?.stream.take()?;
        self.playback = None;
        Some(stream)
    }

    fn take_autoplay_request(&mut self) -> bool {
        if self.loaded
            && !self.busy
            && self
                .playback
                .as_ref()
                .is_some_and(|request| !request.preparing && request.stream.is_none())
        {
            self.playback = None;
            true
        } else {
            false
        }
    }
    fn take_action_request(&mut self) -> Option<i32> {
        if self.loaded && !self.busy {
            self.action_requested.take()
        } else {
            None
        }
    }
    fn take_video_request(&mut self, request: u64) -> bool {
        request == self.video_request && std::mem::take(&mut self.video_requested)
    }

    fn take_share_request(&mut self) -> bool {
        self.loaded && !self.busy && std::mem::take(&mut self.share_requested)
    }
}

impl PostReader {
    /// The reader dialog, so the frame's results timer can parent popups
    /// to it while it is up.
    pub(super) fn dialog(&self) -> Dialog {
        self.dialog
    }

    fn message(&self, ui: &Ui, text: &str) {
        self.status.change_value(text);
        ui.status(text);
    }

    fn controls(&self) {
        let state = self.state.borrow();
        self.play.enable(
            state.video.is_some()
                && state.playback.is_none()
                && !state.video_requested
                && state.media_loading.is_none(),
        );
        self.stop.enable(
            state.playback.is_some()
                || state.video_requested
                || state.media_loading.is_some()
                || state.media_ready,
        );
        self.back.enable(state.media_ready);
        self.forward.enable(state.media_ready);
        self.volume.enable(state.video.is_some());
        self.open_link.enable(self.links.get_selection().is_some());
        self.send.enable(state.loaded && !state.busy);
        self.share.enable(state.loaded && !state.busy);
        self.react.enable(state.loaded && !state.busy);
        let label = blindfb_core::fb::reaction_button_label(state.reaction.as_deref());
        if self.react.get_label() != label {
            self.react.set_label(&label);
            self.react.set_name(&label);
        }
        self.refresh.enable(!state.busy);
        self.translate.enable(state.loaded && !state.busy);
        self.cancel.enable(state.parent.is_some() && !state.busy);
        self.composer.set_editable(!state.busy);
    }

    /// Open the selected link: a profile in BlindFB (the reader closes, as
    /// for a commenter's profile), anything else in the browser.
    fn open_link(&self) {
        let Some(link) = self
            .links
            .get_selection()
            .and_then(|index| self.state.borrow().links.get(index as usize).cloned())
        else {
            return;
        };
        match link.profile {
            Some(profile) => self.state.borrow_mut().pending_profile = Some(profile),
            None => open_url(&link.url),
        }
    }

    /// Point the links list at `target` under an accessible `name`, keeping
    /// the selection on the same URL when it survives the swap.
    fn show_links(&self, target: Vec<Link>, name: &str) {
        let selected_url = self.links.get_selection().and_then(|index| {
            self.state
                .borrow()
                .links
                .get(index as usize)
                .map(|link| link.url.clone())
        });
        self.links.clear();
        for link in &target {
            self.links
                .append(&format!("{} — {}", link.label, link.url));
        }
        if let Some(index) = selected_url
            .and_then(|url| target.iter().position(|link| link.url == url))
            .or_else(|| (!target.is_empty()).then_some(0))
        {
            self.links.set_selection(index as u32, true);
        }
        self.links.set_name(name);
        self.state.borrow_mut().links = target;
        self.controls();
    }

    /// Copy the selected comment ("Author: text"), or — with no comment —
    /// the post text. Reports what it copied in the reader status.
    fn copy_comment(&self, ui: &Ui) {
        let text = {
            let state = self.state.borrow();
            self.comments
                .get_selection()
                .and_then(|index| state.comments.get(index as usize))
                .map(|comment| format!("{}: {}", comment.author, comment.text))
                .unwrap_or_else(|| self.post.get_value().to_string())
        };
        if text.is_empty() {
            self.message(ui, &t("Nothing to copy yet."));
            return;
        }
        let message = if Clipboard::get().set_text(&text) {
            t("Copied.")
        } else {
            t("Could not copy.")
        };
        self.message(ui, &message);
    }

    /// Copy the post and every comment, so a whole discussion can be
    /// pasted into a note or message at once. Replies are marked with
    /// their parent when they have one; links stay attached as they are.
    fn copy_all(&self, ui: &Ui) {
        let text = {
            let state = self.state.borrow();
            let mut out = self.post.get_value().to_string();
            for comment in &state.comments {
                out.push('\n');
                if comment.parent_id.is_some() {
                    out.push_str("  ");
                }
                out.push_str(&format!("{}: {}", comment.author, comment.text));
                for link in &comment.links {
                    out.push_str(&format!(" ({})", link.url));
                }
            }
            out
        };
        if text.trim().is_empty() {
            self.message(ui, &t("Nothing to copy yet."));
            return;
        }
        let message = if Clipboard::get().set_text(&text) {
            t("Post and comments copied.")
        } else {
            t("Could not copy.")
        };
        self.message(ui, &message);
    }

    /// Open the first link of the selected comment in the browser, if it
    /// has links. The Links list already shows them; this skips Tabbing
    /// into it.
    fn open_first_comment_link(&self, ui: &Ui) {
        let first = {
            let state = self.state.borrow();
            self.comments
                .get_selection()
                .and_then(|index| state.comments.get(index as usize))
                .and_then(|comment| comment.links.first())
                .map(|link| link.url.clone())
        };
        match first {
            Some(url) => open_url(&url),
            None => self.message(ui, &t("This comment has no link to open.")),
        }
    }

    /// The links list belongs to the selected comment when it has links of
    /// its own, and to the post otherwise. Called when the comment selection
    /// moves and after every (re)load, so a refresh keeps the same owner.
    fn follow_comment_links(&self) {
        let (target, name, owner) = {
            let state = self.state.borrow();
            let selected = self
                .comments
                .get_selection()
                .and_then(|i| state.comments.get(i as usize));
            match selected.filter(|comment| !comment.links.is_empty()) {
                Some(comment) => (
                    comment.links.clone(),
                    tf!(
                        "Links in {author}'s comment. Enter opens the selected link in your browser",
                        author = comment.author
                    ),
                    Some(comment.id.clone()),
                ),
                None => (
                    state.post_links.clone(),
                    t("Post links. Enter opens the selected link in your browser"),
                    None,
                ),
            }
        };
        let state = self.state.borrow();
        if state.links_for == owner && state.links == target {
            return;
        }
        drop(state);
        self.state.borrow_mut().links_for = owner;
        self.show_links(target, &name);
    }

    fn play_label(&self, playing: bool) {
        // Both labels go through t! here and where the button is built, so the
        // comparison holds in every language.
        let label = if playing {
            t("&Pause video")
        } else {
            t("&Play video")
        };
        if self.play.get_label() == label {
            return;
        }
        self.play.set_label(&label);
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
                (self.media.pause(), t("Video paused."))
            } else {
                (self.media.play(), t("Playing video."))
            };
            if !ok {
                self.state.borrow_mut().media_ready = false;
            }
            self.message(
                ui,
                &if ok {
                    message
                } else {
                    t("Playback failed. Press Play to retry.")
                },
            );
            self.play_label(ok && self.media.get_state() == MediaState::Playing);
            self.controls();
            return;
        }
        let Some(target) = state.video.clone() else {
            return;
        };
        state.video_requested = true;
        state.video_request = state.video_request.wrapping_add(1);
        let request = state.video_request;
        drop(state);
        self.controls();
        ui.send(Job {
            task: Task::PostVideo {
                page: self.page.clone(),
                target,
            },
            tag: Tag::PostVideo(self.session, request),
        });
        self.message(ui, &t("Preparing video…"));
    }

    fn load_stream(&self, ui: &Ui, data: &str) {
        let info = match blindfb_core::media::parse_stream(data) {
            Ok(info) => info,
            Err(error) => {
                self.message(ui, &error);
                return;
            }
        };
        self.state.borrow_mut().media_loading = Some(Instant::now());
        self.message(ui, &t("Loading video…"));
        if !self.media.load_uri(&info.url) {
            self.state.borrow_mut().media_loading = None;
            self.message(ui, &t("Could not load video. Press Play to retry."));
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
            &if was_ready && !stopped {
                t("Could not stop video. Try Pause or close the reader.")
            } else {
                t("Video stopped.")
            },
        );
    }

    fn seek(&self, ui: &Ui, offset: i64) {
        let position = seek_position(self.media.tell(), self.media.length(), offset);
        if self.media.seek(position) < 0 {
            self.message(ui, &t("This video could not seek."));
        } else {
            self.message(
                ui,
                &ntf!(
                    "Video position: {n} second.",
                    "Video position: {n} seconds.",
                    position / 1000
                ),
            );
        }
    }

    fn poll_player(&self, ui: &Ui) {
        for event in self.media.poll() {
            match event {
                PlayerEvent::Loaded => {
                    if self.state.borrow_mut().media_loading.take().is_none() {
                        continue;
                    }
                    self.media.set_volume(self.volume.value() as f64 / 100.0);
                    let ok = self.media.play();
                    self.state.borrow_mut().media_ready = ok;
                    self.play_label(ok);
                    self.controls();
                    self.message(
                        ui,
                        &if ok {
                            t("Playing video.")
                        } else {
                            t("Playback failed. Press Play to retry.")
                        },
                    );
                }
                PlayerEvent::StateChanged => {
                    self.play_label(self.media.get_state() == MediaState::Playing)
                }
                PlayerEvent::Finished => {
                    self.play_label(false);
                    self.message(ui, &t("Video finished."));
                }
                PlayerEvent::Error(error) => {
                    let mut state = self.state.borrow_mut();
                    state.media_loading = None;
                    state.media_ready = false;
                    drop(state);
                    self.play_label(false);
                    self.controls();
                    self.message(ui, &tf!("{error} Press Play to retry.", error = error));
                }
                PlayerEvent::CommandFailed(error) => {
                    // Playback continues; only say the command did not land.
                    self.message(ui, &error);
                }
            }
        }
    }

    fn cancel_reply(&self) {
        self.state.borrow_mut().parent = None;
        let label = composer_label();
        self.composer_label.set_label(&label);
        self.composer.set_name(&label.replace('&', ""));
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
            &t("Opening Facebook's share dialog…"),
        );
    }

    /// Ask for the share text once Facebook reported the audience; runs from the reader's timer.
    fn share_prompt(&self, ui: &Ui, dialog: &Dialog, audience: &str) {
        let entry = TextEntryDialog::builder(
            dialog,
            &tf!(
                "Share this post to your feed. Audience: {audience}.\nSay something about it (optional), then press OK to share:",
                audience = audience
            ),
            &t("Share post"),
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
                self.message(ui, &t("Sharing…"));
            }
            None => {
                ui.send(Job {
                    task: Task::PostShareCancel { page },
                    tag: Tag::PostShareClosed(self.session),
                });
                self.message(ui, &t("Share cancelled. Nothing was shared."));
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
            ID_TRANSLATE => self.start(
                ui,
                Task::PostTranslate {
                    page: self.page.clone(),
                },
                Tag::PostTranslated(self.session),
                &t("Asking Facebook to translate…"),
            ),
            ID_COPY_POST => {
                // Before the post arrives the field holds the loading placeholder;
                // copying it and reporting success leaves junk on the clipboard.
                if !self.state.borrow().loaded {
                    self.message(ui, &t("The post has not loaded yet. Try again in a moment."));
                    return;
                }
                let copied = Clipboard::get().set_text(&self.post.get_value());
                self.message(
                    ui,
                    &if copied {
                        t("Post copied.")
                    } else {
                        t("Could not copy the post.")
                    },
                );
            }
            ID_COPY_POST_LINK => {
                let copied = Clipboard::get().set_text(&self.state.borrow().target);
                self.message(
                    ui,
                    &if copied {
                        t("Link copied.")
                    } else {
                        t("Could not copy the link.")
                    },
                );
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
                        &t("Reacting…"),
                    );
                    return;
                }
                ui.send(Job {
                    task: Task::Fb(react_args(&target, reaction), None),
                    tag: Tag::Reacted { post: target, reaction: reaction.to_owned(), account: ui.state.borrow().account, reader: Some(self.session) },
                });
                self.state.borrow_mut().busy = true;
                self.controls();
                self.message(ui, &t("Reacting…"));
            }
            _ => {}
        }
    }

    fn send(&self, ui: &Ui) {
        let text = self.composer.get_value();
        if text.trim().is_empty() {
            self.message(ui, &t("Write a comment or reply first."));
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
            task: Task::PostSend {
                page: self.page.clone(),
                parent,
                text,
            },
            tag: Tag::PostSent(self.session),
        });
        self.message(ui, &t("Sending comment…"));
    }

    fn menu_action(&self, ui: &Ui, action: i32) {
        let selected = self.comments.get_selection().and_then(|index| {
            self.state
                .borrow()
                .comments
                .get(index as usize)
                .map(|comment| {
                    (
                        comment.id.clone(),
                        comment.author.clone(),
                        comment.text.clone(),
                        comment.url.clone(),
                        comment.author_ref.clone(),
                    )
                })
        });
        let Some((id, author, text, url, author_ref)) = selected else {
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
                let label = tf!("Reply to {author} (Ctrl+Enter to send)", author = author);
                self.composer_label.set_label(&label);
                self.composer.set_name(&label);
                self.controls();
                self.composer.set_focus();
                self.message(ui, &tf!("Replying to {author}.", author = author));
            }
            ID_COPY => {
                let message = if Clipboard::get().set_text(&format!("{author}: {text}")) {
                    t("Comment copied.")
                } else {
                    t("Could not copy the comment.")
                };
                self.message(ui, &message);
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
                    &tf!("Changing your Like on {author}'s comment…", author = author),
                );
            }
            ID_COMMENT_PROFILE => {
                let who = author_ref.unwrap_or(author);
                if !who.is_empty() {
                    self.state.borrow_mut().pending_profile = Some(who);
                    self.message(ui, &t("Opening the commenter's profile…"));
                }
            }
            ID_COMMENT_FRIEND => {
                let who = author_ref.unwrap_or_else(|| author.clone());
                if !who.is_empty() {
                    ui.send(Job {
                        task: Task::Fb(friend_add_args(&who), None),
                        tag: Tag::Text,
                    });
                    self.message(ui, &tf!("Sending a friend request to {author}…", author = author));
                }
            }
            _ => {}
        }
    }
}

pub(super) fn open(ui: &Ui, target: &str, share_requested: bool) {
    open_requested(ui, target, share_requested, None, None);
}

/// What the reader does first when opened over another dialog.
pub(super) enum Over<'a> {
    Read,
    Share,
    React(&'a str),
    Comment,
}

/// Open the reader over another dialog (the group window). False when it did
/// not open or left for another view (a profile), so that dialog closes too.
pub(super) fn open_over(ui: &Ui, target: &str, what: Over) -> bool {
    let action = match what {
        Over::React(reaction) => REACTIONS
            .iter()
            .position(|kind| *kind == reaction)
            .map(|index| ID_REACT + index as i32),
        Over::Comment => Some(ID_POST_COMMENT),
        Over::Read | Over::Share => None,
    };
    open_requested(ui, target, matches!(what, Over::Share), action, None)
}

pub(super) fn open_reel(ui: &Ui, target: &str, stream: Option<String>, preparing: bool) {
    open_requested(
        ui,
        target,
        false,
        None,
        Some(ReelPlayback { stream, preparing }),
    );
}

pub(super) fn react(ui: &Ui, target: &str, reaction: &str) {
    if let Some(index) = REACTIONS.iter().position(|kind| *kind == reaction) {
        open_requested(ui, target, false, Some(ID_REACT + index as i32), None);
    }
}

pub(super) fn comment(ui: &Ui, target: &str) {
    open_requested(ui, target, false, Some(ID_POST_COMMENT), None);
}

fn open_requested(
    ui: &Ui,
    target: &str,
    share_requested: bool,
    action_requested: Option<i32>,
    playback: Option<ReelPlayback>,
) -> bool {
    if ui.post_reader.borrow().is_some() {
        return false;
    }
    let selection = ui.items.get_selection();
    let preview = super::selected(ui)
        .filter(|(_, record)| {
            super::post_target(&ui.state.borrow().view, record)
                .and_then(blindfb_core::posts::reader_url)
                .as_deref()
                == Some(target)
        })
        .map(|_| ui.details.get_value());
    let initial_links = blindfb_core::posts::ordered_links(
        if preview.is_some() {
            ui.link_targets.borrow().clone()
        } else {
            Vec::new()
        },
        target,
    );
    let autoplay = playback.is_some();
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let dialog = Dialog::builder(&ui.frame, &t("Post and comments"))
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let links_label = StaticText::builder(&dialog)
        .with_label(&t("Post lin&ks"))
        .build();
    let links = ListBox::builder(&dialog).build();
    links.set_name(&t("Post links. Enter opens the selected link in your browser"));
    for link in &initial_links {
        links.append(&format!("{} — {}", link.label, link.url));
    }
    links.set_selection(0, true);
    let open_link = Button::builder(&dialog).with_label(&t("&Open link")).build();
    let post_label = StaticText::builder(&dialog)
        .with_label(&t("&Full post"))
        .build();
    let post = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    post.set_name(&t(
        "Full post. Applications or Shift+F10 for links, comment, react, share and copy"
    ));
    let loading = t("Loading post and comments…");
    post.change_value(preview.as_deref().unwrap_or(&loading));
    let media_label = StaticText::builder(&dialog)
        .with_label(&t("Video player"))
        .build();
    let media = Player::new(&dialog);
    let player_buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let play = Button::builder(&dialog).with_label(&t("&Play video")).build();
    let stop = Button::builder(&dialog).with_label(&t("S&top video")).build();
    let back = Button::builder(&dialog)
        .with_label(&t("Back 10 seconds"))
        .build();
    let forward = Button::builder(&dialog)
        .with_label(&t("Forward 10 seconds"))
        .build();
    for button in [&play, &stop, &back, &forward] {
        // set_name overrides wx's own accessible name, so strip the mnemonic
        // marker: a raw label becomes "and Play video" for a screen reader.
        button.set_name(&button.get_label().replace('&', ""));
        player_buttons.add(button, 0, SizerFlag::All, 4);
    }
    let volume_label = StaticText::builder(&dialog)
        .with_label(&t("Video &volume (percent)"))
        .build();
    let volume = Slider::builder(&dialog)
        .with_value(blindfb_core::media::saved_volume() as i32)
        .with_min_value(0)
        .with_max_value(100)
        .build();
    volume.set_name(&t("Video volume, percent"));
    let comments_label = StaticText::builder(&dialog).with_label(&t("&Comments")).build();
    let comments = ListBox::builder(&dialog).build();
    // The keys are the user's, from File > Keyboard shortcuts.
    let none = t("no shortcut");
    comments.set_name(&tf!(
        "Comments. {copy} copies the comment. {copy_all} copies the post and all comments. {open} opens the comment's first link. Applications or Shift+F10 for reply, like, profile, friend, copy and Facebook actions",
        copy = post_key(COMMENT_COPY).unwrap_or_else(|| none.clone()),
        copy_all = post_key(COMMENT_COPY_ALL).unwrap_or_else(|| none.clone()),
        open = post_key(COMMENT_OPEN_LINK).unwrap_or(none),
    ));
    let composer_text = composer_label();
    let composer_label = StaticText::builder(&dialog).with_label(&composer_text).build();
    let composer = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::MultiLine)
        .build();
    composer.set_name(&composer_text.replace('&', ""));
    let status_label = StaticText::builder(&dialog)
        .with_label(&t("Reader status"))
        .build();
    let status = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_name(&t("Reader status"));
    status.enable_focus_from_keyboard();
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let send = Button::builder(&dialog)
        .with_label(&t("&Send (Ctrl+Enter)"))
        .build();
    let share = Button::builder(&dialog).with_label(&t("S&hare…")).build();
    let react = Button::builder(&dialog).with_label(&t("React")).build();
    let refresh = Button::builder(&dialog).with_label(&t("&Refresh")).build();
    let translate = Button::builder(&dialog).with_label(&t("Tr&anslate")).build();
    let cancel = Button::builder(&dialog).with_label(&t("Ca&ncel reply")).build();
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    for button in [&send, &share, &react, &refresh, &translate, &cancel, &close] {
        // Same as above: strip the mnemonic marker before it becomes the
        // accessible name a screen reader announces.
        button.set_name(&button.get_label().replace('&', ""));
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
        dialog,
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
        react,
        refresh,
        translate,
        cancel,
        state: Rc::new(RefCell::new(ReaderState {
            target: target.to_owned(),
            reaction: None,
            comments: Vec::new(),
            links: initial_links.clone(),
            post_links: initial_links,
            links_for: None,
            video: autoplay.then(|| target.to_owned()),
            video_requested: false,
            video_request: 0,
            media_ready: false,
            media_loading: None,
            parent: None,
            busy: true,
            loaded: false,
            share_audience: None,
            participation: None,
            share_requested,
            action_requested,
            playback,
            pending_profile: None,
            profile_close_sent: false,
        })),
    };
    reader.controls();
    *ui.post_reader.borrow_mut() = Some(reader.clone());
    let r = reader.clone();
    open_link.on_click(move |_| r.open_link());
    let r = reader.clone();
    links.on_item_double_clicked(move |_| r.open_link());
    let r = reader.clone();
    links.on_selection_changed(move |_| r.controls());
    let r = reader.clone();
    comments.on_selection_changed(move |_| r.follow_comment_links());
    let r = reader.clone();
    super::type_ahead::attach(&comments, move |_| r.follow_comment_links());
    let (r, u) = (reader.clone(), ui.clone());
    comments.bind_internal(EventType::KEY_DOWN, move |event| {
        // Copy the selected comment (Ctrl+C), the whole discussion
        // (Ctrl+Shift+C), or open the comment's first link (Ctrl+Enter),
        // unless File > Keyboard shortcuts changed them.
        if r.comments.has_focus() {
            if let Some(code) = event.get_key_code() {
                let (ctrl, alt, shift) = (event.control_down(), event.alt_down(), event.shift_down());
                let pressed = |key| {
                    blindfb_core::shortcuts::matches(post_key(key).as_deref(), code, ctrl, alt, shift)
                };
                if pressed(COMMENT_COPY_ALL) {
                    event.skip(false);
                    r.copy_all(&u);
                    return;
                }
                if pressed(COMMENT_COPY) {
                    event.skip(false);
                    r.copy_comment(&u);
                    return;
                }
                if pressed(COMMENT_OPEN_LINK) {
                    event.skip(false);
                    r.open_first_comment_link(&u);
                    return;
                }
            }
        }
        event.skip(true);
    });
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
        // The choice is remembered for the next video and launch; the
        // live player takes it right away when it is ready.
        let _ = blindfb_core::media::save_volume(r.volume.value() as u8);
        if r.state.borrow().media_ready && !r.media.set_volume(r.volume.value() as f64 / 100.0) {
            r.message(&u, &t("Could not set video volume."));
        }
    });
    let (r, u) = (reader.clone(), ui.clone());
    send.on_click(move |_| r.send(&u));
    let r = reader.clone();
    react.on_click(move |_| {
        let mut menu = Menu::builder().build();
        for (i, reaction) in REACTIONS.iter().enumerate() {
            menu.append(ID_REACT + i as i32, &reaction_label(reaction), "", ItemKind::Check);
            menu.check_item(ID_REACT + i as i32, r.state.borrow().reaction.as_deref() == Some(*reaction));
        }
        r.react.popup_menu(&mut menu, None);
    });
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
                Task::PostRefresh {
                    page: r.page.clone(),
                }
            } else {
                Task::PostLoad {
                    page: r.page.clone(),
                    target: r.url.clone(),
                }
            },
            tag: Tag::PostLoaded(r.session),
        });
        r.message(&u, &t("Refreshing comments…"));
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
            state.comments.get(i as usize).map(|c| {
                (
                    !state.busy && !c.id.is_empty(),
                    c.url.is_some(),
                    !state.busy && (c.author_ref.is_some() || !c.author.is_empty()),
                )
            })
        });
        let Some((reply, web, person)) = available else {
            return;
        };
        let mut menu = Menu::builder()
            .append_item(ID_REPLY, &t("&Reply"), &t("Reply to this exact comment"))
            .append_item(
                ID_LIKE,
                &t("&Like / unlike"),
                &t("Like this comment, or remove your Like"),
            )
            .append_item(ID_COPY, &t("&Copy comment"), &t("Copy the full comment"))
            .append_item(
                ID_COPY_ALL,
                &t("Copy &all comments"),
                &t("Copy the post and every comment"),
            )
            .append_item(
                ID_WEB,
                &t("Open comment on &Facebook"),
                &t("Open the comment permalink"),
            )
            .append_separator()
            .append_item(
                ID_COMMENT_PROFILE,
                &t("View &profile"),
                &t("Open the commenter's profile"),
            )
            .append_item(
                ID_COMMENT_FRIEND,
                &t("Add &friend"),
                &t("Send the commenter a friend request"),
            )
            .append_separator()
            .append_item(
                ID_POST_COMMENT,
                &t("Comment on the &post"),
                &t("Write a new top-level comment"),
            )
            .build();
        menu.enable_item(ID_REPLY, reply);
        menu.enable_item(ID_LIKE, reply);
        menu.enable_item(ID_WEB, web);
        menu.enable_item(ID_COMMENT_PROFILE, person);
        menu.enable_item(ID_COMMENT_FRIEND, person);
        let comment_links = r.comments.get_selection().and_then(|i| {
            r.state
                .borrow()
                .comments
                .get(i as usize)
                .map(|comment| comment.links.clone())
        });
        if let Some(links) = comment_links.filter(|links| !links.is_empty()) {
            append_links_menu(&menu, &links);
        }
        r.comments.popup_menu(&mut menu, None);
    });
    let r = reader.clone();
    post.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        let (loaded, ready) = {
            let state = r.state.borrow();
            (state.loaded, state.loaded && !state.busy)
        };
        let mut menu = Menu::builder()
            .append_item(
                ID_POST_COMMENT,
                &t("&Comment…"),
                &t("Write a comment on this post"),
            )
            .append_item(
                ID_SHARE,
                &t("S&hare…"),
                &t("Share with Facebook's own share dialog"),
            )
            .build();
        let react = Menu::builder().build();
        for (i, reaction) in REACTIONS.iter().enumerate() {
            let label = reaction_label(reaction);
            react.append(ID_REACT + i as i32, &label, "", ItemKind::Normal);
        }
        menu.append_submenu(react, &blindfb_core::fb::reaction_button_label(r.state.borrow().reaction.as_deref()), "");
        append_links_menu(&menu, &r.state.borrow().post_links);
        menu.append_separator();
        menu.append(
            ID_TRANSLATE,
            &t("&Translate post and comments (Facebook)"),
            "",
            ItemKind::Normal,
        );
        menu.append(ID_COPY_POST, &t("Cop&y post text"), "", ItemKind::Normal);
        menu.append(ID_COPY_ALL, &t("Copy &all comments"), "", ItemKind::Normal);
        menu.append(ID_COPY_POST_LINK, &t("Copy &link"), "", ItemKind::Normal);
        menu.append(
            ID_WEB_POST,
            &t("Open on &Facebook (web)"),
            "",
            ItemKind::Normal,
        );
        menu.enable_item(ID_COPY_POST, loaded);
        menu.enable_item(ID_SHARE, ready);
        menu.enable_item(ID_TRANSLATE, ready);
        r.post.popup_menu(&mut menu, None);
    });
    let (r, u) = (reader.clone(), ui.clone());
    share.on_click(move |_| r.share_start(&u));
    let (r, u) = (reader.clone(), ui.clone());
    translate.on_click(move |_| r.post_action(&u, ID_TRANSLATE));
    let (r, u) = (reader.clone(), ui.clone());
    dialog.bind_internal(EventType::MENU, move |event| match event.get_id() {
        ID_REPLY | ID_COPY | ID_WEB | ID_LIKE | ID_COMMENT_PROFILE | ID_COMMENT_FRIEND => {
            event.skip(false);
            r.menu_action(&u, event.get_id());
        }
        ID_COPY_ALL => {
            event.skip(false);
            r.copy_all(&u);
        }
        id if (ID_POST_COMMENT..=ID_WEB_POST).contains(&id)
            || id == ID_TRANSLATE
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
            if resolve {
                r.play(&u);
            }
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
            r.message(&u, &t("Video loading timed out. Press Play to retry."));
        }
        let close_for_profile = {
            let mut state = r.state.borrow_mut();
            if state.pending_profile.is_some() && !state.profile_close_sent {
                state.profile_close_sent = true;
                true
            } else {
                false
            }
        };
        if close_for_profile {
            dialog.end_modal(ID_CANCEL);
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
        let questions = r.state.borrow_mut().participation.take();
        if let Some(questions) = questions {
            let answers = super::participation_dialog::show(&dialog, &questions);
            let submitted = answers.is_some();
            u.send(Job {
                task: Task::PostParticipation {
                    page: r.page.clone(), token: questions.token, answers,
                },
                tag: Tag::PostParticipation(r.session, submitted),
            });
        }
    });
    share_timer.start(150, false);
    ui.send(Job {
        task: Task::PostLoad {
            page: reader.page.clone(),
            target: target.to_owned(),
        },
        tag: Tag::PostLoaded(session),
    });
    reader.message(ui, &loading);
    // Land on the full post text, not the link list: the text is why the
    // reader was opened; the links are one Tab away.
    post.set_focus();
    dialog.show_modal();
    share_timer.stop();
    reader.media.shutdown();
    // Remove the session before destroying controls, so queued completions cannot touch them.
    ui.post_reader.borrow_mut().take();
    let _ = ui.jobs.send(Job {
        task: Task::PostClose {
            page: reader.page.clone(),
        },
        tag: Tag::Silent,
    });
    dialog.destroy();
    if let Some(who) = reader.state.borrow_mut().pending_profile.take() {
        super::load_view(ui, super::View::ProfileOf(who));
        return false;
    }
    if let Some(index) = selection.filter(|i| *i < ui.items.get_count()) {
        ui.items.set_selection(index, true);
    }
    ui.items.set_focus();
    true
}

pub(super) fn is_video_page(url: &str) -> bool {
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let Some((_, path)) = rest.split_once('/') else {
        return false;
    };
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    path.starts_with("reel/")
        || path.contains("/videos/")
        || path.starts_with("videos/")
        || (matches!(path, "watch" | "watch/" | "watch/live" | "watch/live/")
            && query.split('&').any(|pair| pair.starts_with("v=")))
}

fn seek_position(current: i64, length: i64, offset: i64) -> i64 {
    let target = current.saturating_add(offset).max(0);
    if length > 0 {
        target.min(length)
    } else {
        target
    }
}

pub(super) fn stream(ui: &Ui, session: u64, request: u64, result: Result<String, String>) {
    let Some(reader) = active(ui, session) else {
        return;
    };
    if !reader.state.borrow_mut().take_video_request(request) {
        reader.controls();
        return;
    }
    match result {
        Ok(url) => reader.load_stream(ui, &url),
        Err(error) => reader.message(ui, &tf!("{error} Press Play to retry.", error = error)),
    }
    reader.controls();
}

pub(super) fn prepared(ui: &Ui, target: &str, result: Result<String, String>) {
    let reader = ui
        .post_reader
        .borrow()
        .as_ref()
        .filter(|reader| reader.url == target)
        .cloned();
    let Some(reader) = reader else { return };
    let mut state = reader.state.borrow_mut();
    if let Some(request) = state.playback.as_mut() {
        request.preparing = false;
        request.stream = result.ok();
    }
}

/// One comments-list row, speaker first. A reply names the comment it answers,
/// so it cannot be mistaken for a top-level comment (issue #22).
fn comment_row(comment: &Comment, comments: &[Comment]) -> String {
    match comment
        .parent_id
        .as_ref()
        .and_then(|parent| comments.iter().find(|other| &other.id == parent))
    {
        Some(parent) => {
            // Facebook starts a reply with a mention of the person answered; the
            // row already names them. A reply to a reply answers that person,
            // not the author of the thread's first comment.
            let to = comment.reply_to.as_deref().unwrap_or(&parent.author);
            let text = comment
                .text
                .strip_prefix(to)
                .filter(|rest| rest.starts_with(char::is_whitespace))
                .map_or(comment.text.as_str(), str::trim_start);
            format!(
                "{}: {} — {}",
                // TRANSLATORS: a reply in the comments list: who wrote it, then
                // whose comment it answers ("Bob to Ann: text — 2h")
                tf!("{author} to {parent}", author = comment.author, parent = to),
                text,
                comment.time
            )
        }
        None => format!("{}: {} — {}", comment.author, comment.text, comment.time),
    }
}

fn selected_index(comments: &[Comment], id: Option<&str>) -> Option<u32> {
    id.and_then(|id| comments.iter().position(|comment| comment.id == id))
        .map(|i| i as u32)
}

/// The open reader, if it is still the one a completion belongs to.
fn active(ui: &Ui, session: u64) -> Option<PostReader> {
    ui.post_reader
        .borrow()
        .as_ref()
        .filter(|r| r.session == session)
        .cloned()
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

/// Update the reader only after Facebook confirmed the requested reaction.
pub(super) fn reaction_changed(ui: &Ui, session: u64, reaction: &str) {
    if let Some(reader) = active(ui, session) {
        reader.state.borrow_mut().reaction = Some(reaction.to_owned());
        note(ui, session, Ok(t("Reaction set.")), true);
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
    if sent {
        if let Ok(json) = &result {
            if let Ok(Some(form)) = blindfb_core::posts::parse_participation(json) {
                reader.state.borrow_mut().participation = Some(form);
                reader.message(ui, &t("This group requires participation answers. Your comment draft has been kept."));
                return;
            }
        }
    }
    reader.state.borrow_mut().busy = false;
    match result.and_then(|json| parse_detail(&json)) {
        Err(error) => {
            reader.message(ui, &tf!("{error} Your draft has been kept.", error = error));
        }
        Ok(detail) => {
            super::remember_reaction(ui, &detail.url, detail.reaction.as_deref());
            // Before this refresh, so a sent comment can be recognised as new.
            let previous_ids: std::collections::HashSet<String> = reader
                .state
                .borrow()
                .comments
                .iter()
                .map(|c| c.id.clone())
                .collect();
            let selected_id = reader.comments.get_selection().and_then(|index| {
                reader
                    .state
                    .borrow()
                    .comments
                    .get(index as usize)
                    .map(|c| c.id.clone())
            });
            let text = format!("{}\n\n{}\n\n{}", detail.author, detail.text, detail.time);
            if reader.post.get_value() != text {
                reader.post.change_value(&text);
            }
            for (index, comment) in detail.comments.iter().enumerate() {
                let row = comment_row(comment, &detail.comments);
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
            // A sent comment is the confirmation: land on it in the list, so
            // the screen reader reads the comment that just posted (issue #22).
            let posted = sent.then(|| {
                detail
                    .comments
                    .iter()
                    .position(|comment| !previous_ids.contains(&comment.id))
                    .map(|index| index as u32)
            });
            if let Some(Some(index)) = posted {
                if reader.comments.get_selection() != Some(index) {
                    reader.comments.set_selection(index, true);
                }
                reader.comments.set_focus();
            } else if let Some(index) = selected_index(&detail.comments, selected_id.as_deref()) {
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
                state.target = detail.url;
                state.post_links = detail.links;
                state.video = detail.video;
                state.reaction = detail.reaction;
                state.loaded = true;
            }
            reader.follow_comment_links();
            if sent {
                reader.composer.change_value("");
                reader.cancel_reply();
            }
            // `done` is a whole sentence from the caller; the count is a second
            // sentence of its own, so neither is assembled from pieces.
            let loaded = ntf!("{n} comment loaded.", "{n} comments loaded.", count);
            reader.message(ui, &format!("{done} {loaded}"));
        }
    }
    reader.controls();
}

pub(super) fn participation_done(ui: &Ui, session: u64, result: Result<String, String>, submitted: bool) {
    let Some(reader) = active(ui, session) else { return };
    if let Ok(json) = &result {
        if let Ok(Some(form)) = blindfb_core::posts::parse_participation(json) {
            reader.message(ui, &form.error);
            reader.state.borrow_mut().participation = Some(form);
            return;
        }
    }
    note(ui, session, result.map(|_| if submitted {
        t("Answers submitted. Refresh to check whether your comment posted. Your draft has been kept.")
    } else {
        t("Participation review cancelled. Your comment draft has been kept.")
    }), true);
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
        assert!(!is_video_page(
            "https://www.facebook.com/a/posts/123?next=/reel/123"
        ));
        assert!(!is_video_page("https://www.facebook.com/watch/?next=v=123"));
        assert_eq!(seek_position(5_000, 60_000, -10_000), 0);
        assert_eq!(seek_position(55_000, 60_000, 10_000), 60_000);
        assert_eq!(seek_position(55_000, 0, 10_000), 65_000);
    }

    #[test]
    fn requested_share_waits_for_load_and_starts_once() {
        let mut state = ReaderState {
            target: "https://www.facebook.com/a/posts/123".to_owned(),
            reaction: None,
            comments: Vec::new(),
            links: Vec::new(),
            post_links: Vec::new(),
            links_for: None,
            video: None,
            video_requested: false,
            video_request: 0,
            media_ready: false,
            media_loading: None,
            parent: None,
            busy: true,
            loaded: false,
            share_audience: None,
            participation: None,
            share_requested: true,
            action_requested: Some(ID_POST_COMMENT),
            playback: None,
            pending_profile: None,
            profile_close_sent: false,
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
        state.playback = Some(ReelPlayback {
            stream: Some("https://video.example/cached".into()),
            preparing: false,
        });
        assert_eq!(
            state.take_cached_stream().as_deref(),
            Some("https://video.example/cached")
        );
        assert!(state.take_cached_stream().is_none());
        state.playback = Some(ReelPlayback {
            stream: None,
            preparing: true,
        });
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
    fn reply_rows_name_the_comment_they_answer() {
        let comments = parse_detail(r#"{"url":"https://www.facebook.com/a/posts/1","author":"A","time":"now","text":"Post","comments":[{"id":"1","author":"Ann","time":"1h","text":"Top level","parent_id":null,"url":null},{"id":"2","author":"Bob","time":"2h","text":"A reply","parent_id":"1","url":null}]}"#).unwrap().comments;
        assert_eq!(comment_row(&comments[0], &comments), "Ann: Top level — 1h");
        assert_eq!(comment_row(&comments[1], &comments), "Bob to Ann: A reply — 2h");
        // The leading mention of the person answered is dropped.
        let mut mention = comments[1].clone();
        mention.text = "Ann\nexactly.".into();
        assert_eq!(comment_row(&mention, &comments), "Bob to Ann: exactly. — 2h");
        // A reply to a reply in Ann's thread names the person it answers.
        let mut nested = comments[1].clone();
        nested.text = "Cleo Day thanks, you too?".into();
        nested.reply_to = Some("Cleo Day".into());
        assert_eq!(comment_row(&nested, &comments), "Bob to Cleo Day: thanks, you too? — 2h");
    }

    #[test]
    fn refresh_restores_identity_instead_of_row_number() {
        let comments = parse_detail(r#"{"url":"https://www.facebook.com/a/posts/1","author":"A","time":"now","text":"Post","comments":[{"id":"second","author":"B","time":"now","text":"Second","parent_id":null,"url":null},{"id":"first","author":"A","time":"now","text":"First","parent_id":null,"url":null}]}"#).unwrap().comments;
        assert_eq!(selected_index(&comments, Some("first")), Some(1));
        assert_eq!(selected_index(&comments, Some("removed")), None);
        assert_eq!(selected_index(&comments, None), None);
    }
}
