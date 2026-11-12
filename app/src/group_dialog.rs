//! The group window: a group's posts and its full member list in one
//! dialog. Opened by activating a group in the Groups list. Enter on a
//! post opens it in the post reader over this window, so closing the
//! reader lands back on the same post; members carry the usual people
//! actions (profile, message, friend request, web) on their context menu.
use super::{load_view, open_url, Job, Tag, Task, Ui, View};
use blindfb_core::fb::web_url;
use blindfb_core::fb::{
    friend_add_args, members_args, parse_feed, parse_records, posts_args, Record,
};
use blindfb_core::{ntf, tf};
use patois::t;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use wxdragon::keycode::{WXK_NUMPAD_ENTER, WXK_RETURN};
use wxdragon::prelude::*;
use wx_utils::confirm;

const ID_MEMBER_PROFILE: i32 = 12201;
const ID_MEMBER_MESSAGE: i32 = 12202;
const ID_MEMBER_FRIEND: i32 = 12203;
const ID_MEMBER_WEB: i32 = 12204;
const MEMBER_CAP: u32 = 500;
/// The window loads like the news feed: a small first page paints at
/// once, then one background top-up fills the rest in.
const POSTS_FIRST: u32 = 10;
const POSTS_CAP: u32 = 50;
const MEMBERS_FIRST: u32 = 50;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct GroupWindow {
    session: u64,
    posts: ListBox,
    members: ListBox,
    status: TextCtrl,
    state: Arc<Mutex<GroupState>>,
}

struct GroupState {
    name: String,
    url: String,
    posts: Vec<Record>,
    members: Vec<Record>,
    posts_count: Option<usize>,
    members_count: Option<usize>,
    /// How many rows the latest posts/members fetch asked for; a full
    /// page back means a background top-up to the cap is worthwhile.
    posts_want: u32,
    members_want: u32,
    posts_loading: bool,
    members_loading: bool,
    /// What to do in the main window once this dialog has closed.
    pending: Option<Pending>,
    close_requested: bool,
}

enum Pending {
    View(View),
}

/// The view a member context-menu action should open, or None when the row is
/// missing the field that action needs (so the window stays open and says why).
fn member_view_action(id: i32, name: &str, url: &str) -> Option<Pending> {
    match id {
        ID_MEMBER_PROFILE if !url.is_empty() => {
            Some(Pending::View(View::ProfileOf(url.to_owned())))
        }
        ID_MEMBER_MESSAGE if !name.is_empty() => Some(Pending::View(View::ChatWith(
            name.to_owned(),
            name.to_owned(),
        ))),
        _ => None,
    }
}

impl GroupWindow {
    fn note(&self, text: &str) {
        self.status.change_value(text);
    }

    fn selected_member(&self) -> Option<Record> {
        let index = self.members.get_selection()? as usize;
        self.state.lock().unwrap().members.get(index).cloned()
    }

    fn member_action(&self, ui: &Ui, id: i32) {
        let Some(member) = self.selected_member() else {
            return;
        };
        let name = member.field(0).to_owned();
        let url = member.field(1).to_owned();
        match id {
            ID_MEMBER_PROFILE | ID_MEMBER_MESSAGE => match member_view_action(id, &name, &url) {
                Some(pending) => self.state.lock().unwrap().pending = Some(pending),
                // The row lacks the field this action needs; say so instead of
                // closing the window onto nothing.
                None if id == ID_MEMBER_PROFILE => {
                    self.note(&t("This member row has no profile link."))
                }
                None => self.note(&t("This member row has no name, so its chat cannot open.")),
            },
            ID_MEMBER_FRIEND => {
                if url.is_empty() {
                    self.note(&t(
                        "This member row has no profile link, so a friend request cannot be sent."
                    ));
                } else {
                    self.note(&tf!("Sending a friend request to {name}…", name = name));
                    ui.send(Job {
                        task: Task::Fb(friend_add_args(&url), None),
                        tag: Tag::Text,
                    });
                }
            }
            ID_MEMBER_WEB => {
                if url.is_empty() {
                    self.note(&t("This member row has no link to open."));
                } else {
                    open_url(&url);
                }
            }
            _ => {}
        }
        // The dialog closes itself from its menu handler (see open()), but only
        // when an action actually queued a view.
        if self.state.lock().unwrap().pending.is_some() {
            self.state.lock().unwrap().close_requested = true;
        }
    }
}

