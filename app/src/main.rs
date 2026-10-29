//! BlindFB: an accessible Facebook and Messenger client for screen reader
//! users, built with wxDragon (native wxWidgets controls throughout, so
//! lists, menus and dialogs are the real platform widgets) on top of the
//! `fb` command-line backend (hieu-lee/facebook-cli).
//!
//! Everything Facebook-related goes through `blindfb_core::fb`. Commands run
//! on one worker thread; results come back over a channel that a timer
//! drains, so the UI never blocks on the network or on Chrome.
//!
//! Not supported in-app, because the backend has no such commands: voice
//! and video calls, adding/removing friends, and posting into a group. For
//! those, the menus link out to the exact person, post, group or Messenger
//! thread on the web. Groups you belong to are listed through the backend's
//! named-page browser primitives, and a group's posts read and behave like
//! any other posts here.

mod post_dialog;
mod reel_cache;
mod update;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use blindfb_core::browsers::{self, Browser, BrowserKind};
use blindfb_core::posts::Link;
use blindfb_core::reels::{self, ReelSource};
use blindfb_core::fb::{
    Fb, Record, chats_args, comment_args, delete_args, feed_args, friends_args, groups_eval_args,
    groups_open_args, notifs_args, parse_feed, parse_groups_json, parse_notifications,
    parse_records, post_args, posts_args, profile_args, profile_url_from_post_url, react_args,
    read_args, recent_feed_args, search_args, send_args, thread_url_from_profile_url, unsend_args,
    view_args, web_url,
};
use wx_utils::{AboutBoxBuilder, confirm, prompt_text, show_error, show_warning};
use wxdragon::prelude::*;

const ID_FEED: i32 = 10001;
const ID_NOTIFS: i32 = 10002;
const ID_FRIENDS: i32 = 10003;
const ID_MYPOSTS: i32 = 10004;
const ID_CHATS: i32 = 10005;
const ID_REFRESH: i32 = 10006;
const ID_UNREAD_CHATS: i32 = 10007;
const ID_GROUPS_VIEW: i32 = 10008;
const ID_RECENT_FEED: i32 = 10009;
const ID_MORE: i32 = 10016;
const ID_REELS: i32 = 10017;
const ID_WEB_POST: i32 = 10080;
const ID_WEB_PERSON: i32 = 10081;
const ID_WEB_CHAT: i32 = 10082;
const ID_NEW_POST: i32 = 10010;
const ID_NEW_POST_IMAGES: i32 = 10011;
const ID_SEARCH: i32 = 10012;
const ID_OCR: i32 = 10013;
const ID_UPDATE: i32 = 10014;
const ID_EXIT: i32 = 10015;
const ID_OPEN: i32 = 10020;
const ID_OPEN_LINK: i32 = 10021;
const ID_KEYS: i32 = 10072;
const ID_REACT_BASE: i32 = 10030; // + index into REACTIONS
const ID_COMMENT: i32 = 10040;
const ID_PROFILE: i32 = 10041;
const ID_AUTHOR_POSTS: i32 = 10042;
const ID_MESSAGE: i32 = 10043;
const ID_DELETE: i32 = 10044;
const ID_COPY_TEXT: i32 = 10045;
const ID_COPY_LINK: i32 = 10046;
const ID_SHARE: i32 = 10047;
const ID_UNSEND: i32 = 10050;
const ID_UNLOCK: i32 = 10051;
const ID_STATUS: i32 = 10060;
const ID_LOGIN: i32 = 10061;
const ID_INSTALL_BACKEND: i32 = 10083;
const ID_INSTALL_BROWSER: i32 = 10084;
const ID_STOP_DAEMON: i32 = 10062;
const ID_QUIT_DAEMON: i32 = 10063;
const ID_ABOUT: i32 = 10070;
const ID_GITHUB: i32 = 10071;

const REACTIONS: [&str; 8] = ["like", "love", "care", "haha", "wow", "sad", "angry", "none"];
const PAGE: u32 = 40;
/// The News feed opens with this many posts, then loads a full page while the user reads.
const FEED_FIRST: u32 = 10;
/// A cached section younger than this is shown without asking Facebook again.
const FRESH: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum View {
    Feed,
    RecentFeed,
    Reels(ReelSource),
    Notifications,
    Friends,
    MyPosts,
    Chats,
    Groups,
    SearchResults,
    PostsOf(String),
    CommentsOf(String),
    ProfileOf(String),
    ChatWith(String),
}

impl View {
    fn title(&self) -> String {
        match self {
            View::Feed => "News feed".to_owned(),
            View::RecentFeed => "Feeds (most recent)".to_owned(),
            View::Reels(source) => source.title(),
            View::Notifications => "Notifications".to_owned(),
            View::Friends => "Friends".to_owned(),
            View::MyPosts => "My posts".to_owned(),
            View::Chats => "Chats".to_owned(),
            View::Groups => "My groups".to_owned(),
            View::SearchResults => "Search results".to_owned(),
            View::PostsOf(who) => format!("Posts by {who}"),
            View::CommentsOf(_) => "Post and comments".to_owned(),
            View::ProfileOf(who) => format!("Profile of {who}"),
            View::ChatWith(name) => format!("Chat with {name}"),
        }
    }
}

enum Tag {
    List {
        view: View,
        request: u64,
        args: Vec<String>,
        unlock_attempted: bool,
        append: bool,
    },
    Text,
    ReloadChat(String, u64),
    ChatSent(u64),
    GroupsOpened(u64),
    UnlockRetry {
        view: View,
        request: u64,
        args: Vec<String>,
        append: bool,
    },
    PostLoaded(u64),
    ReelPrepared { account: u64, target: String },
    PostVideo(u64, u64),
    PostReelReacted(u64),
    PostSent(u64),
    PostLiked(u64),
    PostShareReady(u64),
    PostShared(u64),
    PostNote(u64),
    PostShareClosed(u64),
    Silent,
    SetupStatus,
    SetupInstall,
    SetupLogin,
    BrowserInstalled,
    BrowserInstalledMenu,
}

/// How to sign in: copy the session from one of the user's browsers, or
/// sign in directly in a new window of the backend's own browser.
#[derive(Clone)]
enum SignIn {
    Browser(Browser),
    Visible,
    /// No usable browser exists: install this one first.
    Install(BrowserKind),
}

enum Task {
    ReelsLoad { source: ReelSource, append: bool },
    /// Run the backend with these args (and optional stdin, for the PIN).
    Fb(Vec<String>, Option<String>),
    /// Download and install/update the managed backend binary.
    InstallBackend,
    /// Guided browser sign-in: import the session from the chosen browser,
    /// fall back to a visible browser window, then report the status.
    LoginFlow(SignIn),
    /// Download and install a missing browser (also covers the backend's
    /// own need for a Chrome/Chromium to drive).
    InstallBrowser(BrowserKind),
    PostLoad {
        page: String,
        target: String,
    },
    PostVideo {
        page: String,
        target: String,
    },
    PostReelReact {
        page: String,
        reaction: String,
    },
    PostRefresh {
        page: String,
    },
    PostSend {
        page: String,
        parent: Option<String>,
        text: String,
    },
    PostLike {
        page: String,
        id: String,
    },
    PostShareOpen {
        page: String,
    },
    PostShare {
        page: String,
        text: String,
    },
    PostShareCancel {
        page: String,
    },
    PostClose {
        page: String,
    },
}

struct Job {
    task: Task,
    tag: Tag,
}

struct Done {
    tag: Tag,
    result: Result<String, String>,
}

struct State {
    view: View,
    records_view: View,
    last_args: Vec<String>,
    pending_unlock: Option<RetryList>,
    unlocking: bool,
    records: Vec<Record>,
    attachments: Vec<String>,
    sending: Option<ChatSubmission>,
    account: u64,
    reel_cache: reel_cache::ReelCache,
    /// Author name -> profile URL, learned from post URLs as views load.
    /// Powers the "open on Facebook" link-outs (profiles, Messenger web).
    author_urls: HashMap<String, String>,
    /// Last parsed backend output and request per section, so switching back is instant.
    cache: HashMap<View, (Vec<Record>, Instant, Vec<String>)>,
    prefetched: bool,
    /// The list request refreshing a cached section the user is already reading.
    background: Option<u64>,
}

#[derive(Default)]
struct ListRequests {
    next: u64,
    latest: HashMap<View, u64>,
    pending: HashSet<View>,
}

impl ListRequests {
    fn begin(&mut self, view: &View) -> u64 {
        self.next = self.next.wrapping_add(1);
        self.latest.insert(view.clone(), self.next);
        self.pending.insert(view.clone());
        self.next
    }

    fn current(&self, view: &View, request: u64) -> bool {
        self.latest.get(view) == Some(&request)
    }

    fn finish(&mut self, view: &View, request: u64) -> bool {
        if !self.current(view, request) {
            return false;
        }
        self.pending.remove(view);
        true
    }

    fn invalidate(&mut self) {
        self.latest.clear();
        self.pending.clear();
    }
}

struct RetryList {
    view: View,
    request: u64,
    args: Vec<String>,
    append: bool,
}

#[derive(Clone)]
struct ChatSubmission {
    account: u64,
    chat: String,
    text: String,
    files: Vec<String>,
}

impl ChatSubmission {
    fn begin(pending: &mut Option<Self>, message: Self) -> bool {
        if pending.is_some() {
            return false;
        }
        *pending = Some(message);
        true
    }

    fn clear_sent(&self, view: &View, text: &str, files: &mut Vec<String>) -> bool {
        // Files attached after Send belong to the next draft.
        if files.starts_with(&self.files) {
            files.drain(..self.files.len());
        }
        *view == View::ChatWith(self.chat.clone()) && text == self.text
    }

    fn finish(pending: &mut Option<Self>, account: u64) -> Option<Self> {
        if pending.as_ref().is_some_and(|message| message.account == account) {
            pending.take()
        } else {
            None
        }
    }
}

/// Append one line to startup.log in BlindFB's data folder. Support
/// lifeline: when the window never appears, this log says how far startup
/// got, and a blind user can read it and report the last line.
fn startup_log(line: &str) {
    let Some(dir) = blindfb_core::backend::data_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) =
        std::fs::OpenOptions::new().create(true).append(true).open(dir.join("startup.log"))
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

fn main() {
    startup_log("main: starting");
    std::panic::set_hook(Box::new(|info| {
        startup_log(&format!("panic: {info}"));
    }));
    let _ = wxdragon::main(|_| {
        run();
    });
    if let Some(fb) = Fb::discover() { reels::close_all(&fb); }
    startup_log("main: exited");
}

