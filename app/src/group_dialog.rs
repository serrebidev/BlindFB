//! The group window: a group's posts and its full member list in one
//! dialog. Opened by activating a group in the Groups list. Enter on a
//! post opens it in the post reader over this window, so closing the
//! reader lands back on the same post. Posts carry the main list's post
//! actions (react, comment, share, author, copy, web) on their context menu;
//! members carry the usual people actions (profile, message, friend
//! request, web).
use super::post_dialog::Over;
use super::{load_view, open_url, Job, Tag, Task, Ui, View, REACTIONS};
use blindfb_core::fb::{
    friend_add_args, group_photos_args, link_url, members_args, parse_feed, parse_records,
    posts_args, web_url, Record,
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
const ID_POST_OPEN: i32 = 12210;
const ID_POST_COMMENT: i32 = 12211;
const ID_POST_SHARE: i32 = 12212;
const ID_POST_PROFILE: i32 = 12213;
const ID_POST_AUTHOR_POSTS: i32 = 12214;
const ID_POST_MESSAGE: i32 = 12215;
const ID_POST_COPY: i32 = 12216;
const ID_POST_COPY_LINK: i32 = 12217;
const ID_POST_WEB: i32 = 12218;
const ID_POST_REACT: i32 = 12220; // + index into REACTIONS
const MEMBER_CAP: u32 = 500;
/// The window opens with a small first page per list and grows only by
/// manual "Load more", so photo-heavy groups stay fast to open.
const POSTS_FIRST: u32 = 10;
const POSTS_CAP: u32 = 50;
/// Load more re-reads from the top, so each step asks for this many more.
/// The backend scrolls posts for at most 90 s (~150 posts) and members for
/// at most 120 s (~2000).
const POSTS_STEP: u32 = 50;
const MEMBERS_STEP: u32 = 500;

/// The next "Load more" request size, or `None` once the cap is reached.
fn next_posts_want(want: u32) -> Option<u32> {
    if want >= POSTS_CAP {
        None
    } else {
        Some(want.saturating_add(POSTS_STEP).min(POSTS_CAP))
    }
}
fn next_members_want(want: u32) -> Option<u32> {
    if want >= MEMBER_CAP {
        None
    } else {
        Some(want.saturating_add(MEMBERS_STEP).min(MEMBER_CAP))
    }
}
fn next_photos_want(want: u32) -> Option<u32> {
    if want >= PHOTOS_CAP {
        None
    } else {
        Some(want.saturating_mul(2).min(PHOTOS_CAP))
    }
}

#[derive(Clone, Copy)]
enum List {
    Posts,
    Members,
    Photos,
}
const MEMBERS_FIRST: u32 = 50;
const PHOTOS_FIRST: u32 = 50;
const PHOTOS_CAP: u32 = 200;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct GroupWindow {
    session: u64,
    /// The dialog itself, so popups from the results timer can be parented
    /// to it while it is up.
    dialog: Dialog,
    posts: ListBox,
    members: ListBox,
    photos: ListBox,
    status: TextCtrl,
    state: Arc<Mutex<GroupState>>,
}

struct GroupState {
    name: String,
    url: String,
    posts: Vec<Record>,
    members: Vec<Record>,
    photos: Vec<Record>,
    posts_count: Option<usize>,
    members_count: Option<usize>,
    photos_count: Option<usize>,
    /// How many rows the latest posts/members/photos fetch asked for; a full
    /// page back means "Load more" may have more.
    posts_want: u32,
    members_want: u32,
    photos_want: u32,
    posts_loading: bool,
    members_loading: bool,
    photos_loading: bool,
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
    /// The dialog, so the frame's results timer can parent popups to it
    /// while it is up.
    pub(super) fn dialog(&self) -> Dialog {
        self.dialog
    }

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
    let photos_label = StaticText::builder(&dialog).with_label(&t("&Photos")).build();
    let photos = ListBox::builder(&dialog).build();
    photos.set_name(&t("Photos in this group. Enter opens the selected photo"));
    let status_label = StaticText::builder(&dialog)
        .with_label(&t("Group status"))
        .build();
    let status = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ReadOnly)
        .build();
    status.set_name(&t("Group status"));
    status.enable_focus_from_keyboard();
    status.change_value(&t("Loading posts and members…"));
    let new_post = Button::builder(&dialog).with_label(&t("&New post")).build();
    let copy_photo = Button::builder(&dialog).with_label(&t("Copy photo &link")).build();
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    root.add(&posts_label, 0, SizerFlag::All, 4);
    root.add(&posts, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&members_label, 0, SizerFlag::All, 4);
    root.add(&members, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&photos_label, 0, SizerFlag::All, 4);
    root.add(&photos, 2, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&status_label, 0, SizerFlag::All, 4);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&new_post, 0, SizerFlag::All, 4);
    root.add(&copy_photo, 0, SizerFlag::All, 4);
    root.add(&close, 0, SizerFlag::All, 4);
    dialog.set_sizer(root, true);

    let window = GroupWindow {
        session,
        dialog,
        posts,
        members,
        photos,
        status,
        state: Arc::new(Mutex::new(GroupState {
            name: name.clone(),
            url: url.clone(),
            posts: Vec::new(),
            members: Vec::new(),
            photos: Vec::new(),
            posts_count: None,
            members_count: None,
            photos_count: None,
            posts_want: POSTS_FIRST,
            members_want: MEMBERS_FIRST,
            photos_want: PHOTOS_FIRST,
            posts_loading: true,
            members_loading: true,
            photos_loading: true,
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
    ui.send(Job {
        task: Task::Fb(group_photos_args(&url, PHOTOS_FIRST), None),
        tag: Tag::GroupPhotos(session),
    });

    super::type_ahead::attach(&posts, |_| {});
    super::type_ahead::attach(&members, |_| {});
    super::type_ahead::attach(&photos, |_| {});
    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    posts.on_item_double_clicked(move |_| {
        if w.on_more_row(List::Posts) {
            w.load_more(&u, List::Posts);
        } else {
            w.open_selected_post(&u, &d, Over::Read);
        }
    });
    let (w, u) = (window.clone(), ui.clone());
    members.on_item_double_clicked(move |_| {
        if w.on_more_row(List::Members) {
            w.load_more(&u, List::Members);
        }
    });
    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    photos.on_item_double_clicked(move |_| {
        if w.on_more_row(List::Photos) {
            w.load_more(&u, List::Photos);
        } else {
            w.open_selected_photo(&u, &d);
        }
    });
    let (w, u) = (window.clone(), ui.clone());
    let d = dialog;
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if matches!(key.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER)) {
                if w.posts.has_focus() {
                    event.skip(false);
                    if w.on_more_row(List::Posts) {
                        w.load_more(&u, List::Posts);
                    } else {
                        w.open_selected_post(&u, &d, Over::Read);
                    }
                    return;
                }
                if w.members.has_focus() && w.on_more_row(List::Members) {
                    event.skip(false);
                    w.load_more(&u, List::Members);
                    return;
                }
                if w.photos.has_focus() {
                    event.skip(false);
                    if w.on_more_row(List::Photos) {
                        w.load_more(&u, List::Photos);
                    } else {
                        w.open_selected_photo(&u, &d);
                    }
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
    // The main list's post menu, same labels, acting on this window's row.
    let w = window.clone();
    posts.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        let Some(record) = w.selected_post() else {
            return;
        };
        let has_author = record.author_ref.as_deref().is_some_and(|r| !r.is_empty())
            || !record.field(1).is_empty();
        let mut menu = Menu::builder()
            .append_item(ID_POST_OPEN, &t("&Open post and comments"), "")
            .append_item(ID_POST_SHARE, &t("S&hare…"), "")
            .append_item(ID_POST_COMMENT, &t("&Comment…"), "")
            .build();
        let current = record.reaction.as_deref();
        let react = Menu::builder().build();
        for (i, reaction) in REACTIONS.iter().enumerate() {
            let label = super::reaction_label(reaction);
            react.append(ID_POST_REACT + i as i32, &label, "", ItemKind::Check);
            react.check_item(ID_POST_REACT + i as i32, *reaction != "none" && current == Some(*reaction));
        }
        menu.append_submenu(react, &super::react_submenu_title(current), "");
        menu.append_separator();
        menu.append(ID_POST_PROFILE, &t("Author &profile"), "", ItemKind::Normal);
        menu.append(ID_POST_AUTHOR_POSTS, &t("Author's p&osts"), "", ItemKind::Normal);
        menu.append(ID_POST_MESSAGE, &t("&Message author"), "", ItemKind::Normal);
        menu.append_separator();
        menu.append(ID_POST_COPY, &t("Cop&y text"), "", ItemKind::Normal);
        menu.append(ID_POST_COPY_LINK, &t("Copy &link"), "", ItemKind::Normal);
        menu.append(ID_POST_WEB, &t("Open post on &Facebook (web)"), "", ItemKind::Normal);
        for id in [ID_POST_PROFILE, ID_POST_AUTHOR_POSTS, ID_POST_MESSAGE] {
            menu.enable_item(id, has_author);
        }
        w.posts.popup_menu(&mut menu, None);
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
        } else if (ID_POST_OPEN..ID_POST_REACT + REACTIONS.len() as i32).contains(&id) {
            event.skip(false);
            w.post_action(&u, &d, id);
        } else {
            event.skip(true);
        }
    });
    let (w, u) = (window.clone(), ui.clone());
    new_post.on_click(move |_| {
        let (name, url) = {
            let state = w.state.lock().unwrap();
            (state.name.clone(), state.url.clone())
        };
        let Some(text) = super::prompt_long_text(&dialog, t("Post text:"), &t("New post")) else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        let Some(anonymous) = super::choose_group_post_identity(&dialog, &name) else { return };
        u.send(Job {
            task: Task::Fb(blindfb_core::fb::group_post_args(&text, &url, &[], &[], anonymous), None),
            tag: Tag::GroupPost(name),
        });
    });
    let w = window.clone();
    copy_photo.on_click(move |_| {
        match w.selected_photo().and_then(|record| link_url(record.field(1))) {
            Some(link) if crate::copy_to_clipboard(&link) => w.note(&t("Link copied.")),
            Some(_) => {}
            None => w.note(&t("This item has no link to copy.")),
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
    fn list(&self, which: List) -> ListBox {
        match which {
            List::Posts => self.posts,
            List::Members => self.members,
            List::Photos => self.photos,
        }
    }

    /// Rows shown for `which`, not counting its "Load more" row.
    fn rows(&self, which: List) -> usize {
        let state = self.state.lock().unwrap();
        match which {
            List::Posts => state.posts.len(),
            List::Members => state.members.len(),
            List::Photos => state.photos.len(),
        }
    }

    /// Whether the selected row of `which` is its trailing "Load more" row.
    fn on_more_row(&self, which: List) -> bool {
        let list = self.list(which);
        let rows = self.rows(which);
        list.get_selection().is_some_and(|i| i as usize == rows)
            && list.get_count() as usize > rows
    }

    /// Show or drop the trailing "Load more" row.
    fn show_more(&self, which: List, show: bool) {
        let (list, rows) = (self.list(which), self.rows(which));
        if list.get_count() as usize > rows {
            if list.get_selection().is_some_and(|i| i as usize == rows) && rows > 0 {
                list.set_selection(rows as u32 - 1, true);
            }
            list.delete(rows as u32);
        }
        if show {
            list.append(&match which {
                List::Posts => t("Load more posts"),
                List::Members => t("Load more members"),
                List::Photos => t("Load more photos"),
            });
        }
    }

    /// Ask for the next step of rows; the answer appends only new ones.
    fn load_more(&self, ui: &Ui, which: List) {
        let args = {
            let mut state = self.state.lock().unwrap();
            let url = state.url.clone();
            match which {
                List::Posts if !state.posts_loading => {
                    if let Some(next) = next_posts_want(state.posts_want) {
                        state.posts_loading = true;
                        state.posts_want = next;
                        let mut args = posts_args(&url, state.posts_want, true);
                        args.push("--continue".to_owned());
                        Some((args, Tag::GroupPosts(self.session)))
                    } else {
                        None
                    }
                }
                List::Members if !state.members_loading => {
                    if let Some(next) = next_members_want(state.members_want) {
                        state.members_loading = true;
                        state.members_want = next;
                        Some((
                            members_args(&url, state.members_want, true),
                            Tag::GroupMembers(self.session),
                        ))
                    } else {
                        None
                    }
                }
                List::Photos if !state.photos_loading => {
                    if let Some(next) = next_photos_want(state.photos_want) {
                        state.photos_loading = true;
                        state.photos_want = next;
                        Some((
                            group_photos_args(&url, state.photos_want),
                            Tag::GroupPhotos(self.session),
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        let Some((args, tag)) = args else {
            self.note(&t("Items are already loading…"));
            return;
        };
        self.show_more(which, false);
        refresh_note(self);
        ui.send(Job {
            task: Task::Fb(args, None),
            tag,
        });
    }

    fn selected_post(&self) -> Option<Record> {
        let index = self.posts.get_selection()? as usize;
        self.state.lock().unwrap().posts.get(index).cloned()
    }

    fn selected_photo(&self) -> Option<Record> {
        let index = self.photos.get_selection()? as usize;
        self.state.lock().unwrap().photos.get(index).cloned()
    }

    /// Opens the reader on the selected group's photo. Photo rows carry a
    /// full URL, unlike posts' relative links.
    fn open_selected_photo(&self, ui: &Ui, dialog: &Dialog) {
        let Some(record) = self.selected_photo() else {
            return;
        };
        let url = record.field(1).trim().to_owned();
        if url.is_empty() {
            self.note(&t("This photo row has no link, so it cannot be opened."));
            return;
        }
        if super::post_dialog::open_over(ui, &url, Over::Read) {
            self.photos.set_focus();
        } else {
            dialog.end_modal(ID_OK);
        }
    }

    /// The selected row's post link (relative, `/groups/<id>/posts/<post>/`).
    fn selected_target(&self) -> Option<String> {
        let record = self.selected_post()?;
        let url = self.state.lock().unwrap().url.clone();
        super::post_target(&View::PostsOf(url), &record).map(str::to_owned)
    }

    /// Opens the reader on top of this window, doing `what` first. Rows carry
    /// relative links, so they go through `reader_url` like the main list's
    /// rows; the reader rejects anything else.
    fn open_selected_post(&self, ui: &Ui, dialog: &Dialog, what: Over) {
        if self.selected_post().is_none() {
            return;
        }
        let Some(target) = self.selected_target() else {
            self.note(&t("This post row has no link, so it cannot be opened."));
            return;
        };
        match blindfb_core::posts::reader_url(&target) {
            Some(url) => {
                if super::post_dialog::open_over(ui, &url, what) {
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

    /// A post context-menu command. Reader actions open the reader over this
    /// window; author views close it and open in the main window.
    fn post_action(&self, ui: &Ui, dialog: &Dialog, id: i32) {
        let Some(record) = self.selected_post() else {
            return;
        };
        // The author line can carry badges ("Name / Admin"): the ref is the
        // identity, the first name the fallback (see main.rs person_of).
        let name = super::poster_name(record.field(1)).to_owned();
        let who = record.author_ref.clone().filter(|r| !r.is_empty());
        let view = match id {
            ID_POST_OPEN => return self.open_selected_post(ui, dialog, Over::Read),
            ID_POST_SHARE => return self.open_selected_post(ui, dialog, Over::Share),
            ID_POST_COMMENT => return self.open_selected_post(ui, dialog, Over::Comment),
            ID_POST_PROFILE | ID_POST_AUTHOR_POSTS => {
                who.or_else(|| (!name.is_empty()).then(|| name.clone())).map(|who| {
                    if id == ID_POST_PROFILE {
                        View::ProfileOf(who)
                    } else {
                        View::PostsOf(who)
                    }
                })
            }
            ID_POST_MESSAGE => {
                (!name.is_empty()).then(|| View::ChatWith(who.unwrap_or(name.clone()), name))
            }
            ID_POST_COPY => {
                let text = super::display_for(&View::Feed, &record);
                if crate::copy_to_clipboard(&text) {
                    self.note(&t("Text copied."));
                }
                return;
            }
            ID_POST_COPY_LINK => {
                match self.selected_target().as_deref().and_then(link_url) {
                    Some(link) if crate::copy_to_clipboard(&link) => self.note(&t("Link copied.")),
                    Some(_) => {}
                    None => self.note(&t("This item has no link to copy.")),
                }
                return;
            }
            ID_POST_WEB => {
                match self.selected_target() {
                    Some(target) => open_url(&web_url(&target)),
                    None => self.note(&t("This post row has no link, so it cannot be opened.")),
                }
                return;
            }
            _ => {
                let index = (id - ID_POST_REACT) as usize;
                if let Some(reaction) = REACTIONS.get(index) {
                    self.open_selected_post(ui, dialog, Over::React(reaction));
                }
                return;
            }
        };
        // No author: the menu disables these items, so nothing to say here.
        if let Some(view) = view {
            self.state.lock().unwrap().pending = Some(Pending::View(view));
            dialog.end_modal(ID_OK);
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
            if state.members_loading {
                ntf!("{n} member so far", "{n} members so far", n)
            } else {
                ntf!("{n} member", "{n} members", n)
            }
        })
        .unwrap_or_else(|| t("members loading"));
    let photos = state
        .photos_count
        .map(|n| {
            if state.photos_loading {
                ntf!("{n} photo so far", "{n} photos so far", n)
            } else {
                ntf!("{n} photo", "{n} photos", n)
            }
        })
        .unwrap_or_else(|| t("photos loading"));
    // TRANSLATORS: the group window status: group name, then the posts part
    // ("12 posts", "posts loading"…), then the members part, then the photos part
    window.note(&tf!(
        "{group}: {posts} · {members} · {photos}.",
        group = state.name,
        posts = posts,
        members = members,
        photos = photos
    ));
}

fn photo_label(record: &Record) -> String {
    let info = record.field(2);
    if info.is_empty() {
        record.field(0).to_owned()
    } else {
        format!("{} — {}", record.field(0), info)
    }
}

fn fill_photos(window: &GroupWindow, raw: &str) {
    let records = parse_records(raw);
    let first = window.state.lock().unwrap().photos.is_empty();
    if first {
        window.photos.clear();
        for record in &records {
            window.photos.append(&photo_label(record));
        }
        if !records.is_empty() {
            window.photos.set_selection(0, true);
        }
        let mut state = window.state.lock().unwrap();
        state.photos_count = Some(records.len());
        state.photos = records;
    } else {
        let known: std::collections::HashSet<String> = window
            .state
            .lock()
            .unwrap()
            .photos
            .iter()
            .map(|r| r.raw.clone())
            .collect();
        let mut fresh = Vec::new();
        for record in records {
            if !known.contains(&record.raw) {
                window.photos.append(&photo_label(&record));
                fresh.push(record);
            }
        }
        let mut state = window.state.lock().unwrap();
        state.photos.extend(fresh);
        state.photos_count = Some(state.photos.len());
    }
    refresh_note(window);
}

pub(super) fn deliver_photos(ui: &Ui, session: u64, result: Result<String, String>) {
    let window = ui.group_window.lock().unwrap().clone();
    let Some(window) = window.filter(|w| w.session == session) else {
        return;
    };
    match result {
        Ok(raw) => {
            let returned = parse_records(&raw).len();
            fill_photos(&window, &raw);
            let want = window.state.lock().unwrap().photos_want;
            let _ = ui;
            window.state.lock().unwrap().photos_loading = false;
            window
                .show_more(List::Photos, returned >= want as usize && want < PHOTOS_CAP);
            refresh_note(&window);
        }
        Err(e) => {
            window.state.lock().unwrap().photos_loading = false;
            let shown = !window.state.lock().unwrap().photos.is_empty();
            window.show_more(List::Photos, shown);
            window.note(&tf!("Photos could not load: {error}", error = e));
        }
    }
}

fn member_label(record: &Record) -> String {
    let info = record.field(2);
    if info.is_empty() {
        record.field(0).to_owned()
    } else {
        format!("{} — {}", record.field(0), info)
    }
}

/// First fill replaces; a manual "Load more" (which re-reads from the top)
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
        window.show_more(List::Posts, false);
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
        window.show_more(List::Members, false);
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
            fill_posts(&window, &raw);
            let want = window.state.lock().unwrap().posts_want;
            let _ = ui;
            window.state.lock().unwrap().posts_loading = false;
            // A short scan can still have further posts on Facebook.
            window.show_more(List::Posts, want < POSTS_CAP);
            refresh_note(&window);
        }
        Err(e) => {
            window.state.lock().unwrap().posts_loading = false;
            let want = window.state.lock().unwrap().posts_want;
            window.show_more(List::Posts, want < POSTS_CAP);
            window.note(&tf!("Posts could not load: {error}", error = e));
        }
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
            let want = window.state.lock().unwrap().members_want;
            let _ = ui;
            window.state.lock().unwrap().members_loading = false;
            window
                .show_more(List::Members, returned >= want as usize && want < MEMBER_CAP);
            refresh_note(&window);
        }
        Err(e) => {
            window.state.lock().unwrap().members_loading = false;
            let shown = !window.state.lock().unwrap().members.is_empty();
            window.show_more(List::Members, shown);
            window.note(&tf!("Members could not load: {error}", error = e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_more_steps_stop_at_the_caps() {
        assert_eq!(next_posts_want(POSTS_FIRST), Some(POSTS_CAP));
        assert_eq!(next_posts_want(POSTS_CAP), None);
        assert_eq!(next_members_want(MEMBERS_FIRST), Some(MEMBER_CAP));
        assert_eq!(next_members_want(MEMBER_CAP), None);
        assert_eq!(next_photos_want(PHOTOS_FIRST), Some(100));
        assert_eq!(next_photos_want(100), Some(PHOTOS_CAP));
        assert_eq!(next_photos_want(PHOTOS_CAP), None);
    }

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