impl GroupState {
    fn take_pending(&mut self) -> Option<Pending> {
        self.pending.take()
    }
}

pub(super) fn open(ui: &Ui, record: &Record) {
    if ui.group_window.lock().unwrap().is_some() {
        ui.status(&t("A group window is already open. Close it to open another group."));
        return;
    }
    let name = record.field(0).to_owned();
    let url = record.field(1).to_owned();
    if url.is_empty() {
        ui.status(&t("This group row has no link, so its window cannot open."));
        return;
    }
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let dialog = Dialog::builder(&ui.frame, &tf!("Group: {name}", name = name))
        .with_size(900, 700)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let posts_label = StaticText::builder(&dialog).with_label(&t("&Posts")).build();
    let posts = ListBox::builder(&dialog).build();
    posts.set_name(&t("Posts in this group. Enter opens the selected post"));
    let members_label = StaticText::builder(&dialog).with_label(&t("&Members")).build();
    let members = ListBox::builder(&dialog).build();
    members.set_name(&t(
        "Members of this group. Applications or Shift+F10 for profile, message and friend actions"
    ));
    let status_label = StaticText::builder(&dialog)
        .with_label(&t("Group status"))
        .build();
    let status = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_name(&t("Group status"));
    status.enable_focus_from_keyboard();
    status.change_value(&t("Loading posts and members…"));
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    root.add(&posts_label, 0, SizerFlag::All, 4);
    root.add(&posts, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&members_label, 0, SizerFlag::All, 4);
    root.add(&members, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&status_label, 0, SizerFlag::All, 4);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&close, 0, SizerFlag::All, 4);
    dialog.set_sizer(root, true);

    let window = GroupWindow {
        session,
        posts,
        members,
        status,
        state: Arc::new(Mutex::new(GroupState {
            name: name.clone(),
            url: url.clone(),
            posts: Vec::new(),
            members: Vec::new(),
            posts_count: None,
            members_count: None,
            posts_want: POSTS_FIRST,
            members_want: MEMBERS_FIRST,
            posts_loading: true,
            members_loading: true,
            pending: None,
            close_requested: false,
        })),
    };
    *ui.group_window.lock().unwrap() = Some(window.clone());

    ui.send(Job {
        task: Task::Fb(posts_args(&url, POSTS_FIRST, true), None),
        tag: Tag::GroupPosts(session),
    });
    ui.send(Job {
        task: Task::Fb(members_args(&url, MEMBERS_FIRST, true), None),
        tag: Tag::GroupMembers(session),
    });

    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    posts.on_item_double_clicked(move |_| w.open_selected_post(&u, &d));
    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if matches!(key.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER)) {
                if w.posts.has_focus() {
                    event.skip(false);
                    w.open_selected_post(&u, &d);
                    return;
                }
                if w.members.has_focus() {
                    event.skip(false);
                    w.state.lock().unwrap().pending = w.selected_member().and_then(|m| {
                        let url = m.field(1).to_owned();
                        (!url.is_empty()).then_some(Pending::View(View::ProfileOf(url)))
                    });
                    if w.state.lock().unwrap().pending.is_some() {
                        d.end_modal(ID_OK);
                    } else if w.selected_member().is_some() {
                        w.note(&t("This member row has no profile link."));
                    }
                    return;
                }
            }
        }
        event.skip(true);
    });
    let w = window.clone();
    members.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        if w.selected_member().is_none() {
            return;
        }
        let mut menu = Menu::builder()
            .append_item(
                ID_MEMBER_PROFILE,
                &t("View &profile"),
                &t("Open this member's profile"),
            )
            .append_item(
                ID_MEMBER_MESSAGE,
                &t("&Message"),
                &t("Chat with this member"),
            )
            .append_item(
                ID_MEMBER_FRIEND,
                &t("Add &friend"),
                &t("Send this member a friend request"),
            )
            .append_item(
                ID_MEMBER_WEB,
                &t("Open on &Facebook"),
                &t("Open this member's profile on the web"),
            )
            .build();
        w.members.popup_menu(&mut menu, None);
    });
    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    dialog.bind_internal(EventType::MENU, move |event| {
        let id = event.get_id();
        if (ID_MEMBER_PROFILE..=ID_MEMBER_WEB).contains(&id) {
            event.skip(false);
            w.member_action(&u, id);
            if w.state.lock().unwrap().close_requested {
                d.end_modal(ID_OK);
            }
        } else {
            event.skip(true);
        }
    });
    close.on_click(move |_| dialog.end_modal(ID_CANCEL));
    let d = dialog;
    dialog.bind_internal(EventType::CLOSE_WINDOW, move |event| {
        event.skip(false);
        d.end_modal(ID_CANCEL);
    });

    posts.set_focus();
    dialog.show_modal();
    ui.group_window.lock().unwrap().take();
    dialog.destroy();
    if let Some(pending) = window.state.lock().unwrap().take_pending() {
        match pending {
            Pending::View(view) => load_view(ui, view),
        }
    }
    ui.items.set_focus();
}