fn run() {
    startup_log("run: building the window");
    let fb = Fb::discover();
    let (job_tx, job_rx) = channel::<Job>();
    let (done_tx, done_rx) = channel::<Done>();
    let latest_list = Arc::new(Mutex::new(ListRequests::default()));
    spawn_worker(fb.clone(), job_rx, done_tx.clone(), latest_list.clone());

    let frame = Frame::builder().with_title("BlindFB").with_size(Size::new(1100, 760)).build();
    frame.create_status_bar(1, 0, -1, "status");
    frame.set_status_text(
        if fb.is_some() {
            "Ready. Pick a view to start."
        } else {
            "Setting up the Facebook backend…"
        },
        0,
    );

    let panel = Panel::builder(&frame).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let views_label = StaticText::builder(&panel).with_label("Views").build();
    let views = ListBox::builder(&panel).build();
    for name in [
        "News feed",
        "Notifications",
        "Friends",
        "My posts",
        "Chats",
        "Groups",
        "Feeds (most recent)",
            "Reels",
    ] {
        views.append(name);
    }
    views.set_selection(0, true);

    let items_label = StaticText::builder(&panel).with_label("Items").build();
    let items = ListBox::builder(&panel).with_size(Size::new(-1, 260)).build();

    let links_label = StaticText::builder(&panel).with_label("Lin&ks in selected item").build();
    let links = ListBox::builder(&panel).with_size(Size::new(-1, 70)).build();
    links.set_name("Links in selected item. Enter opens the selected link in your browser");
    let open_link = Button::builder(&panel).with_label("Open selected &link").build();
    open_link.enable(false);

    let details_label = StaticText::builder(&panel).with_label("Details").build();
    let details = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .with_size(Size::new(-1, 170))
        .build();

    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    let refresh_btn = Button::builder(&panel).with_label("Refresh (F5)").build();
    let more_btn = Button::builder(&panel).with_label("Load more (Ctrl+L)").build();
    let open_btn = Button::builder(&panel).with_label("Open / read full post and links").build();
    let react_btn = Button::builder(&panel).with_label("React").build();
    let comment_btn = Button::builder(&panel).with_label("Comment").build();
    let profile_btn = Button::builder(&panel).with_label("Profile").build();
    let posts_btn = Button::builder(&panel).with_label("Posts").build();
    let message_btn = Button::builder(&panel).with_label("Message").build();
    let delete_btn = Button::builder(&panel).with_label("Delete post").build();
    for b in [
        &refresh_btn,
        &more_btn,
        &open_btn,
        &react_btn,
        &comment_btn,
        &profile_btn,
        &posts_btn,
        &message_btn,
        &delete_btn,
    ] {
        actions.add(b, 0, SizerFlag::All, 3);
    }

    let compose = BoxSizer::builder(Orientation::Horizontal).build();
    let compose_label = StaticText::builder(&panel).with_label("Message").build();
    let compose_box = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
    let attach_btn = Button::builder(&panel).with_label("Attach files").build();
    let send_btn = Button::builder(&panel).with_label("Send").build();
    compose.add(&compose_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 3);
    compose.add(&compose_box, 1, SizerFlag::Expand | SizerFlag::All, 3);
    compose.add(&attach_btn, 0, SizerFlag::All, 3);
    compose.add(&send_btn, 0, SizerFlag::All, 3);

    root.add(&views_label, 0, SizerFlag::All, 4);
    root.add(&views, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 4);
    root.add(&items_label, 0, SizerFlag::All, 4);
    root.add(&items, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 4);
    root.add(&links_label, 0, SizerFlag::All, 4);
    root.add(&links, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 4);
    root.add(&open_link, 0, SizerFlag::All, 4);
    root.add(&details_label, 0, SizerFlag::All, 4);
    root.add(&details, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 4);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 2);
    root.add_sizer(&compose, 0, SizerFlag::Expand, 2);
    panel.set_sizer(root, true);

    build_menus(&frame);
    startup_log("run: menus built");

    let state = Rc::new(RefCell::new(State {
        view: View::Feed,
        records_view: View::Feed,
        last_args: Vec::new(),
        pending_unlock: None,
        unlocking: false,
        records: Vec::new(),
        attachments: Vec::new(),
        sending: None,
        account: 0,
        reel_cache: reel_cache::ReelCache::default(),
        author_urls: HashMap::new(),
        cache: HashMap::new(),
        prefetched: false,
        background: None,
    }));
    let ui = Ui {
        frame,
        items,
        details,
        links,
        open_link,
        link_targets: Rc::new(RefCell::new(Vec::new())),
        compose_box,
        state: state.clone(),
        jobs: job_tx.clone(),
        done_tx,
        post_reader: Rc::new(RefCell::new(None)),
        latest_list,
    };

    // First-run setup: check the backend status. If the backend is missing
    // entirely, the SetupStatus result sends the installer; if it is there
    // but signed out, setup offers the browser sign-in. Signed in already:
    // straight to the feed.
    ui.status("Checking the Facebook backend…");
    ui.send(Job { task: Task::Fb(vec!["status".into()], None), tag: Tag::SetupStatus });

    let ui_views = ui.clone();
    views.on_selection_changed(move |event| {
        if let Some(sel) = event.get_selection() {
            let view = match sel {
                0 => View::Feed,
                1 => View::Notifications,
                2 => View::Friends,
                3 => View::MyPosts,
                5 => View::Groups,
                6 => View::RecentFeed,
                7 => View::Reels(ReelSource::Recommended),
                _ => View::Chats,
            };
            load_view(&ui_views, view);
        }
    });

    let ui_sel = ui.clone();
    items.on_selection_changed(move |event| {
        if let Some(sel) = event.get_selection() {
            show_details(&ui_sel, sel as usize);
            prepare_reels(&ui_sel);
        }
    });
    let ui_links = ui.clone();
    links.on_selection_changed(move |_| {
        ui_links.open_link.enable(ui_links.links.get_selection().is_some());
    });
    let ui_links = ui.clone();
    open_link.on_click(move |_| open_detail_link(&ui_links));
    let ui_links = ui.clone();
    links.on_item_double_clicked(move |_| open_detail_link(&ui_links));
    let ui_open = ui.clone();
    items.on_item_double_clicked(move |_| open_selected(&ui_open));
    let ui_context = ui.clone();
    items.bind_internal(EventType::CONTEXT_MENU, move |event| {
        event.skip(false);
        show_item_menu(&ui_context);
    });
    let ui_enter = ui.clone();
    // Char hook, not the list's key-down: Windows dialog navigation eats Enter before the list sees it.
    frame.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if ui_enter.links.has_focus()
                && matches!(
                    key.get_key_code(),
                    Some(wxdragon::keycode::WXK_RETURN | wxdragon::keycode::WXK_NUMPAD_ENTER)
                )
            {
                event.skip(false);
                open_detail_link(&ui_enter);
                return;
            }
            if ui_enter.items.has_focus()
                && matches!(
                    key.get_key_code(),
                    Some(wxdragon::keycode::WXK_RETURN | wxdragon::keycode::WXK_NUMPAD_ENTER)
                )
            {
                event.skip(false);
                open_selected(&ui_enter);
                return;
            }
        }
        event.skip(true);
    });

    let ui_refresh = ui.clone();
    refresh_btn.on_click(move |_| refresh(&ui_refresh));
    let ui_more = ui.clone();
    more_btn.on_click(move |_| load_more(&ui_more));
    let ui_open2 = ui.clone();
    open_btn.on_click(move |_| open_selected(&ui_open2));
    let ui_react = ui.clone();
    react_btn.on_click(move |_| react_popup(&ui_react));
    let ui_comment = ui.clone();
    comment_btn.on_click(move |_| comment_selected(&ui_comment));
    let ui_profile = ui.clone();
    profile_btn.on_click(move |_| profile_selected(&ui_profile));
    let ui_posts = ui.clone();
    posts_btn.on_click(move |_| posts_selected(&ui_posts));
    let ui_message = ui.clone();
    message_btn.on_click(move |_| message_selected(&ui_message));
    let ui_delete = ui.clone();
    delete_btn.on_click(move |_| delete_selected(&ui_delete));
    let ui_attach = ui.clone();
    attach_btn.on_click(move |_| attach_files(&ui_attach));
    let ui_send = ui.clone();
    send_btn.on_click(move |_| send_composed(&ui_send));
    let ui_send2 = ui.clone();
    compose_box.on_text_enter(move |_| send_composed(&ui_send2));

    let ui_menu = ui.clone();
    frame.on_menu(move |event| handle_menu(&ui_menu, event.get_id()));

    // The timer polls the results channel. It must live as long as the app,
    // so it is intentionally leaked rather than dropped with this stack frame.
    let timer = Box::leak(Box::new(Timer::new(&ui.frame)));
    let ui_tick = ui.clone();
    let ticked = std::cell::Cell::new(false);
    timer.on_tick(move |_| {
        if !ticked.get() {
            ticked.set(true);
            startup_log("tick: event loop alive, results flowing");
        }
        while let Ok(done) = done_rx.try_recv() {
            apply_done(&ui_tick, done);
        }
    });
    timer.start(120, false);

    // A separate callback opens the PIN modal so the results timer can keep draining jobs.
    let unlock_timer = Box::leak(Box::new(Timer::new(&ui.frame)));
    let ui_unlock = ui.clone();
    unlock_timer.on_tick(move |_| {
        let pending = { ui_unlock.state.borrow_mut().pending_unlock.take() };
        if let Some(retry) = pending {
            if current_list(&ui_unlock, &retry.view, retry.request) {
                unlock_messenger(&ui_unlock, Some(retry));
            } else {
                ui_unlock.state.borrow_mut().unlocking = false;
            }
        }
    });
    unlock_timer.start(120, false);

    frame.centre();
    startup_log("run: showing the window");
    frame.show(true);
    frame.raise();
    views.set_focus();
    // A copy the updater just relaunched takes the foreground on Windows.
    ship_shape::ui::bring_to_front_after_update(&frame);
    startup_log("run: window shown");
}

#[derive(Clone)]
struct Ui {
    frame: Frame,
    items: ListBox,
    details: TextCtrl,
    links: ListBox,
    open_link: Button,
    link_targets: Rc<RefCell<Vec<Link>>>,
    compose_box: TextCtrl,
    state: Rc<RefCell<State>>,
    jobs: Sender<Job>,
    done_tx: Sender<Done>,
    post_reader: Rc<RefCell<Option<post_dialog::PostReader>>>,
    /// The list request the user is waiting for; the worker skips older ones.
    latest_list: Arc<Mutex<ListRequests>>,
}

impl Ui {
    fn status(&self, text: &str) {
        self.frame.set_status_text(text, 0);
    }
    fn send(&self, job: Job) {
        self.status("Working…");
        let _ = self.jobs.send(job);
    }
}