impl GroupWindow {
    /// Opens the reader on top of this window. Rows carry relative links
    /// (`/groups/<id>/posts/<post>/`), so they go through `reader_url` like
    /// the main list's rows; the reader rejects anything else.
    fn open_selected_post(&self, ui: &Ui, dialog: &Dialog) {
        let Some(index) = self.posts.get_selection() else {
            return;
        };
        let target = {
            let state = self.state.lock().unwrap();
            state.posts.get(index as usize).and_then(|record| {
                super::post_target(&View::PostsOf(state.url.clone()), record).map(str::to_owned)
            })
        };
        let Some(target) = target else {
            self.note(&t("This post row has no link, so it cannot be opened."));
            return;
        };
        match blindfb_core::posts::reader_url(&target) {
            Some(url) => {
                if super::post_dialog::open_over(ui, &url) {
                    self.posts.set_focus();
                } else {
                    // The reader left for a profile; that view is loading behind us.
                    dialog.end_modal(ID_OK);
                }
            }
            None => {
                if confirm(
                    dialog,
                    t("BlindFB cannot read this kind of item yet (for example a photo album). Open it in your web browser?"),
                    &t("Open post"),
                ) {
                    open_url(&web_url(&target));
                }
            }
        }
    }
}

fn refresh_note(window: &GroupWindow) {
    let state = window.state.lock().unwrap();
    let posts = state
        .posts_count
        .map(|n| {
            if state.posts_loading {
                ntf!("{n} post so far", "{n} posts so far", n)
            } else {
                ntf!("{n} post", "{n} posts", n)
            }
        })
        .unwrap_or_else(|| t("posts loading"));
    let members = state
        .members_count
        .map(|n| {
            if n >= MEMBER_CAP as usize {
                tf!("first {cap} members (list capped)", cap = MEMBER_CAP)
            } else if state.members_loading {
                ntf!("{n} member so far", "{n} members so far", n)
            } else {
                ntf!("{n} member", "{n} members", n)
            }
        })
        .unwrap_or_else(|| t("members loading"));
    // TRANSLATORS: the group window status: group name, then the posts part
    // ("12 posts", "posts loading"…), then the members part
    window.note(&tf!(
        "{group}: {posts} · {members}.",
        group = state.name,
        posts = posts,
        members = members
    ));
}

fn member_label(record: &Record) -> String {
    let info = record.field(2);
    if info.is_empty() {
        record.field(0).to_owned()
    } else {
        format!("{} — {}", record.field(0), info)
    }
}

/// First fill replaces; a staged top-up (which re-reads from the top)
/// appends only rows not already shown, so the user's place in the
/// list never moves while the rest loads behind them.
fn fill_posts(window: &GroupWindow, raw: &str) {
    let records = parse_feed(raw);
    let first = window.state.lock().unwrap().posts.is_empty();
    if first {
        window.posts.clear();
        for record in &records {
            window
                .posts
                .append(&super::display_for(&View::Feed, record));
        }
        if !records.is_empty() {
            window.posts.set_selection(0, true);
        }
        let mut state = window.state.lock().unwrap();
        state.posts_count = Some(records.len());
        state.posts = records;
    } else {
        let known: std::collections::HashSet<String> = window
            .state
            .lock()
            .unwrap()
            .posts
            .iter()
            .map(|r| r.raw.clone())
            .collect();
        let mut fresh = Vec::new();
        for record in records {
            if !known.contains(&record.raw) {
                window
                    .posts
                    .append(&super::display_for(&View::Feed, &record));
                fresh.push(record);
            }
        }
        let mut state = window.state.lock().unwrap();
        state.posts.extend(fresh);
        state.posts_count = Some(state.posts.len());
    }
    refresh_note(window);
}

fn fill_members(window: &GroupWindow, raw: &str) {
    let records = parse_records(raw);
    let first = window.state.lock().unwrap().members.is_empty();
    if first {
        window.members.clear();
        for record in &records {
            window.members.append(&member_label(record));
        }
        if !records.is_empty() {
            window.members.set_selection(0, true);
        }
        let mut state = window.state.lock().unwrap();
        state.members_count = Some(records.len());
        state.members = records;
    } else {
        let known: std::collections::HashSet<String> = window
            .state
            .lock()
            .unwrap()
            .members
            .iter()
            .map(|r| r.raw.clone())
            .collect();
        let mut fresh = Vec::new();
        for record in records {
            if !known.contains(&record.raw) {
                window.members.append(&member_label(&record));
                fresh.push(record);
            }
        }
        let mut state = window.state.lock().unwrap();
        state.members.extend(fresh);
        state.members_count = Some(state.members.len());
    }
    refresh_note(window);
}

pub(super) fn deliver_posts(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.group_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(raw) => {
            let returned = parse_feed(&raw).len();
            fill_posts(&window, &raw);
            // A full first page means there may be more: top up to the
            // cap in the background, like the feed's read-ahead.
            let (want, url) = {
                let state = window.state.lock().unwrap();
                (state.posts_want, state.url.clone())
            };
            if returned >= want as usize && want < POSTS_CAP {
                window.state.lock().unwrap().posts_want = POSTS_CAP;
                ui.send(Job {
                    task: Task::Fb(posts_args(&url, POSTS_CAP, true), None),
                    tag: Tag::GroupPosts(session),
                });
            } else {
                window.state.lock().unwrap().posts_loading = false;
                refresh_note(&window);
            }
        }
        Err(e) => window.note(&tf!("Posts could not load: {error}", error = e)),
    }
}

pub(super) fn deliver_members(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.group_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(raw) => {
            let returned = parse_records(&raw).len();
            fill_members(&window, &raw);
            let (want, url) = {
                let state = window.state.lock().unwrap();
                (state.members_want, state.url.clone())
            };
            if returned >= want as usize && want < MEMBER_CAP {
                window.state.lock().unwrap().members_want = MEMBER_CAP;
                ui.send(Job {
                    task: Task::Fb(members_args(&url, MEMBER_CAP, true), None),
                    tag: Tag::GroupMembers(session),
                });
            } else {
                window.state.lock().unwrap().members_loading = false;
                refresh_note(&window);
            }
        }
        Err(e) => window.note(&tf!("Members could not load: {error}", error = e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_action_with_no_source_field_queues_nothing() {
        // A member row whose url column is absent must not close the window.
        assert!(member_view_action(ID_MEMBER_PROFILE, "Alice", "").is_none());
        assert!(member_view_action(ID_MEMBER_MESSAGE, "", "https://facebook.com/x").is_none());
        // With the needed field, the matching view is queued.
        assert!(matches!(
            member_view_action(ID_MEMBER_PROFILE, "Alice", "https://facebook.com/x"),
            Some(Pending::View(View::ProfileOf(_)))
        ));
        assert!(matches!(
            member_view_action(ID_MEMBER_MESSAGE, "Alice", ""),
            Some(Pending::View(View::ChatWith(_, _)))
        ));
    }
}