fn spawn_worker(
    fb: Option<Fb>,
    jobs: Receiver<Job>,
    done: Sender<Done>,
    latest_list: Arc<Mutex<ListRequests>>,
) {
    thread::spawn(move || {
        let mut fb = fb;
        // Before the first backend command, so an old daemon never serves this session.
        if blindfb_core::backend::managed_outdated() {
            startup_log("setup: updating the Facebook backend");
            match blindfb_core::backend::install_or_update() {
                Ok(summary) => startup_log(&format!("setup: {summary}")),
                Err(error) => startup_log(&format!("setup: backend update failed: {error}")),
            }
            fb = Fb::discover();
        }
        while let Ok(job) = jobs.recv() {
            // Arrowing through Views queues a load per section; run only the one still wanted for that section.
            let list = match &job.tag {
                Tag::List { view, request, .. } => Some((view, *request)),
                Tag::GroupsOpened(request) => Some((&View::Groups, *request)),
                _ => None,
            };
            if let Some((view, request)) = list {
                if !latest_list.lock().unwrap().current(view, request) {
                    continue;
                }
            }
            let read = matches!(job.tag, Tag::List { .. })
                && matches!(job.task, Task::Fb(_, None));
            if read && fb.is_none() {
                fb = Fb::discover();
            }
            if let (true, Some(fb), Task::Fb(args, _)) = (read, fb.clone(), &job.task) {
                // The backend gives every command its own tab, so reads can run side by side.
                let (args, tag, done) = (args.clone(), job.tag, done.clone());
                thread::spawn(move || {
                    let result = fb.run(&refs(&args));
                    let _ = done.send(Done { tag, result });
                });
                continue;
            }
            if let Task::ReelsLoad { source, append } = &job.task {
                let (fb, source, append, tag, done) = (fb.clone(), source.clone(), *append, job.tag, done.clone());
                thread::spawn(move || {
                    let result = match fb {
                        Some(fb) => reels::load(&fb, &source, append),
                        None => Err("The Facebook backend is not installed".to_owned()),
                    };
                    let _ = done.send(Done {tag, result});
                });
                continue;
            }
            let video = match &job.task {
                Task::PostVideo {page, target} => Some((page.clone(), target.clone())),
                _ => None,
            };
            if let Some((page, target)) = video {
                // Stream extraction/downloads must not delay comments, reactions or closing the page.
                let (fb, page, target, tag, done) =
                    (fb.clone(), page, target, job.tag, done.clone());
                thread::spawn(move || {
                    let result = match fb {
                        Some(fb) => blindfb_core::media::stream_url(&fb, &page, &target),
                        None => Err("The Facebook backend is not installed".to_owned()),
                    };
                    let _ = done.send(Done { tag, result });
                });
                continue;
            }
            let result = match job.task {
                Task::ReelsLoad { .. } => unreachable!("reel reads run independently"),
                Task::InstallBackend => {
                    let installed = blindfb_core::backend::install_or_update();
                    if installed.is_ok() {
                        fb = Fb::discover();
                    }
                    installed
                }
                Task::LoginFlow(sign) => {
                    if fb.is_none() {
                        fb = Fb::discover();
                    }
                    match &fb {
                        None => Err("the backend is not installed yet".to_owned()),
                        Some(fb) => login_flow(fb, sign),
                    }
                }
                Task::InstallBrowser(kind) => {
                    let installed = browsers::install(kind);
                    if installed.is_ok() {
                        fb = Fb::discover();
                    }
                    installed
                }
                Task::Fb(args, stdin) => {
                    if fb.is_none() {
                        fb = Fb::discover();
                    }
                    match &fb {
                        None => Err(
                            "the fb backend is not installed — use Backend → Install or update backend"
                                .to_owned(),
                        ),
                        Some(fb) => match &stdin {
                            Some(input) => fb.run_stdin(&refs(&args), input),
                            None => fb.run(&refs(&args)),
                        },
                    }
                }
                Task::PostLoad { page, target } => match &fb {
                    Some(fb) => blindfb_core::posts::load(fb, &page, &target),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostVideo { page, target } => match &fb {
                    Some(fb) => blindfb_core::media::stream_url(fb, &page, &target),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostReelReact { page, reaction } => match &fb {
                    Some(fb) => blindfb_core::posts::reel_react(fb, &page, &reaction),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostRefresh { page } => match &fb {
                    Some(fb) => blindfb_core::posts::refresh(fb, &page),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostSend { page, parent, text } => match &fb {
                    Some(fb) => {
                        blindfb_core::posts::send_comment(fb, &page, parent.as_deref(), &text)
                    }
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostLike { page, id } => match &fb {
                    Some(fb) => blindfb_core::posts::like_comment(fb, &page, &id),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostShareOpen { page } => match &fb {
                    Some(fb) => blindfb_core::posts::share_open(fb, &page),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostShare { page, text } => match &fb {
                    Some(fb) => blindfb_core::posts::share_now(fb, &page, &text),
                    None => Err("The Facebook backend is not installed".to_owned()),
                },
                Task::PostShareCancel { page } => match &fb {
                    Some(fb) => blindfb_core::posts::share_cancel(fb, &page),
                    None => Ok(String::new()),
                },
                Task::PostClose { page } => match &fb {
                    Some(fb) => blindfb_core::posts::close(fb, &page),
                    None => Ok(String::new()),
                },
            };
            if done.send(Done { tag: job.tag, result }).is_err() {
                break;
            }
        }
    });
}

/// Sign in with the session of the browser the user picked: the backend
/// imports it (one "Allow" click in Chromium browsers; Firefox cookies are
/// copied from its profile). If the import does not end signed in, fall
/// back to `login --visible`, a window of the backend's own browser.
fn login_flow(fb: &Fb, sign: SignIn) -> Result<String, String> {
    match sign {
        SignIn::Browser(browser) if browser.kind == BrowserKind::Firefox => {
            match browsers::import_firefox(fb, &browser) {
                Ok(status) => Ok(status),
                Err(why) => visible_fallback(fb, why),
            }
        }
        SignIn::Browser(browser) => match browsers::import_chromium(fb, &browser) {
            Ok(status) => Ok(status),
            Err(why) => visible_fallback(fb, why),
        },
        SignIn::Visible => {
            fb.run(&["login", "--visible"])?;
            fb.run(&["status"])
        }
        SignIn::Install(_) => Err("install the browser first".to_owned()),
    }
}

/// Last resort for every import path: a visible sign-in window. Reports
/// the earlier failure when even that does not sign in.
fn visible_fallback(fb: &Fb, why: String) -> Result<String, String> {
    fb.run(&["login", "--visible"]).map_err(|error| {
        format!("Session import failed: {why}\n\nDirect sign-in failed: {error}")
    })?;
    let status = fb.run(&["status"])?;
    if browsers::signed_in(&status) { Ok(status) } else { Err(why) }
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

fn build_menus(frame: &Frame) {
    let file_menu = Menu::builder()
        .append_item(ID_OPEN_LINK, "Open Facebook &link…\tCtrl+O", "Open a Facebook post, video or reel link")
        .append_item(ID_NEW_POST, "&New post…\tCtrl+N", "Write a new post")
        .append_item(
            ID_NEW_POST_IMAGES,
            "New post with &images…",
            "Write a post with images attached",
        )
        .append_item(
            ID_SEARCH,
            "&Search Facebook…\tCtrl+F",
            "Search people, pages, groups, posts and events",
        )
        .append_item(
            ID_OCR,
            "&OCR an image…",
            "Read the text in an image file with the operating system's OCR engine",
        )
        .append_item(ID_UPDATE, "Check for &updates…", "Check GitHub for a newer BlindFB")
        .append_separator()
        .append_item(ID_EXIT, "E&xit\tCtrl+Q", "Close BlindFB")
        .build();

    let view_menu = Menu::builder()
        .append_item(ID_FEED, "News &feed\tCtrl+1", "Your news feed")
        .append_item(ID_NOTIFS, "&Notifications\tCtrl+2", "Your notifications")
        .append_item(ID_FRIENDS, "Fr&iends\tCtrl+3", "Your friends list")
        .append_item(ID_MYPOSTS, "&My posts\tCtrl+4", "Posts on your own profile")
        .append_item(ID_CHATS, "&Chats\tCtrl+5", "Messenger chats")
        .append_item(ID_GROUPS_VIEW, "&Groups\tCtrl+6", "Groups you belong to")
        .append_item(
            ID_RECENT_FEED,
            "Feeds (most &recent)\tCtrl+7",
            "Newest posts from followed friends, pages and groups",
        )
        .append_item(ID_REELS, "Re&els\tCtrl+8", "Browse reels with the accessible player")
        .append_item(ID_UNREAD_CHATS, "&Unread chats only", "Chats with unread messages")
        .append_separator()
        .append_item(ID_REFRESH, "&Refresh\tF5", "Reload the current view")
        .append_item(ID_MORE, "&Load more\tCtrl+L", "Append more items and keep your reading position")
        .build();

    let mut post_menu = Menu::builder()
        .append_item(ID_OPEN, "&Open post / comments", "Open the selected item; Enter in the items list")
        .append_item(ID_SHARE, "S&hare…", "Share the selected post")
        .append_item(ID_COMMENT, "&Comment…", "Comment on the selected post")
        .append_item(ID_PROFILE, "Author &profile", "Show the profile of the selected item's author or owner")
        .append_item(ID_AUTHOR_POSTS, "Author's p&osts", "Posts by the selected item's author")
        .append_item(ID_MESSAGE, "&Message author / friend", "Open a Messenger chat with this person")
        .append_item(ID_WEB_POST, "Open post on &Facebook (web)", "Open the selected post in your browser")
        .append_item(ID_WEB_PERSON, "Open person or group on Fa&cebook (web)", "Open the selected person's profile (or the group) in your browser, where friend management lives")
        .append_separator()
        .append_item(ID_DELETE, "&Delete post…", "Delete the selected post (your own posts only)");
    for (i, reaction) in REACTIONS.iter().enumerate() {
        post_menu = post_menu.append_item(
            ID_REACT_BASE + i as i32,
            &format!("React: {reaction}"),
            "Set this reaction on the selected post",
        );
    }
    let post_menu = post_menu.build();

    let chat_menu = Menu::builder()
        .append_item(ID_OPEN, "&Open chat", "Open the selected chat")
        .append_item(
            ID_WEB_CHAT,
            "Open chat in &Messenger (web)",
            "Open this chat in your browser — voice and video calls live there",
        )
        .append_item(
            ID_UNSEND,
            "&Unsend my last message…",
            "Remove your newest message in the open chat for everyone",
        )
        .append_item(
            ID_UNLOCK,
            "Unloc&k encrypted chats…",
            "Enter your Messenger PIN to read end-to-end encrypted chats",
        )
        .build();

    let backend_menu = Menu::builder()
        .append_item(
            ID_INSTALL_BACKEND,
            "&Install or update backend…",
            "Download BlindFB's supported facebook-cli backend from GitHub",
        )
        .append_item(ID_STATUS, "Backend &status", "Show fb daemon status")
        .append_item(ID_LOGIN, "&Sign in…", "Sign in through your browser")
        .append_item(
            ID_INSTALL_BROWSER,
            "Install a &browser…",
            "Download a browser BlindFB can use (Chrome, Brave, Edge, Firefox)",
        )
        .append_item(ID_STOP_DAEMON, "S&top daemon", "Stop the fb daemon")
        .append_item(
            ID_QUIT_DAEMON,
            "&Quit daemon and Chrome",
            "Stop the daemon and close its hidden Chrome",
        )
        .build();

    let help_menu = Menu::builder()
        .append_item(ID_KEYS, "&Keyboard commands\tF1", "Keyboard commands and reader controls")
        .append_item(ID_GITHUB, "BlindFB on &GitHub", "Open the BlindFB repository")
        .append_item(ID_ABOUT, "&About BlindFB", "About this program")
        .build();

    let menu_bar = MenuBar::builder()
        .append(file_menu, "&File")
        .append(view_menu, "&View")
        .append(post_menu, "&Post")
        .append(chat_menu, "&Chat")
        .append(backend_menu, "&Backend")
        .append(help_menu, "&Help")
        .build();
    frame.set_menu_bar(menu_bar);
}

fn activate_view(ui: &Ui, view: &View) {
    let mut state = ui.state.borrow_mut();
    let clear = state.records_view != *view;
    if clear {
        state.records.clear();
    }
    state.view = view.clone();
    drop(state);
    if clear {
        ui.items.clear();
        set_details(ui, "", None);
    }
}

fn begin_list(ui: &Ui, view: &View) -> u64 {
    activate_view(ui, view);
    ui.latest_list.lock().unwrap().begin(view)
}

fn current_list(ui: &Ui, view: &View, request: u64) -> bool {
    let state = ui.state.borrow();
    state.view == *view && ui.latest_list.lock().unwrap().current(view, request)
}

fn load_args(ui: &Ui, view: View, args: Vec<String>) {
    let request = begin_list(ui, &view);
    ui.state.borrow_mut().last_args = args.clone();
    ui.send(Job {
        task: Task::Fb(args.clone(), None),
        tag: Tag::List { view: view.clone(), request, args, unlock_attempted: false, append: false },
    });
    ui.status(&format!("Loading {}…", view.title()));
}

fn load_view(ui: &Ui, view: View) {
    let cached = ui.state.borrow().cache.get(&view).cloned();
    activate_view(ui, &view);
    if let Some((records, _, args)) = &cached {
        ui.state.borrow_mut().last_args = args.clone();
        show_records(ui, &view, records.clone(), None);
    }
    if ui.latest_list.lock().unwrap().pending.contains(&view) {
        ui.status(&format!("Loading {}…", view.title()));
        return;
    }
    if let Some((_, at, _)) = &cached {
        // The News feed pages forward on every load, so it waits for F5.
        if matches!(view, View::Feed | View::Reels(_)) || at.elapsed() < FRESH {
            return;
        }
    }
    if let View::Reels(source) = &view {
        load_reels(ui, source.clone(), false);
    } else if view == View::Groups {
        load_groups(ui);
    } else if let Some(args) = cached.as_ref().map(|(_, _, args)| args.clone())
        .or_else(|| default_args(&view)) {
        load_args(ui, view, args);
    }
    if cached.is_some() {
        let mut state = ui.state.borrow_mut();
        state.background = ui.latest_list.lock().unwrap().latest.get(&state.view).copied();
    }
}

/// Read every main section in the background once, so first visits are instant too.
fn prefetch(ui: &Ui) {
    for view in [View::RecentFeed, View::Notifications, View::Friends, View::MyPosts, View::Chats] {
        if let Some(args) = default_args(&view) {
            let request = {
                let mut requests = ui.latest_list.lock().unwrap();
                if requests.pending.contains(&view) || ui.state.borrow().cache.contains_key(&view) {
                    continue;
                }
                requests.begin(&view)
            };
            let _ = ui.jobs.send(Job {
                task: Task::Fb(args.clone(), None),
                tag: Tag::List { view, request, args, unlock_attempted: false, append: false },
            });
        }
    }
}

fn cacheable(view: &View, args: &[String]) -> bool {
    matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::Reels(_)
            | View::Notifications
            | View::Friends
            | View::MyPosts
            | View::Chats
            | View::Groups
    ) && !args.iter().any(|a| a == "--unread")
}

fn default_args(view: &View) -> Option<Vec<String>> {
    Some(match view {
        View::Feed => feed_args(FEED_FIRST, false, true, true),
        View::RecentFeed => recent_feed_args(PAGE),
        View::Notifications => notifs_args(PAGE),
        View::Friends => friends_args(None, 100),
        View::MyPosts => posts_args("me", PAGE, true),
        View::Chats => chats_args(PAGE, false),
        View::Groups | View::SearchResults | View::Reels(_) => return None,
        View::PostsOf(who) => posts_args(who, PAGE, true),
        View::CommentsOf(post) => view_args(post, 50),
        View::ProfileOf(who) => profile_args(who),
        View::ChatWith(name) => read_args(name, 50),
    })
}

/// Groups load in two steps: open the groups home page in the backend's
/// named browser page, then eval the group links out of it. The open result
/// is ignored on purpose: the page may already be open from an earlier load,
/// and the eval is what counts.
fn load_groups(ui: &Ui) {
    let request = begin_list(ui, &View::Groups);
    ui.status("Loading my groups…");
    ui.send(Job { task: Task::Fb(groups_open_args(), None), tag: Tag::GroupsOpened(request) });
}

fn load_reels(ui: &Ui, source: ReelSource, append: bool) {
    let view = View::Reels(source.clone());
    let request = begin_list(ui, &view);
    ui.send(Job {
        task: Task::ReelsLoad {source, append},
        tag: Tag::List {view: view.clone(), request, args: Vec::new(), unlock_attempted: false, append},
    });
    ui.status(&format!("Loading {}…", view.title()));
}

/// Ask which browser to copy the Facebook session from. The user's
/// default browser is marked; the last entry signs in directly in a new
/// window of the backend's own browser. When no supported browser is
/// installed, offer to install Google Chrome — the backend needs a
/// Chrome-family browser to run at all.
fn choose_sign_in(ui: &Ui) -> Option<SignIn> {
    let detected = browsers::detect_browsers();
    if !detected.iter().any(|browser| browser.kind.is_chromium() && browser.exe.is_some()) {
        if confirm(
            &ui.frame,
            "BlindFB needs an installed Chromium browser (Chrome, Brave or Edge) to run its separate browser profile. Firefox can supply your Facebook session but cannot run the backend. Install Google Chrome now?",
            "Sign in",
        ) {
            return Some(SignIn::Install(BrowserKind::Chrome));
        }
        return None;
    }
    let mut labels: Vec<String> = detected
        .iter()
        .map(|browser| {
            if browser.is_default {
                format!("{} (your default browser)", browser.name())
            } else {
                browser.name().to_owned()
            }
        })
        .collect();
    labels.push("Sign in in a new browser window".to_owned());
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        "Sign in",
        if cfg!(windows) {
            "Copy your Facebook session from which browser? You must be signed in to Facebook in that browser. Close the browser first if its cookie database is locked. Or choose a new browser window to sign in directly."
        } else {
            "Copy your Facebook session from which browser? You must be signed in to Facebook in that browser. In Chrome, Brave and Edge, tick \"Allow remote debugging\" if the browser asks."
        },
        &label_refs,
    )
    .build();
    if dialog.show_modal() != ID_OK {
        return None;
    }
    let index = dialog.get_selection();
    if index < 0 {
        return None;
    }
    let index = index as usize;
    if index >= detected.len() {
        return Some(SignIn::Visible);
    }
    Some(SignIn::Browser(detected[index].clone()))
}

/// Present the sign-in choice and start what the user picked. Returns
/// whether a job was sent (setup callers load the feed when it was not).
fn offer_sign_in(ui: &Ui, tag: Tag) -> bool {
    let sign = choose_sign_in(ui);
    if sign.is_none() {
        return false;
    }
    ui.latest_list.lock().unwrap().invalidate();
    {
        let mut state = ui.state.borrow_mut();
        state.account = state.account.wrapping_add(1);
        state.reel_cache = reel_cache::ReelCache::default();
        state.sending = None;
        state.view = View::Feed;
        state.records_view = View::Feed;
        state.attachments.clear();
        state.cache.clear();
        state.prefetched = false;
        state.background = None;
        state.author_urls.clear();
        state.records.clear();
        state.last_args.clear();
        state.pending_unlock = None;
        state.unlocking = false;
    }
    ui.items.clear();
    set_details(ui, "", None);
    ui.compose_box.clear();
    match sign {
        Some(SignIn::Install(kind)) => {
            ui.status(&format!("Installing {}…", kind.name()));
            ui.send(Job { task: Task::InstallBrowser(kind), tag: Tag::BrowserInstalled });
            true
        }
        Some(sign) => {
            ui.send(Job { task: Task::LoginFlow(sign), tag });
            ui.status("Signing in through your browser…");
            true
        }
        None => false,
    }
}

/// Menu entry point for the same guided sign-in.
fn start_sign_in(ui: &Ui, tag: Tag) {
    offer_sign_in(ui, tag);
}

/// Backend → Install a browser…: pick a browser to download from its
/// vendor (installed ones are marked; reinstalling also repairs).
fn install_browser_menu(ui: &Ui) {
    let kinds = [BrowserKind::Chrome, BrowserKind::Brave, BrowserKind::Edge, BrowserKind::Firefox];
    let detected = browsers::detect_browsers();
    let labels: Vec<String> = kinds
        .iter()
        .map(|kind| {
            if detected.iter().any(|browser| browser.kind == *kind) {
                format!("{} (installed)", kind.name())
            } else {
                kind.name().to_owned()
            }
        })
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        "Install a browser",
        "Install which browser? Downloads come from the vendor. Chrome, Chromium, Brave and Edge can run BlindFB's separate browser profile. Firefox can supply a Facebook session.",
        &label_refs,
    )
    .build();
    if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        if index >= 0 {
            let kind = kinds[index as usize];
            ui.status(&format!("Installing {}…", kind.name()));
            ui.send(Job { task: Task::InstallBrowser(kind), tag: Tag::BrowserInstalledMenu });
        }
    }
}

fn refresh(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    if matches!(view, View::Feed | View::Reels(_)) && ui.latest_list.lock().unwrap().pending.contains(&view) {
        ui.status("Wait for this section to finish loading before refreshing.");
        return;
    }
    match view {
        View::Reels(source) => load_reels(ui, source, false),
        View::Feed => load_args(ui, view, feed_args(FEED_FIRST, true, true, true)),
        View::Groups => load_groups(ui),
        _ => {
            let previous = ui.state.borrow().last_args.clone();
            if let Some(args) = refresh_args(&view, &previous) {
                load_args(ui, view, args);
            }
        }
    }
}

fn refresh_args(view: &View, previous: &[String]) -> Option<Vec<String>> {
    if *view == View::SearchResults {
        (!previous.is_empty()).then(|| previous.to_vec())
    } else {
        default_args(view)
    }
}

fn more_args(view: &View, previous: &[String], loaded: usize, loading: bool) -> Option<Vec<String>> {
    if loading || previous.is_empty() {
        return None;
    }
    match view {
        View::Feed => Some(feed_args(PAGE, false, true, true)),
        View::Notifications => Some(notifs_args(u32::try_from(loaded).ok()?.checked_add(PAGE)?)),
        View::RecentFeed | View::MyPosts | View::PostsOf(_) | View::Friends
        | View::Chats | View::SearchResults => {
            let mut args = previous.to_vec();
            let index = args.iter().position(|arg| arg == "-n")? + 1;
            let count: u32 = args.get(index)?.parse().ok()?;
            args[index] = count.checked_add(PAGE)?.to_string();
            Some(args)
        }
        _ => None,
    }
}

fn load_more(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    if let View::Reels(source) = view {
        let loaded = {
            let state = ui.state.borrow();
            state.records_view == state.view && !state.records.is_empty()
        };
        if ui.latest_list.lock().unwrap().pending.contains(&ui.state.borrow().view) {
            ui.status("Reels are already loading…");
        } else if !loaded {
            ui.status("Load the reels list first, or refresh it to retry.");
        } else {
            load_reels(ui, source, true);
        }
        return;
    }
    let (view, args) = {
        let state = ui.state.borrow();
        if state.records_view != state.view || state.records.is_empty() {
            ui.status("Wait for this section to load before requesting more items.");
            return;
        }
        let loading = ui.latest_list.lock().unwrap().pending.contains(&state.view);
        let Some(args) = more_args(&state.view, &state.last_args, state.records.len(), loading) else {
            ui.status(if loading { "Items are already loading…" } else {
                "Load more is available for feeds, posts, search results, notifications, friends and chat lists."
            });
            return;
        };
        (state.view.clone(), args)
    };
    let request = begin_list(ui, &view);
    ui.send(Job {
        task: Task::Fb(args.clone(), None),
        tag: Tag::List { view: view.clone(), request, args, unlock_attempted: false, append: true },
    });
    ui.status(&format!("Loading more {}…", view.title()));
}

fn merge_records(view: &View, mut existing: Vec<Record>, incoming: Vec<Record>) -> Vec<Record> {
    let key = |record: &Record| match view {
        View::Friends | View::Groups => record.field(1).to_owned(),
        View::Chats => record.field(0).to_owned(),
        View::Notifications => format!("{}\t{}", record.field(2), record.field(3)),
        _ => post_target(view, record).map(str::to_owned).unwrap_or_else(|| record.raw.clone()),
    };
    let mut seen: HashSet<String> = existing.iter().map(&key).collect();
    let feed = matches!(view, View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_));
    let mut seen_posts: HashSet<String> = existing.iter().map(blindfb_core::fb::feed_record_key).collect();
    existing.extend(incoming.into_iter().filter(|record| {
        seen.insert(key(record)) && (!feed || seen_posts.insert(blindfb_core::fb::feed_record_key(record)))
    }));
    existing
}

/// A row's list label, marked when the row has no link to open. Without the
/// marker the only way to find out is pressing Enter and getting a dialog.
fn list_label(view: &View, record: &Record) -> String {
    let label = display_for(view, record);
    let has_posts = matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::Notifications
            | View::MyPosts
            | View::PostsOf(_)
            | View::SearchResults
    );
    if has_posts && post_target(view, record).is_none() {
        format!("{label} (no link)")
    } else {
        label
    }
}

fn display_for(view: &View, record: &Record) -> String {
    match view {
        View::Feed | View::RecentFeed | View::Reels(_) | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            if record.fields.len() >= 5 {
                // A post without text or time must not read as ". ." to a screen reader.
                let text = one_line(&record.fields[4..].join(" "));
                [record.field(1), text.as_str(), record.field(2)]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(". ")
            } else {
                record.raw.clone()
            }
        }
        View::Notifications => {
            if record.fields.len() >= 3 {
                format!(
                    "{}{}. {}",
                    if record.field(1) == "!" { "Unread. " } else { "" },
                    record.field(2),
                    record.field(0)
                )
            } else {
                record.raw.clone()
            }
        }
        View::Friends => {
            if record.fields.len() >= 3 {
                format!("{} — {}", record.field(0), record.field(2))
            } else {
                record.raw.clone()
            }
        }
        View::Chats => {
            if record.fields.len() >= 5 {
                let flags = record.field(1);
                let marker = if flags.contains('!') { " (unread)" } else { "" };
                format!(
                    "{}{} · {} — {}",
                    record.field(3),
                    marker,
                    record.field(2),
                    record.fields[4..].join(" ")
                )
            } else {
                record.raw.clone()
            }
        }
        View::CommentsOf(_) => {
            if record.fields.len() >= 4 && record.field(0) == "c" {
                format!(
                    "{} · {} — {}",
                    record.field(1),
                    record.field(2),
                    record.fields[3..].join(" ")
                )
            } else {
                record.raw.clone()
            }
        }
        _ => record.raw.clone(),
    }
}

fn details_for(view: &View, record: &Record) -> String {
    match view {
        View::Feed | View::RecentFeed | View::Reels(_) | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            if record.fields.len() >= 5 {
                format!(
                    "Author: {}\nTime: {}\nReactions / comments / shares: {}\n\n{}",
                    record.field(1),
                    record.field(2),
                    record.field(3),
                    record.fields[4..].join("\t")
                )
            } else {
                record.raw.clone()
            }
        }
        View::Friends => format!(
            "Name: {}\nRef: {}\n{}",
            record.field(0),
            record.field(1),
            record.field(2)
        ),
        View::Chats => format!(
            "Chat: {}\nTime: {}\nFlags: {} (! unread, g group, e encrypted)\n\n{}",
            record.field(3),
            record.field(2),
            record.field(1),
            record.fields[4..].join("\t")
        ),
        View::Groups => format!(
            "Group: {}\nOn the web: {}\n\nOpen the group to read and take part in its posts here. Posting to a group, and group settings, live on the web page.",
            record.field(0),
            record.field(1)
        ),
        _ => record.raw.clone(),
    }
}

fn one_line(text: &str) -> String {
    text.replace('⏎', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn post_target<'a>(view: &View, record: &'a Record) -> Option<&'a str> {
    let value = match view {
        View::Notifications => record.field(3),
        View::Feed | View::RecentFeed | View::Reels(_) | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            record.field(0)
        }
        _ => return None,
    }
    .trim();
    let reference = value.starts_with('p')
        && value.len() >= 5
        && value.len() <= 16
        && value[1..].bytes().all(|c| c.is_ascii_alphanumeric());
    let url = value.starts_with("https://www.facebook.com/")
        || value.starts_with("https://facebook.com/")
        || value.starts_with("https://m.facebook.com/")
        || value.starts_with('/');
    (reference || url).then_some(value)
}

fn record_key(view: &View, record: &Record) -> String {
    post_target(view, record)
        .unwrap_or_else(|| match view {
            View::Friends | View::Groups => record.field(1),
            View::Feed | View::RecentFeed | View::Reels(_) | View::MyPosts | View::PostsOf(_) | View::SearchResults => &record.raw,
            _ => record.field(0),
        })
        .to_owned()
}

// shortcut: extract URLs written in the text; named attachment links are available in the full reader.
fn links_in_text(text: &str, post: Option<&str>) -> Vec<Link> {
    let mut links = Vec::new();
    if let Some(url) = post {
        links.push(Link { label: "Full post on Facebook".to_owned(), url: url.to_owned() });
    }
    for word in text.split(|c: char| c.is_whitespace() || c == '⏎') {
        let mut url = word.trim_start_matches(['(', '[', '<', '"', '\'']);
        loop {
            let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '"', '\'', '>']);
            let extra_closing = [('(', ')'), ('[', ']')].iter().any(|(open, close)| {
                trimmed.ends_with(*close) && trimmed.matches(*close).count() > trimmed.matches(*open).count()
            });
            if extra_closing {
                url = &trimmed[..trimmed.len() - 1];
            } else {
                url = trimmed;
                break;
            }
        }
        let Some((scheme, rest)) = url.split_once("://") else { continue };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
            || authority.is_empty()
            || url.chars().any(|c| c.is_control() || matches!(c, '\\' | '"' | '\'' | '<' | '>'))
        {
            continue;
        }
        if !links.iter().any(|link| link.url.trim_end_matches('/') == url.trim_end_matches('/')) {
            links.push(Link { label: url.to_owned(), url: url.to_owned() });
        }
    }
    match post {
        Some(post) => blindfb_core::posts::ordered_links(links, post),
        None => links,
    }
}

fn set_details(ui: &Ui, text: &str, post: Option<&str>) {
    if ui.details.get_value() != text {
        ui.details.change_value(text);
    }
    let links = links_in_text(text, post);
    if *ui.link_targets.borrow() == links { return; }
    let selected = ui.links.get_selection().and_then(|i| {
        ui.link_targets.borrow().get(i as usize).map(|link| link.url.clone())
    });
    ui.links.clear();
    for link in &links {
        let label = if link.label == link.url {
            link.url.clone()
        } else {
            format!("{} — {}", link.label, link.url)
        };
        ui.links.append(&label);
    }
    if let Some(index) = selected.and_then(|url| links.iter().position(|link| link.url == url))
        .or_else(|| (!links.is_empty()).then_some(0))
    {
        ui.links.set_selection(index as u32, true);
    }
    *ui.link_targets.borrow_mut() = links;
    ui.open_link.enable(ui.links.get_selection().is_some());
}

fn open_detail_link(ui: &Ui) {
    if let Some(url) = ui.links.get_selection().and_then(|i| {
        ui.link_targets.borrow().get(i as usize).map(|link| link.url.clone())
    }) {
        open_url(&url);
    }
}

fn append_links_menu(menu: &Menu, links: &[Link]) {
    if links.is_empty() { return; }
    const BASE: i32 = 20000;
    let submenu = Menu::builder().build();
    for (index, link) in links.iter().enumerate() {
        let label = link.label.replace('&', "&&").replace(['\t', '\r', '\n'], " ");
        submenu.append(BASE + index as i32, &format!("{}. {label}", index + 1), &link.url, ItemKind::Normal);
    }
    // Keep the URLs shown when the menu opened, even if a background refresh finishes.
    let targets = links.to_vec();
    submenu.on_selected(move |event| {
        event.event.skip(false);
        if let Some(link) = event.get_id().checked_sub(BASE).and_then(|index| targets.get(index as usize)) {
            open_url(&link.url);
        }
    });
    menu.append_submenu(submenu, "&Links", "Open a link in your browser");
}

fn prepare_reels(ui: &Ui) {
    let Some(index) = ui.items.get_selection() else { return };
    let (account, page, targets) = {
        let mut state = ui.state.borrow_mut();
        if !matches!(state.records_view, View::Reels(_)) || state.view != state.records_view { return; }
        let View::Reels(source) = &state.records_view else { return };
        let page = reels::page_name(source);
        let targets: Vec<_> = state.records.iter().skip(index as usize).take(2)
            .map(|record| record.field(0).to_owned()).collect();
        let targets = state.reel_cache.plan(&targets, Instant::now());
        (state.account, page, targets)
    };
    for target in targets {
        let _ = ui.jobs.send(Job {
            task: Task::PostVideo { page: page.clone(), target: target.clone() },
            tag: Tag::ReelPrepared { account, target },
        });
    }
}

fn show_details(ui: &Ui, index: usize) {
    let state = ui.state.borrow();
    if let Some(record) = state.records.get(index) {
        let text = details_for(&state.view, record);
        let post = post_target(&state.view, record)
            .filter(|target| target.starts_with("https://") || target.starts_with('/'))
            .map(web_url);
        drop(state);
        set_details(ui, &text, post.as_deref());
    }
}

fn selected(ui: &Ui) -> Option<(usize, Record)> {
    let index = ui.items.get_selection()? as usize;
    let state = ui.state.borrow();
    if state.view != state.records_view {
        return None;
    }
    state.records.get(index).cloned().map(|r| (index, r))
}

fn selected_post_ref(ui: &Ui) -> Option<String> {
    let (_, record) = selected(ui)?;
    post_target(&ui.state.borrow().view, &record).map(str::to_owned)
}

/// Show a section's backend output in the Items list, updated in place so the
/// selection and the screen reader's position survive.
fn show_records(ui: &Ui, view: &View, records: Vec<Record>, body: Option<String>) {
    if matches!(view, View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_)) {
        let mut state = ui.state.borrow_mut();
        for record in &records {
            let author = record.field(1);
            if !author.is_empty() {
                if let Some(profile) = profile_url_from_post_url(record.field(0)) {
                    state.author_urls.insert(author.to_owned(), profile);
                }
            }
        }
        drop(state);
    }
    let old_index = ui.items.get_selection();
    let (old_labels, selected_key) = {
        let state = ui.state.borrow();
        let labels: Vec<_> =
            state.records.iter().map(|r| list_label(&state.records_view, r)).collect();
        let key = old_index
            .and_then(|i| state.records.get(i as usize))
            .map(|r| record_key(&state.records_view, r))
            .filter(|k| !k.is_empty() && k != "-");
        (labels, key)
    };
    let labels: Vec<_> = records.iter().map(|r| list_label(view, r)).collect();
    let selected = selected_key
        .and_then(|key| records.iter().position(|r| record_key(view, r) == key))
        .or_else(|| (!records.is_empty()).then_some(0));
    {
        let mut state = ui.state.borrow_mut();
        state.records = records;
        state.records_view = view.clone();
    }
    for (index, label) in labels.iter().enumerate() {
        if index >= ui.items.get_count() as usize {
            ui.items.append(label);
        } else if old_labels.get(index) != Some(label) {
            ui.items.set_string(index as u32, label);
        }
    }
    while ui.items.get_count() as usize > labels.len() {
        ui.items.delete(ui.items.get_count() - 1);
    }
    if let Some(selected) = selected {
        if ui.items.get_selection() != Some(selected as u32) {
            ui.items.set_selection(selected as u32, true);
        }
    }
    if let Some(body) = body {
        set_details(ui, &body, None);
    } else if ui.items.get_count() > 0 {
        show_details(ui, selected.unwrap_or(0));
    } else {
        set_details(ui, "(nothing here)", None);
    }
    ui.status(&format!("{} — {} item(s)", view.title(), ui.items.get_count()));
    prepare_reels(ui);
}

fn apply_done(ui: &Ui, done: Done) {
    match done.tag {
        Tag::List { view, request, args, unlock_attempted, append } => {
            let current = current_list(ui, &view, request);
            if !ui.latest_list.lock().unwrap().finish(&view, request) {
                return;
            }
            match done.result.and_then(|raw| parse_for(&view, &raw)) {
                Ok((records, body)) => {
                    let previous = {
                        let state = ui.state.borrow();
                        if current { state.records.clone() } else {
                            state.cache.get(&view).map(|(rows, _, _)| rows.clone()).unwrap_or_default()
                        }
                    };
                    let before = previous.len();
                    let records = if append {
                        merge_records(&view, previous, records)
                    } else { records };
                    let added = records.len().saturating_sub(before);
                    if cacheable(&view, &args) {
                        ui.state
                            .borrow_mut()
                            .cache
                            .insert(view.clone(), (records.clone(), Instant::now(), args.clone()));
                    }
                    if !current {
                        return;
                    }
                    ui.state.borrow_mut().last_args = args;
                    show_records(ui, &view, records, body);
                    if append {
                        ui.status(&format!("{added} new item(s) loaded. {} item(s) total.", ui.items.get_count()));
                    }
                    if view == View::Feed && !append && ui.items.get_count() > 0 {
                        load_more(ui);
                    } else if view == View::Feed
                        && !ui.state.borrow().prefetched
                    {
                        // After the feed's second page, so other sections never slow the first ones.
                        ui.state.borrow_mut().prefetched = true;
                        prefetch(ui);
                    }
                }
                Err(e) => {
                    if !current {
                        return;
                    }
                    if matches!(view, View::Chats | View::ChatWith(_)) && needs_messenger_unlock(&e)
                    {
                        let mut state = ui.state.borrow_mut();
                        if !unlock_attempted && !state.unlocking {
                            state.unlocking = true;
                            state.pending_unlock = Some(RetryList { view, request, args, append });
                            drop(state);
                            ui.status("Messenger needs its history restored. Enter your PIN in the unlock dialog.");
                        } else {
                            drop(state);
                            ui.status("Messenger history is still locked. Use Chat → Unlock encrypted chats to retry.");
                            if unlock_attempted {
                                show_error(
                                    &ui.frame,
                                    "Messenger still blocks input after unlocking. Check your PIN or restore message history in Messenger, then retry.",
                                    "Messenger history",
                                );
                            }
                        }
                        return;
                    }
                    if ui.state.borrow().background == Some(request) {
                        ui.status(&format!(
                            "Could not update {}. Showing earlier results.",
                            view.title()
                        ));
                        return;
                    }
                    ui.status("Error");
                    show_error(&ui.frame, &e, "BlindFB");
                }
            }
        }
        Tag::PostVideo(session, request) => post_dialog::stream(ui, session, request, done.result),
        Tag::PostReelReacted(session) => {
            post_dialog::apply(ui, session, done.result, "Reaction set.", false)
        }
        Tag::ReelPrepared {account, target} => {
            {
                let mut state = ui.state.borrow_mut();
                if state.account != account { return; }
                state.reel_cache.finish(&target, &done.result, Instant::now());
            }
            post_dialog::prepared(ui, &target, done.result);
            prepare_reels(ui);
        }
        Tag::PostLoaded(session) => {
            post_dialog::apply(ui, session, done.result, "Post ready.", false)
        }
        Tag::PostSent(session) => {
            post_dialog::apply(ui, session, done.result, "Comment sent.", true)
        }
        Tag::PostLiked(session) => {
            post_dialog::apply(ui, session, done.result, "Like changed.", false)
        }
        Tag::PostShareReady(session) => post_dialog::share_ready(ui, session, done.result),
        Tag::PostShared(session) => post_dialog::note(
            ui,
            session,
            done.result.map(|_| "Shared to your feed.".to_owned()),
            true,
        ),
        Tag::PostShareClosed(session) => post_dialog::note(
            ui,
            session,
            done.result.map(|_| "Share cancelled. Nothing was shared.".to_owned()),
            true,
        ),
        Tag::PostNote(session) => {
            post_dialog::note(ui, session, done.result.map(|_| "Reaction set.".to_owned()), false)
        }
        Tag::Silent => {}
        Tag::UnlockRetry { view, request, args, append } => {
            ui.state.borrow_mut().unlocking = false;
            match done.result {
                Ok(_) if !args.is_empty() && current_list(ui, &view, request) => {
                    ui.latest_list.lock().unwrap().pending.insert(view.clone());
                    ui.send(Job {
                        task: Task::Fb(args.clone(), None),
                        tag: Tag::List { view, request, args, unlock_attempted: true, append },
                    });
                    ui.status("Messenger unlocked. Loading your chats…");
                }
                Ok(_) => ui.status("Messenger unlocked."),
                Err(e) => {
                    ui.status("Messenger could not be unlocked.");
                    show_error(
                        &ui.frame,
                        format!(
                            "Messenger could not restore your history. Check the PIN, or restore history in Messenger and retry.\n\n{e}"
                        ),
                        "Unlock encrypted chats",
                    );
                }
            }
        }
        Tag::Text => match done.result {
            Ok(text) => {
                set_details(ui, text.trim(), None);
                ui.status("Done");
            }
            Err(e) => {
                ui.status("Error");
                show_error(&ui.frame, &e, "BlindFB");
            }
        },
        Tag::ChatSent(account) => {
            let submitted = ChatSubmission::finish(&mut ui.state.borrow_mut().sending, account);
            let Some(submitted) = submitted else { return };
            match done.result {
                Ok(_) => {
                    let (clear, current) = {
                        let mut state = ui.state.borrow_mut();
                        let view = state.view.clone();
                        let clear = submitted.clear_sent(
                            &view, &ui.compose_box.get_value(), &mut state.attachments,
                        );
                        (clear, view == View::ChatWith(submitted.chat.clone()))
                    };
                    if clear {
                        ui.compose_box.clear();
                    }
                    ui.status("Sent");
                    if current {
                        load_view(ui, View::ChatWith(submitted.chat));
                    }
                }
                Err(e) => {
                    ui.status("Message could not be sent. Your draft has been kept.");
                    show_error(&ui.frame, &e, "Send message");
                }
            }
        }
        Tag::ReloadChat(chat, account) => {
            if ui.state.borrow().account != account {
                return;
            }
            match done.result {
                Ok(_) => {
                    ui.status("Message removed");
                    if ui.state.borrow().view == View::ChatWith(chat.clone()) {
                        load_view(ui, View::ChatWith(chat));
                    }
                }
                Err(e) => {
                    ui.status("Error");
                    show_error(&ui.frame, &e, "BlindFB");
                }
            }
        }
        Tag::SetupStatus => match done.result {
            Ok(out) if out.contains("logged in") && !out.contains("not logged in") => {
                startup_log("setup: backend present and signed in");
                load_view(ui, View::Feed);
            }
            Ok(_) => {
                // Installed but signed out: offer the browser sign-in once.
                startup_log("setup: backend present, signed out — offering sign-in");
                ui.status("The backend is installed but not signed in.");
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status("Signed out. Use Backend → Sign in when ready.");
                }
            }
            Err(_) if Fb::discover().is_none() => {
                startup_log("setup: backend missing — installing");
                ui.status("Installing the Facebook backend…");
                ui.send(Job { task: Task::InstallBackend, tag: Tag::SetupInstall });
            }
            Err(error) => {
                ui.status("The Facebook backend could not start.");
                show_error(&ui.frame, error, "Backend startup");
                offer_sign_in(ui, Tag::SetupLogin);
            }
        },
        Tag::SetupInstall => match done.result {
            Ok(summary) => {
                ui.status(&summary);
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status("Signed out. Use Backend → Sign in when ready.");
                }
            }
            Err(error) => {
                show_error(
                    &ui.frame,
                    format!(
                        "The backend could not be installed automatically: {error}\nYou can retry from Backend → Install or update backend."
                    ),
                    "Backend install failed",
                );
            }
        },
        Tag::SetupLogin => match done.result {
            Ok(out) if out.contains("logged in") && !out.contains("not logged in") => {
                ui.status("Signed in.");
                load_view(ui, View::Feed);
            }
            Ok(out) => {
                set_details(ui, out.trim(), None);
                ui.status("Sign-in did not finish — see the details, or try Backend → Sign in.");
                show_warning(&ui.frame, out.trim(), "Sign-in did not finish");
            }
            Err(error) => {
                show_error(&ui.frame, error, "Sign in");
                ui.status("Sign-in failed. Use Backend → Sign in to retry.");
            }
        },
        Tag::BrowserInstalled => match done.result {
            Ok(summary) => {
                ui.status(&summary);
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status("Signed out. Use Backend → Sign in when ready.");
                }
            }
            Err(error) => {
                show_error(&ui.frame, error, "Browser install");
                ui.status("Browser installation failed.");
            }
        },
        Tag::BrowserInstalledMenu => match done.result {
            Ok(summary) => ui.status(&summary),
            Err(error) => show_error(&ui.frame, error, "Browser install"),
        },
        Tag::GroupsOpened(request) => {
            if ui.latest_list.lock().unwrap().current(&View::Groups, request) {
                let args = groups_eval_args();
                let _ = ui.jobs.send(Job {
                    task: Task::Fb(args.clone(), None),
                    tag: Tag::List { view: View::Groups, request, args, unlock_attempted: false, append: false },
                });
            }
        }
    }
}

fn parse_for(view: &View, raw: &str) -> Result<(Vec<Record>, Option<String>), String> {
    Ok(match view {
        View::Notifications => (parse_notifications(raw)?, None),
        View::Reels(_) => reels::parse(raw)?,
        View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_) => (parse_feed(raw), None),
        View::ChatWith(_) => (
            raw.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| Record { fields: vec![l.to_owned()], raw: l.to_owned() })
                .collect(),
            None,
        ),
        View::CommentsOf(_) => {
            let mut body_lines = Vec::new();
            let mut comments = Vec::new();
            for line in raw.lines() {
                if line.starts_with("c\t") {
                    comments.push(Record {
                        fields: line.split('\t').map(str::to_owned).collect(),
                        raw: line.to_owned(),
                    });
                } else if !line.trim().is_empty() {
                    body_lines.push(line);
                }
            }
            (comments, Some(body_lines.join("\n")))
        }
        View::ProfileOf(_) => (
            raw.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| Record { fields: vec![l.to_owned()], raw: l.to_owned() })
                .collect(),
            Some(raw.trim().to_owned()),
        ),
        View::Groups => (parse_groups_json(raw), None),
        _ => (parse_records(raw), None),
    })
}

/// The backend's error when Messenger's restore-history PIN prompt covers the page.
fn needs_messenger_unlock(error: &str) -> bool {
    error.contains("history prompt blocked input")
}

/// Ask for the Messenger PIN (stdin only, never stored). With `retry`, the
/// failed list request runs once more after a successful unlock.
fn unlock_messenger(ui: &Ui, retry: Option<RetryList>) {
    let dialog = TextEntryDialog::builder(&ui.frame, "Messenger PIN:", "Unlock encrypted chats")
        .password()
        .build();
    let pin = (dialog.show_modal() == ID_OK).then(|| dialog.get_value()).flatten();
    dialog.destroy();
    let Some(pin) = pin.filter(|p| !p.is_empty()) else {
        ui.state.borrow_mut().unlocking = false;
        ui.status("Unlock cancelled.");
        return;
    };
    let retry = retry.unwrap_or_else(|| RetryList {
        view: ui.state.borrow().view.clone(),
        request: 0,
        args: Vec::new(),
        append: false,
    });
    ui.state.borrow_mut().unlocking = true;
    ui.status("Unlocking Messenger…");
    ui.send(Job {
        task: Task::Fb(vec!["unlock".into(), "-".into()], Some(format!("{pin}\n"))),
        tag: Tag::UnlockRetry { view: retry.view, request: retry.request, args: retry.args, append: retry.append },
    });
}

fn open_facebook_link(ui: &Ui) {
    let dialog = TextEntryDialog::builder(
        &ui.frame,
        "Facebook post, video or reel link (full https:// URL):",
        "Open Facebook link",
    ).build();
    let target = (dialog.show_modal() == ID_OK).then(|| dialog.get_value()).flatten();
    dialog.destroy();
    let Some(target) = target else { return };
    let target = target.trim();
    match (target.starts_with("https://"), blindfb_core::posts::reader_url(target)) {
        (true, Some(url)) => post_dialog::open(ui, &url, false),
        _ => show_error(
            &ui.frame,
            "Paste a full https:// Facebook post, video or reel link. This link is invalid or unsupported.",
            "Open Facebook link",
        ),
    }
}

fn open_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    if view == View::Notifications {
        if let Some(source) = reels::notification_source(&record) {
            load_view(ui, View::Reels(source));
            return;
        }
    }
    match view {
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::SearchResults
        | View::Notifications => {
            let Some(target) = post_target(&view, &record) else {
                show_warning(
                    &ui.frame,
                    "This row has no link, so there is no post to open. Sponsored and suggested rows usually have none.",
                    "Open post",
                );
                return;
            };
            if !target.starts_with("http") && !target.starts_with('/') {
                // Short refs (search results) still open in the classic comments view.
                load_view(ui, View::CommentsOf(target.to_owned()));
                return;
            }
            match blindfb_core::posts::reader_url(target) {
                Some(url) if matches!(view, View::Reels(_)) => {
                    let (stream, preparing) = {
                        let state = ui.state.borrow();
                        (state.reel_cache.ready(&url, Instant::now()).map(str::to_owned), state.reel_cache.is_preparing(&url))
                    };
                    post_dialog::open_reel(ui, &url, stream, preparing);
                }
                Some(url) => post_dialog::open(ui, &url, false),
                None => {
                    // Photo albums and other pages the reader cannot parse yet.
                    if confirm(
                        &ui.frame,
                        "BlindFB cannot read this kind of item yet (for example a photo album). Open it in your web browser?",
                        "Open post",
                    ) {
                        open_url(&web_url(target));
                    }
                }
            }
        }
        View::Chats => {
            let name = record.field(3).to_owned();
            if !name.is_empty() {
                load_view(ui, View::ChatWith(name));
            }
        }
        View::Friends => profile_selected(ui),
        View::Groups => {
            let url = record.field(1).to_owned();
            if !url.is_empty() {
                load_view(ui, View::PostsOf(url));
            }
        }
        _ => {}
    }
}

/// The selected post's target, or a spoken warning saying why there is none.
fn post_or_warn(ui: &Ui, title: &str) -> Option<String> {
    let post = selected_post_ref(ui);
    if post.is_none() {
        let why = if selected(ui).is_some() {
            "This row has no link, so BlindFB cannot open, react to or comment on it. Sponsored and suggested rows usually have none."
        } else {
            "Select a post first."
        };
        show_warning(&ui.frame, why, title);
    }
    post
}

fn react_popup(ui: &Ui) {
    let Some(post) = post_or_warn(ui, "React") else {
        return;
    };
    let labels: Vec<&str> = REACTIONS.to_vec();
    let dialog = SingleChoiceDialog::builder(&ui.frame, "Reaction", "React", &labels).build();
    let reaction = if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        (index >= 0).then(|| REACTIONS[index as usize])
    } else { None };
    dialog.destroy();
    if let Some(reaction) = reaction {
        react_to(ui, &post, reaction);
    }
}

fn react_to(ui: &Ui, post: &str, reaction: &str) {
    if post_dialog::is_video_page(post) {
        post_dialog::react(ui, post, reaction);
    } else {
        ui.send(Job { task: Task::Fb(react_args(post, reaction), None), tag: Tag::Text });
    }
}

fn comment_selected(ui: &Ui) {
    let Some(post) = post_or_warn(ui, "Comment") else {
        return;
    };
    if post_dialog::is_video_page(&post) {
        post_dialog::comment(ui, &post);
        return;
    }
    if let Some(text) = prompt_text(&ui.frame, "Your comment:", "Comment") {
        if !text.trim().is_empty() {
            ui.send(Job { task: Task::Fb(comment_args(&post, &text), None), tag: Tag::Text });
        }
    }
}

/// The person a selected record is about: a friend record's ref, a post's
/// author name, or a chat's name.
fn person_of(ui: &Ui, record: &Record) -> Option<String> {
    let view = ui.state.borrow().view.clone();
    match view {
        View::Friends => Some(record.field(1).to_owned()),
        View::Chats => Some(record.field(3).to_owned()),
        _ => {
            let author = record.field(1);
            if author.is_empty() { None } else { Some(author.to_owned()) }
        }
    }
}

fn profile_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    if let Some(who) = person_of(ui, &record) {
        load_view(ui, View::ProfileOf(who));
    }
}

fn posts_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    if let Some(who) = person_of(ui, &record) {
        load_view(ui, View::PostsOf(who));
    }
}

fn message_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    let name = match view {
        View::Friends => record.field(0).to_owned(),
        View::Chats => record.field(3).to_owned(),
        _ => record.field(1).to_owned(),
    };
    if !name.is_empty() {
        load_view(ui, View::ChatWith(name));
    }
}

/// Link out to the selected post on the web. Feed and post views load with
/// --urls, so a post record's first field is its web address.
fn open_post_web(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let target = match ui.state.borrow().view {
        View::Notifications => record.field(3),
        _ => record.field(0),
    };
    if target.starts_with("http") || target.starts_with('/') {
        open_url(&web_url(target));
    } else {
        show_warning(
            &ui.frame,
            "This row has no web link (it is a suggestion or ad row).",
            "Open on Facebook",
        );
    }
}

/// Link out to the selected person on the web: their profile, where friend
/// management and everything else the backend cannot do lives. Uses the
/// profile URL learned from their posts when known, else a people search.
fn open_person_web(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    if ui.state.borrow().view == View::Groups {
        let url = record.field(1).to_owned();
        if !url.is_empty() {
            open_url(&url);
        }
        return;
    }
    let view = ui.state.borrow().view.clone();
    let name = match view {
        View::Friends => record.field(0).to_owned(),
        View::Chats => record.field(3).to_owned(),
        _ => record.field(1).to_owned(),
    };
    if name.is_empty() {
        return;
    }
    let known = ui.state.borrow().author_urls.get(&name).cloned();
    match known {
        Some(url) => open_url(&url),
        None => {
            let query = name.replace(' ', "%20");
            open_url(&format!("https://www.facebook.com/search/people/?q={query}"));
        }
    }
}

/// Link out to the selected chat in Messenger on the web, which is where
/// voice and video calls live. Deep-links the thread when the person's
/// profile URL is known, else opens the Messenger inbox.
fn open_chat_web(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    let name = match &view {
        View::ChatWith(name) => name.clone(),
        View::Chats => match selected(ui) {
            Some((_, record)) => record.field(3).to_owned(),
            None => return,
        },
        _ => {
            show_warning(&ui.frame, "Select or open a chat first.", "Open in Messenger");
            return;
        }
    };
    let thread = ui
        .state
        .borrow()
        .author_urls
        .get(&name)
        .and_then(|profile| thread_url_from_profile_url(profile));
    match thread {
        Some(url) => open_url(&url),
        None => open_url("https://www.facebook.com/messages/"),
    }
}

fn delete_selected(ui: &Ui) {
    let Some(post) = selected_post_ref(ui) else {
        show_warning(&ui.frame, "Select one of your posts first.", "Delete post");
        return;
    };
    if confirm(
        &ui.frame,
        "Delete this post? Facebook moves it to the trash (permanent removal after 30 days).",
        "Delete post",
    ) {
        ui.send(Job { task: Task::Fb(delete_args(&post), None), tag: Tag::Text });
    }
}

fn attach_files(ui: &Ui) {
    let dialog = FileDialog::builder(&ui.frame).build();
    if dialog.show_modal() == ID_OK {
        if let Some(path) = dialog.get_path() {
            let mut state = ui.state.borrow_mut();
            state.attachments.push(path);
            ui.status(&format!("{} file(s) attached", state.attachments.len()));
        }
    }
}

fn send_composed(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    let View::ChatWith(chat) = view else {
        show_warning(&ui.frame, "Open a chat first, then write and send.", "Send");
        return;
    };
    let text = ui.compose_box.get_value();
    let files = ui.state.borrow().attachments.clone();
    if text.trim().is_empty() && files.is_empty() {
        return;
    }
    let account = ui.state.borrow().account;
    let submitted = ChatSubmission { account, chat: chat.clone(), text: text.clone(), files: files.clone() };
    if !ChatSubmission::begin(&mut ui.state.borrow_mut().sending, submitted) {
        ui.status("Wait for your message to finish sending.");
        return;
    }
    ui.send(Job {
        task: Task::Fb(send_args(&chat, &text, &files), None),
        tag: Tag::ChatSent(account),
    });
}

fn handle_menu(ui: &Ui, id: i32) {
    match id {
        ID_EXIT => ui.frame.close(true),
        ID_FEED => load_view(ui, View::Feed),
        ID_NOTIFS => load_view(ui, View::Notifications),
        ID_FRIENDS => load_view(ui, View::Friends),
        ID_MYPOSTS => load_view(ui, View::MyPosts),
        ID_CHATS => load_view(ui, View::Chats),
        ID_GROUPS_VIEW => load_view(ui, View::Groups),
        ID_RECENT_FEED => load_view(ui, View::RecentFeed),
        ID_REELS => {
            load_view(ui, View::Reels(ReelSource::Recommended));
            ui.items.set_focus();
        }
        ID_WEB_POST => open_post_web(ui),
        ID_WEB_PERSON => open_person_web(ui),
        ID_WEB_CHAT => open_chat_web(ui),
        ID_UNREAD_CHATS => load_args(ui, View::Chats, chats_args(PAGE, true)),
        ID_REFRESH => refresh(ui),
        ID_MORE => load_more(ui),
        ID_OPEN => open_selected(ui),
        ID_OPEN_LINK => open_facebook_link(ui),
        ID_COMMENT => comment_selected(ui),
        ID_SHARE => {
            if let Some(post) = post_or_warn(ui, "Share") {
                match blindfb_core::posts::reader_url(&post) {
                    Some(url) => post_dialog::open(ui, &url, true),
                    None => show_warning(
                        &ui.frame,
                        "BlindFB cannot share this kind of item yet. Open it on Facebook to share it.",
                        "Share post",
                    ),
                }
            }
        }
        ID_PROFILE => profile_selected(ui),
        ID_AUTHOR_POSTS => posts_selected(ui),
        ID_MESSAGE => message_selected(ui),
        ID_DELETE => delete_selected(ui),
        ID_COPY_TEXT => {
            if let Some((_, record)) = selected(ui) {
                let text = display_for(&ui.state.borrow().view, &record);
                copy_text(ui, &text, "Text copied.");
            }
        }
        ID_COPY_LINK => match selected_link(ui) {
            Some(link) => copy_text(ui, &link, "Link copied."),
            None => show_warning(&ui.frame, "This item has no link to copy.", "Copy link"),
        },
        ID_NEW_POST => new_post(ui, false),
        ID_NEW_POST_IMAGES => new_post(ui, true),
        ID_SEARCH => search_dialog(ui),
        ID_OCR => ocr_dialog(ui),
        ID_UPDATE => update::check(&ui.frame, ship_shape::ui::CheckTrigger::Manual),
        ID_STATUS => ui.send(Job { task: Task::Fb(vec!["status".into()], None), tag: Tag::Text }),
        ID_STOP_DAEMON => {
            ui.send(Job { task: Task::Fb(vec!["stop".into()], None), tag: Tag::Text })
        }
        ID_QUIT_DAEMON => {
            ui.send(Job { task: Task::Fb(vec!["quit".into()], None), tag: Tag::Text })
        }
        ID_INSTALL_BACKEND => {
            ui.status("Downloading the Facebook backend…");
            ui.send(Job { task: Task::InstallBackend, tag: Tag::Text });
        }
        ID_LOGIN => start_sign_in(ui, Tag::SetupLogin),
        ID_INSTALL_BROWSER => install_browser_menu(ui),
        ID_UNLOCK => {
            if !ui.state.borrow().unlocking {
                unlock_messenger(ui, None);
            }
        }
        ID_UNSEND => {
            let view = ui.state.borrow().view.clone();
            if let View::ChatWith(chat) = view {
                if confirm(
                    &ui.frame,
                    "Remove your newest message in this chat for everyone?",
                    "Unsend",
                ) {
                    ui.send(Job {
                        task: Task::Fb(unsend_args(&chat, None), None),
                        tag: Tag::ReloadChat(chat, ui.state.borrow().account),
                    });
                }
            } else {
                show_warning(&ui.frame, "Open a chat first.", "Unsend");
            }
        }
        ID_KEYS => {
            let dialog = MessageDialog::builder(
                &ui.frame,
                "Ctrl+O: open a Facebook post, video or reel link.\n\
                 Enter on an item: open it. Applications or Shift+F10: item actions.\n\
                 Ctrl+1 through Ctrl+8: switch views. Ctrl+8 opens Reels. F5: refresh. Ctrl+L: load more.\n\
                 Reels stay cached until F5. Arrows change the selected reel; Enter opens it and starts playback automatically. Escape returns to the list.\n\
                 Tab from Items to Links in selected item; Enter opens its selected link. External links come first and Full post on Facebook is last.\n\
                 The post reader starts on Post links. Applications or Shift+F10 on a post includes a Links submenu. Tab reaches post text, video controls, comments and the composer.\n\
                 Enter on a selected link or Open link opens your browser.\n\
                 Tab to Play/Pause, Stop, Back 10 seconds or Forward 10 seconds, then press Space.\n\
                 Arrow keys on Video volume change its percentage.\n\
                 Applications or Shift+F10 on post text: comment, react or share. On a comment: reply or like.\n\
                 Ctrl+Enter in the comment box sends. Escape closes the reader and stops playback.",
                "Keyboard commands",
            ).build();
            dialog.show_modal();
            dialog.destroy();
        }
        ID_ABOUT => {
            AboutBoxBuilder::new(&ui.frame)
                .name("BlindFB")
                .version(env!("CARGO_PKG_VERSION"))
                .description(
                    "An accessible Facebook and Messenger client for screen reader users.\n\
                     Built with wxDragon on the facebook-cli (fb) backend.",
                )
                .copyright("Copyright (c) 2026 Brandon Bracey")
                .website("https://github.com/serrebidev/BlindFB")
                .licence("MIT")
                .add_developer("Brandon Bracey")
                .show();
        }
        ID_GITHUB => open_url("https://github.com/serrebidev/BlindFB"),
        _ => {
            if (ID_REACT_BASE..ID_REACT_BASE + REACTIONS.len() as i32).contains(&id) {
                if let Some(post) = post_or_warn(ui, "React") {
                    let reaction = REACTIONS[(id - ID_REACT_BASE) as usize];
                    react_to(ui, &post, reaction);
                }
            }
        }
    }
}

fn copy_text(ui: &Ui, text: &str, done: &str) {
    if Clipboard::get().set_text(text) {
        ui.status(done);
    } else {
        show_error(&ui.frame, "Could not copy to the clipboard.", "Copy");
    }
}

/// The selected item's web link, when it has one (short refs have none).
fn selected_link(ui: &Ui) -> Option<String> {
    let (_, record) = selected(ui)?;
    let view = ui.state.borrow().view.clone();
    let target = match view {
        View::Notifications => record.field(3),
        View::Groups => record.field(1),
        _ => record.field(0),
    };
    (target.starts_with("http") || target.starts_with('/')).then(|| web_url(target))
}

/// Applications / Shift+F10 on an item. The ids are the menu bar's, so `handle_menu` runs them.
fn show_item_menu(ui: &Ui) {
    if selected(ui).is_none() {
        return;
    }
    let view = ui.state.borrow().view.clone();
    let mut menu = Menu::builder().build();
    let add = |menu: &Menu, items: &[(i32, &str)]| {
        for &(id, label) in items {
            if id == 0 {
                menu.append_separator();
            } else {
                menu.append(id, label, "", ItemKind::Normal);
            }
        }
    };
    match view {
        View::Feed | View::RecentFeed | View::Reels(_) | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            add(&menu, &[
                (ID_OPEN, "&Open post and comments"),
                (ID_SHARE, "S&hare…"),
                (ID_COMMENT, "&Comment…"),
            ]);
            let react = Menu::builder().build();
            for (i, reaction) in REACTIONS.iter().enumerate() {
                let label = if *reaction == "none" {
                    "Remove reaction".to_owned()
                } else {
                    reaction[..1].to_uppercase() + &reaction[1..]
                };
                react.append(ID_REACT_BASE + i as i32, &label, "", ItemKind::Normal);
            }
            menu.append_submenu(react, "&React", "");
            append_links_menu(&menu, &ui.link_targets.borrow());
            add(
                &menu,
                &[
                    (0, ""),
                    (ID_PROFILE, "Author &profile"),
                    (ID_AUTHOR_POSTS, "Author's p&osts"),
                    (ID_MESSAGE, "&Message author"),
                    (0, ""),
                    (ID_COPY_TEXT, "Cop&y text"),
                    (ID_COPY_LINK, "Copy &link"),
                    (ID_WEB_POST, "Open post on &Facebook (web)"),
                    (ID_WEB_PERSON, "Open author on Fa&cebook (web)"),
                ],
            );
            if view == View::MyPosts {
                add(&menu, &[(0, ""), (ID_DELETE, "&Delete post…")]);
            }
        }
        View::Notifications => add(
            &menu,
            &[
                (ID_OPEN, "&Open"),
                (ID_SHARE, "S&hare…"),
                (ID_COPY_TEXT, "Cop&y text"),
                (ID_COPY_LINK, "Copy &link"),
                (ID_WEB_POST, "Open on &Facebook (web)"),
            ],
        ),
        View::Friends => add(
            &menu,
            &[
                (ID_PROFILE, "&Profile"),
                (ID_AUTHOR_POSTS, "P&osts"),
                (ID_MESSAGE, "&Message"),
                (ID_WEB_PERSON, "Open on &Facebook (web)"),
            ],
        ),
        View::Chats => {
            add(&menu, &[(ID_OPEN, "&Open chat"), (ID_WEB_CHAT, "Open in &Messenger (web)")])
        }
        View::ChatWith(_) => add(
            &menu,
            &[
                (ID_COPY_TEXT, "Cop&y message"),
                (ID_UNSEND, "&Unsend my last message…"),
                (ID_WEB_CHAT, "Open in &Messenger (web)"),
            ],
        ),
        View::Groups => add(
            &menu,
            &[
                (ID_OPEN, "&Open group posts"),
                (ID_COPY_LINK, "Copy &link"),
                (ID_WEB_PERSON, "Open group on &Facebook (web)"),
            ],
        ),
        _ => add(&menu, &[(ID_COPY_TEXT, "Cop&y text")]),
    }
    ui.items.popup_menu(&mut menu, None);
}

fn new_post(ui: &Ui, with_images: bool) {
    let Some(text) = prompt_text(&ui.frame, "Post text:", "New post") else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let mut images: Vec<String> = Vec::new();
    if with_images {
        let dialog = FileDialog::builder(&ui.frame).build();
        if dialog.show_modal() != ID_OK {
            return;
        }
        let Some(path) = dialog.get_path() else { return };
        images.push(path);
    }
    let labels = ["Public", "Friends", "Only me"];
    let dialog =
        SingleChoiceDialog::builder(&ui.frame, "Who can see this?", "Audience", &labels).build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    let audience = match dialog.get_selection() {
        0 => Some("public"),
        1 => Some("friends"),
        2 => Some("me"),
        _ => return,
    };
    ui.send(Job { task: Task::Fb(post_args(&text, audience, &images), None), tag: Tag::Text });
}

fn search_dialog(ui: &Ui) {
    let Some(query) = prompt_text(&ui.frame, "Search for:", "Search Facebook") else {
        return;
    };
    if query.trim().is_empty() {
        return;
    }
    let labels = ["Everything", "People", "Pages", "Groups", "Posts", "Events"];
    let dialog =
        SingleChoiceDialog::builder(&ui.frame, "Search in:", "Search Facebook", &labels).build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    let kind = match dialog.get_selection() {
        0 => None,
        1 => Some("people"),
        2 => Some("pages"),
        3 => Some("groups"),
        4 => Some("posts"),
        5 => Some("events"),
        _ => return,
    };
    load_args(ui, View::SearchResults, search_args(&query, kind, PAGE));
}

fn ocr_dialog(ui: &Ui) {
    if !native_ocr::is_available() {
        show_warning(
            &ui.frame,
            "No OCR engine is available on this system. On Linux, install tesseract-ocr.",
            "OCR an image",
        );
        return;
    }
    let dialog = FileDialog::builder(&ui.frame).build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    let Some(path) = dialog.get_path() else {
        return;
    };
    let done_tx = ui.done_tx.clone();
    ui.status("Reading image text…");
    thread::spawn(move || {
        let result = ocr_file(&path);
        let _ = done_tx.send(Done { tag: Tag::Text, result });
    });
}

fn ocr_file(path: &str) -> Result<String, String> {
    let img = image::open(PathBuf::from(path)).map_err(|e| format!("could not open image: {e}"))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    native_ocr::recognize_rgba(rgba.as_raw(), width, height).map_err(|e| format!("OCR failed: {e}"))
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reels_with_matching_captions_keep_their_distinct_urls() {
        let view = View::Reels(ReelSource::Recommended);
        assert!(cacheable(&view, &[]));
        let (rows, _) = reels::parse(r#"{"rows":[{"url":"https://www.facebook.com/reel/1/","author":"A","text":"Same"},{"url":"https://www.facebook.com/reel/2/","author":"A","text":"Same"}]}"#).unwrap();
        assert_eq!(post_target(&view, &rows[0]), Some("https://www.facebook.com/reel/1/"));
        assert_eq!(merge_records(&view, rows.clone(), rows).len(), 2);
    }

    #[test]
    fn text_links_trim_punctuation_keep_balanced_urls_and_reject_other_schemes() {
        let links = links_in_text(
            "Article (https://example.org/story). https://example.org/a_(b), http://example.net/x?y=1&z=2. \"https://example.org/story\" https:// file:///secret javascript:alert(1) https://bad\\host/x https://example.net/a⏎https://example.net/b [https://[::1]]",
            None,
        );
        let urls: Vec<_> = links.iter().map(|link| link.url.as_str()).collect();
        assert_eq!(urls, vec!["https://example.org/story", "https://example.org/a_(b)", "http://example.net/x?y=1&z=2", "https://example.net/a", "https://example.net/b", "https://[::1]"]);
        let post = "https://www.facebook.com/a/posts/1";
        let links = links_in_text(&format!("{post}/ {post} https://example.org/story"), Some(post));
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "https://example.org/story");
        assert_eq!(links[1].label, "Full post on Facebook");
        assert_eq!(links[1].url, post);
    }

    #[test]
    fn account_reset_rejects_old_reads_without_reusing_request_numbers() {
        let mut requests = ListRequests::default();
        let old = requests.begin(&View::Chats);
        requests.invalidate();
        assert!(!requests.current(&View::Chats, old));
        assert!(!requests.finish(&View::Chats, old));
        assert!(requests.pending.is_empty());
        let new = requests.begin(&View::Chats);
        assert_ne!(old, new);
        assert!(!requests.finish(&View::Chats, old));
        assert!(requests.pending.contains(&View::Chats));
        assert!(requests.finish(&View::Chats, new));
    }

    #[test]
    fn prefetch_is_a_pending_section_request_until_it_finishes() {
        let mut requests = ListRequests::default();
        let prefetch = requests.begin(&View::Friends);
        assert!(requests.pending.contains(&View::Friends));
        requests.begin(&View::Feed);
        assert!(requests.current(&View::Friends, prefetch));
        assert!(requests.finish(&View::Friends, prefetch));
        assert!(!requests.pending.contains(&View::Friends));
    }

    #[test]
    fn refreshing_search_preserves_query_kind_and_count() {
        let args = search_args("a specific query", Some("posts"), PAGE * 2);
        assert_eq!(refresh_args(&View::SearchResults, &args), Some(args));
        assert_eq!(refresh_args(&View::SearchResults, &[]), None);
        assert_eq!(refresh_args(&View::Chats, &[]), default_args(&View::Chats));
    }

    #[test]
    fn sending_blocks_duplicates_until_completion() {
        let message = ChatSubmission { account: 0, chat: "Alice".into(), text: "Hi".into(), files: vec![] };
        let mut pending = None;
        assert!(ChatSubmission::begin(&mut pending, message.clone()));
        assert!(!ChatSubmission::begin(&mut pending, message.clone()));
        pending.take();
        assert!(ChatSubmission::begin(&mut pending, message));
    }

    #[test]
    fn old_account_completion_cannot_take_new_accounts_pending_message() {
        let mut pending = Some(ChatSubmission {
            account: 1, chat: "Bob".into(), text: "New account draft".into(), files: vec![],
        });
        assert!(ChatSubmission::finish(&mut pending, 0).is_none());
        assert_eq!(pending.as_ref().unwrap().text, "New account draft");
        assert!(ChatSubmission::finish(&mut pending, 1).is_some());
        assert!(pending.is_none());
    }

    #[test]
    fn sent_message_preserves_new_text_files_and_other_chat_drafts() {
        let message = ChatSubmission {
            account: 0, chat: "Alice".into(), text: "First".into(), files: vec!["first.png".into()],
        };
        let mut files = vec!["first.png".into(), "next.png".into()];
        assert!(!message.clear_sent(&View::ChatWith("Bob".into()), "First", &mut files));
        assert_eq!(files, ["next.png"]);
        assert!(!message.clear_sent(&View::ChatWith("Alice".into()), "Next draft", &mut files));
        assert_eq!(files, ["next.png"]);
        assert!(message.clear_sent(&View::ChatWith("Alice".into()), "First", &mut files));
        assert_eq!(files, ["next.png"]);
    }

    #[test]
    fn section_requests_survive_navigation_and_reject_superseded_results() {
        let mut requests = ListRequests::default();
        let append = requests.begin(&View::Feed);
        requests.begin(&View::Notifications);
        // Returning to a cached feed submits no replacement request.
        assert!(requests.current(&View::Feed, append));
        assert!(requests.pending.contains(&View::Feed));
        assert!(requests.finish(&View::Feed, append));
        assert!(!requests.pending.contains(&View::Feed));

        let old = requests.begin(&View::Feed);
        let fresh = requests.begin(&View::Feed);
        requests.begin(&View::Notifications);
        assert!(!requests.finish(&View::Feed, old));
        assert!(requests.pending.contains(&View::Feed));
        assert!(requests.finish(&View::Feed, fresh));
        assert!(!requests.finish(&View::Feed, old));
        assert!(requests.current(&View::Feed, fresh));
    }

    #[test]
    fn more_uses_feed_cursor_and_preserves_section_filters() {
        assert_eq!(more_args(&View::Feed, &feed_args(PAGE, true, true, true), PAGE as usize, false),
            Some(feed_args(PAGE, false, true, true)));
        assert_eq!(more_args(&View::Chats, &chats_args(PAGE, true), 38, false),
            Some(chats_args(PAGE * 2, true)));
        assert_eq!(more_args(&View::SearchResults, &search_args("rust", Some("posts"), PAGE), 40, false),
            Some(search_args("rust", Some("posts"), PAGE * 2)));
        assert_eq!(more_args(&View::Notifications, &notifs_args(PAGE), 40, false),
            Some(notifs_args(PAGE * 2)));
        assert_eq!(more_args(&View::Groups, &groups_eval_args(), 40, false), None);
        assert_eq!(more_args(&View::Feed, &[], 0, false), None);
        assert_eq!(more_args(&View::SearchResults, &search_args("new query", Some("posts"), PAGE), 40, true), None);
    }

    #[test]
    fn more_appends_new_posts_without_reordering_existing_rows() {
        let old = parse_feed("/a/posts/1\tA\tnow\t0/0/0\tFirst\n-\tAd\tnow\t0/0/0\tAd text");
        let new = parse_feed("/a/posts/1\tA\tlater\t1/0/0\tFirst\n-\tAd\tnow\t0/0/0\tAd text\n/a/posts/2\tA\tnow\t0/0/0\tSecond");
        let merged = merge_records(&View::Feed, old, new);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].field(0), "/a/posts/1");
        assert_eq!(merged[1].field(0), "-");
        assert_eq!(record_key(&View::Feed, &merged[1]), merged[1].raw);
        assert_eq!(merged[2].field(0), "/a/posts/2");
    }

    #[test]
    fn more_deduplicates_alternate_photo_links_across_pages() {
        let old = parse_feed("/photo/?fbid=1&set=a.9\tJeremy Smith\tnow\t0/0/0\tSame photo post");
        let new = parse_feed("/photo/?fbid=2&set=a.9\tJeremy Smith\tnow\t0/0/0\tSame photo post");
        assert_eq!(merge_records(&View::Feed, old, new).len(), 1);
    }

    #[test]
    fn linkless_rows_say_so_without_touching_copied_text() {
        let row = |fields: &[&str]| Record {
            fields: fields.iter().map(|f| f.to_string()).collect(),
            raw: String::new(),
        };
        let linked = row(&["/a/posts/1", "Alice", "5 min", "1/2/3", "Hello"]);
        let ad = row(&["-", "Sponsor", "", "0/0/0", "Buy things"]);
        assert!(list_label(&View::Feed, &linked).contains("Hello"));
        assert!(!list_label(&View::Feed, &linked).contains("(no link)"));
        assert!(list_label(&View::Feed, &ad).ends_with("(no link)"));
        assert!(list_label(&View::RecentFeed, &ad).ends_with("(no link)"));
        // Copy text keeps the plain label: the marker is a list affordance only.
        assert!(!display_for(&View::Feed, &ad).contains("(no link)"));
        // Chats are not post rows, so they never get the marker.
        assert!(!list_label(&View::Chats, &row(&["c1", "-", "5m", "Bea", "hi"])).contains("no link"));
        assert_eq!(display_for(&View::Feed, &linked), "Alice. Hello. 5 min");
        assert_eq!(display_for(&View::Feed, &row(&["/a/posts/2", "Carol", "1m", "0/0/0", ""])), "Carol. 1m");
        assert_eq!(display_for(&View::Feed, &row(&["/a/posts/3", "Carol", "", "0/0/0", ""])), "Carol");
    }

    #[test]
    fn routes_only_post_targets_and_detects_messenger_lock() {
        let row = |fields: &[&str]| Record {
            fields: fields.iter().map(|f| f.to_string()).collect(),
            raw: String::new(),
        };
        let url = "https://www.facebook.com/a/posts/1";
        assert_eq!(post_target(&View::Notifications, &row(&["1h", "!", "x", url])), Some(url));
        assert_eq!(post_target(&View::Notifications, &row(&["1h", "!", "x", "2h"])), None);
        assert_eq!(post_target(&View::Feed, &row(&["p3k9a", "A"])), Some("p3k9a"));
        assert_eq!(post_target(&View::Feed, &row(&["-", "Ad"])), None);
        assert_eq!(post_target(&View::Chats, &row(&[url])), None);
        assert!(needs_messenger_unlock(
            "Error: Messenger history prompt blocked input; use `fb unlock`"
        ));
        assert!(!needs_messenger_unlock("network error"));
    }
}
