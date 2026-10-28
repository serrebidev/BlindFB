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

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use blindfb_core::browsers::{self, Browser, BrowserKind};
use blindfb_core::fb::{
    Fb, Record, chats_args, comment_args, delete_args, feed_args, friends_args, groups_eval_args,
    groups_open_args, notifs_args, parse_feed, parse_groups_json, parse_notifications,
    parse_records, post_args, posts_args, profile_args, profile_url_from_post_url, react_args,
    read_args, recent_feed_args, search_args, send_args, thread_url_from_profile_url, unsend_args,
    view_args, web_url,
};
use blindfb_core::update;
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
const ID_REACT_BASE: i32 = 10030; // + index into REACTIONS
const ID_COMMENT: i32 = 10040;
const ID_PROFILE: i32 = 10041;
const ID_AUTHOR_POSTS: i32 = 10042;
const ID_MESSAGE: i32 = 10043;
const ID_DELETE: i32 = 10044;
const ID_COPY_TEXT: i32 = 10045;
const ID_COPY_LINK: i32 = 10046;
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
/// A cached section younger than this is shown without asking Facebook again.
const FRESH: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum View {
    Feed,
    RecentFeed,
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
    },
    /// Background read that only fills the section cache.
    Prefetch(View),
    Text,
    ReloadChat(String),
    UpdateCheck,
    GroupsOpened(u64),
    UnlockRetry {
        view: View,
        request: u64,
        args: Vec<String>,
    },
    PostLoaded(u64),
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
    list_request: u64,
    last_args: Vec<String>,
    pending_unlock: Option<RetryList>,
    unlocking: bool,
    records: Vec<Record>,
    attachments: Vec<String>,
    /// Author name -> profile URL, learned from post URLs as views load.
    /// Powers the "open on Facebook" link-outs (profiles, Messenger web).
    author_urls: HashMap<String, String>,
    /// Last raw backend output per section, so switching back is instant.
    cache: HashMap<View, (String, Instant)>,
    prefetched: bool,
    /// The list request refreshing a cached section the user is already reading.
    background: Option<u64>,
}

struct RetryList {
    view: View,
    request: u64,
    args: Vec<String>,
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
    startup_log("main: exited");
}

fn run() {
    startup_log("run: building the window");
    let fb = Fb::discover();
    let (job_tx, job_rx) = channel::<Job>();
    let (done_tx, done_rx) = channel::<Done>();
    let latest_list = Arc::new(AtomicU64::new(0));
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
    ] {
        views.append(name);
    }
    views.set_selection(0, true);

    let items_label = StaticText::builder(&panel).with_label("Items").build();
    let items = ListBox::builder(&panel).with_size(Size::new(-1, 260)).build();

    let details_label = StaticText::builder(&panel).with_label("Details").build();
    let details = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .with_size(Size::new(-1, 170))
        .build();

    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    let refresh_btn = Button::builder(&panel).with_label("Refresh (F5)").build();
    let open_btn = Button::builder(&panel).with_label("Open").build();
    let react_btn = Button::builder(&panel).with_label("React").build();
    let comment_btn = Button::builder(&panel).with_label("Comment").build();
    let profile_btn = Button::builder(&panel).with_label("Profile").build();
    let posts_btn = Button::builder(&panel).with_label("Posts").build();
    let message_btn = Button::builder(&panel).with_label("Message").build();
    let delete_btn = Button::builder(&panel).with_label("Delete post").build();
    for b in [
        &refresh_btn,
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
        list_request: 0,
        last_args: Vec::new(),
        pending_unlock: None,
        unlocking: false,
        records: Vec::new(),
        attachments: Vec::new(),
        author_urls: HashMap::new(),
        cache: HashMap::new(),
        prefetched: false,
        background: None,
    }));
    let ui = Ui {
        frame,
        items,
        details,
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
                _ => View::Chats,
            };
            load_view(&ui_views, view);
        }
    });

    let ui_sel = ui.clone();
    items.on_selection_changed(move |event| {
        if let Some(sel) = event.get_selection() {
            show_details(&ui_sel, sel as usize);
        }
    });
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
    startup_log("run: window shown");
}

#[derive(Clone)]
struct Ui {
    frame: Frame,
    items: ListBox,
    details: TextCtrl,
    compose_box: TextCtrl,
    state: Rc<RefCell<State>>,
    jobs: Sender<Job>,
    done_tx: Sender<Done>,
    post_reader: Rc<RefCell<Option<post_dialog::PostReader>>>,
    /// The list request the user is waiting for; the worker skips older ones.
    latest_list: Arc<AtomicU64>,
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
    latest_list: Arc<AtomicU64>,
) {
    thread::spawn(move || {
        let mut fb = fb;
        while let Ok(job) = jobs.recv() {
            // Arrowing through Views queues a load per section; run only the one still wanted.
            if let Tag::List { request, .. } | Tag::GroupsOpened(request) = &job.tag {
                if *request != latest_list.load(Ordering::Relaxed) {
                    continue;
                }
            }
            let read = matches!(job.tag, Tag::List { .. } | Tag::Prefetch(_))
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
            let result = match job.task {
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
        .append_item(ID_UNREAD_CHATS, "&Unread chats only", "Chats with unread messages")
        .append_separator()
        .append_item(ID_REFRESH, "&Refresh\tF5", "Reload the current view")
        .build();

    let mut post_menu = Menu::builder()
        .append_item(ID_OPEN, "&Open post / comments", "Open the selected item; Enter in the items list")
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
            "Download the newest facebook-cli backend from GitHub",
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

fn begin_list(ui: &Ui, view: &View) -> u64 {
    let mut state = ui.state.borrow_mut();
    let clear = state.records_view != *view;
    if clear {
        state.records.clear();
    }
    state.view = view.clone();
    state.list_request = state.list_request.wrapping_add(1);
    let request = state.list_request;
    ui.latest_list.store(request, Ordering::Relaxed);
    drop(state);
    if clear {
        ui.items.clear();
        ui.details.clear();
    }
    request
}

fn current_list(ui: &Ui, view: &View, request: u64) -> bool {
    let state = ui.state.borrow();
    state.list_request == request && state.view == *view
}

fn load_args(ui: &Ui, view: View, args: Vec<String>) {
    let request = begin_list(ui, &view);
    ui.state.borrow_mut().last_args = args.clone();
    ui.send(Job {
        task: Task::Fb(args.clone(), None),
        tag: Tag::List { view: view.clone(), request, args, unlock_attempted: false },
    });
    ui.status(&format!("Loading {}…", view.title()));
}

fn load_view(ui: &Ui, view: View) {
    let cached = ui.state.borrow().cache.get(&view).cloned();
    if let Some((raw, at)) = &cached {
        begin_list(ui, &view);
        show_list(ui, &view, raw);
        // The News feed pages forward on every load, so it waits for F5.
        if view == View::Feed || at.elapsed() < FRESH {
            return;
        }
    }
    if view == View::Groups {
        load_groups(ui);
    } else if let Some(args) = default_args(&view) {
        load_args(ui, view, args);
    }
    if cached.is_some() {
        let mut state = ui.state.borrow_mut();
        state.background = Some(state.list_request);
    }
}

/// Read every main section in the background once, so first visits are instant too.
fn prefetch(ui: &Ui) {
    for view in [View::RecentFeed, View::Notifications, View::Friends, View::MyPosts, View::Chats] {
        if let Some(args) = default_args(&view) {
            let _ = ui.jobs.send(Job { task: Task::Fb(args, None), tag: Tag::Prefetch(view) });
        }
    }
}

fn cacheable(view: &View, args: &[String]) -> bool {
    matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::Notifications
            | View::Friends
            | View::MyPosts
            | View::Chats
            | View::Groups
    ) && !args.iter().any(|a| a == "--unread")
}

fn default_args(view: &View) -> Option<Vec<String>> {
    Some(match view {
        View::Feed => feed_args(PAGE, false, true, true),
        View::RecentFeed => recent_feed_args(PAGE),
        View::Notifications => notifs_args(PAGE),
        View::Friends => friends_args(None, 100),
        View::MyPosts => posts_args("me", PAGE, true),
        View::Chats => chats_args(PAGE, false),
        View::Groups | View::SearchResults => return None,
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
    {
        let mut state = ui.state.borrow_mut();
        state.cache.clear();
        state.prefetched = false;
    }
    match choose_sign_in(ui) {
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
    match view {
        View::Feed => load_args(ui, view, feed_args(PAGE, true, true, true)),
        View::Groups => load_groups(ui),
        _ => {
            if let Some(args) = default_args(&view) {
                load_args(ui, view, args);
            }
        }
    }
}

fn display_for(view: &View, record: &Record) -> String {
    match view {
        View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            if record.fields.len() >= 5 {
                let time = record.field(2);
                format!(
                    "{}. {}{}{}",
                    record.field(1),
                    one_line(&record.fields[4..].join(" ")),
                    if time.is_empty() { "" } else { ". " },
                    time
                )
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
        View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
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
        View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
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
            _ => record.field(0),
        })
        .to_owned()
}

fn show_details(ui: &Ui, index: usize) {
    let state = ui.state.borrow();
    if let Some(record) = state.records.get(index) {
        let text = details_for(&state.view, record);
        drop(state);
        ui.details.set_value(&text);
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
fn show_list(ui: &Ui, view: &View, raw: &str) -> bool {
    let (records, body) = match parse_for(view, raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            ui.status("Could not read this section.");
            show_error(&ui.frame, error, "BlindFB");
            return false;
        }
    };
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
            state.records.iter().map(|r| display_for(&state.records_view, r)).collect();
        let key = old_index
            .and_then(|i| state.records.get(i as usize))
            .map(|r| record_key(&state.records_view, r))
            .filter(|k| !k.is_empty() && k != "-");
        (labels, key)
    };
    let labels: Vec<_> = records.iter().map(|r| display_for(view, r)).collect();
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
        ui.details.set_value(&body);
    } else if ui.items.get_count() > 0 {
        show_details(ui, selected.unwrap_or(0));
    } else {
        ui.details.set_value("(nothing here)");
    }
    ui.status(&format!("{} — {} item(s)", view.title(), ui.items.get_count()));
    true
}

fn apply_done(ui: &Ui, done: Done) {
    match done.tag {
        Tag::List { view, request, args, unlock_attempted } => {
            let current = current_list(ui, &view, request);
            match done.result {
                Ok(raw) => {
                    if cacheable(&view, &args) && parse_for(&view, &raw).is_ok() {
                        ui.state
                            .borrow_mut()
                            .cache
                            .insert(view.clone(), (raw.clone(), Instant::now()));
                    }
                    if !current {
                        return;
                    }
                    if show_list(ui, &view, &raw)
                        && view == View::Feed
                        && !ui.state.borrow().prefetched
                    {
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
                            state.pending_unlock = Some(RetryList { view, request, args });
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
        Tag::Prefetch(view) => {
            let Ok(raw) = done.result else { return };
            if parse_for(&view, &raw).is_err() {
                return;
            }
            ui.state.borrow_mut().cache.insert(view.clone(), (raw.clone(), Instant::now()));
            // The user opened this section before its prefetch finished: show it now.
            let waiting = {
                let state = ui.state.borrow();
                state.view == view && state.records_view != view
            };
            if waiting {
                show_list(ui, &view, &raw);
            }
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
        Tag::UnlockRetry { view, request, args } => {
            ui.state.borrow_mut().unlocking = false;
            match done.result {
                Ok(_) if !args.is_empty() && current_list(ui, &view, request) => {
                    ui.send(Job {
                        task: Task::Fb(args.clone(), None),
                        tag: Tag::List { view, request, args, unlock_attempted: true },
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
                ui.details.set_value(text.trim());
                ui.status("Done");
            }
            Err(e) => {
                ui.status("Error");
                show_error(&ui.frame, &e, "BlindFB");
            }
        },
        Tag::ReloadChat(chat) => match done.result {
            Ok(_) => {
                ui.compose_box.clear();
                ui.state.borrow_mut().attachments.clear();
                ui.status("Sent");
                load_view(ui, View::ChatWith(chat));
            }
            Err(e) => {
                ui.status("Error");
                show_error(&ui.frame, &e, "BlindFB");
            }
        },
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
                ui.details.set_value(out.trim());
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
            if current_list(ui, &View::Groups, request) {
                let args = groups_eval_args();
                ui.send(Job {
                    task: Task::Fb(args.clone(), None),
                    tag: Tag::List { view: View::Groups, request, args, unlock_attempted: false },
                });
            }
        }
        Tag::UpdateCheck => match done.result {
            Ok(payload) => {
                let mut parts = payload.splitn(2, '\n');
                let version = parts.next().unwrap_or_default().to_owned();
                let url = parts.next().unwrap_or(update::RELEASES_PAGE).to_owned();
                if update::is_newer(env!("CARGO_PKG_VERSION"), &version) {
                    ui.status(&format!("BlindFB {version} is available"));
                    if confirm(
                        &ui.frame,
                        &format!(
                            "BlindFB {version} is available (you have {}). Open the download page?",
                            env!("CARGO_PKG_VERSION")
                        ),
                        "Update available",
                    ) {
                        open_url(&url);
                    }
                } else {
                    ui.status("BlindFB is up to date");
                    show_warning(
                        &ui.frame,
                        format!("You have the latest BlindFB ({}).", env!("CARGO_PKG_VERSION")),
                        "Check for updates",
                    );
                }
            }
            Err(e) => show_error(&ui.frame, &e, "Check for updates"),
        },
    }
}

fn parse_for(view: &View, raw: &str) -> Result<(Vec<Record>, Option<String>), String> {
    Ok(match view {
        View::Notifications => (parse_notifications(raw)?, None),
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
    });
    ui.state.borrow_mut().unlocking = true;
    ui.status("Unlocking Messenger…");
    ui.send(Job {
        task: Task::Fb(vec!["unlock".into(), "-".into()], Some(format!("{pin}\n"))),
        tag: Tag::UnlockRetry { view: retry.view, request: retry.request, args: retry.args },
    });
}

fn open_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    match view {
        View::Feed
        | View::RecentFeed
        | View::MyPosts
        | View::PostsOf(_)
        | View::SearchResults
        | View::Notifications => {
            let Some(target) = post_target(&view, &record) else {
                show_warning(
                    &ui.frame,
                    "Facebook hides this post's link, so BlindFB cannot open it yet. Ads and suggestions often have none.",
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
                Some(url) => post_dialog::open(ui, &url),
                None => {
                    // Reels, photo albums and other pages the reader cannot parse yet.
                    if confirm(
                        &ui.frame,
                        "BlindFB cannot read this kind of item yet (for example a reel or a photo album). Open it in your web browser?",
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
            "Facebook hides this post's link, so BlindFB cannot open, react to or comment on it yet."
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
    if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        if index >= 0 {
            let reaction = REACTIONS[index as usize];
            ui.send(Job { task: Task::Fb(react_args(&post, reaction), None), tag: Tag::Text });
        }
    }
}

fn comment_selected(ui: &Ui) {
    let Some(post) = post_or_warn(ui, "Comment") else {
        return;
    };
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
    ui.send(Job {
        task: Task::Fb(send_args(&chat, &text, &files), None),
        tag: Tag::ReloadChat(chat),
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
        ID_WEB_POST => open_post_web(ui),
        ID_WEB_PERSON => open_person_web(ui),
        ID_WEB_CHAT => open_chat_web(ui),
        ID_UNREAD_CHATS => load_args(ui, View::Chats, chats_args(PAGE, true)),
        ID_REFRESH => refresh(ui),
        ID_OPEN => open_selected(ui),
        ID_COMMENT => comment_selected(ui),
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
        ID_UPDATE => check_updates(ui),
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
        ID_LOGIN => start_sign_in(ui, Tag::Text),
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
                        tag: Tag::ReloadChat(chat),
                    });
                }
            } else {
                show_warning(&ui.frame, "Open a chat first.", "Unsend");
            }
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
                    ui.send(Job {
                        task: Task::Fb(react_args(&post, reaction), None),
                        tag: Tag::Text,
                    });
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
        View::Feed | View::RecentFeed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            add(&menu, &[(ID_OPEN, "&Open post and comments"), (ID_COMMENT, "&Comment…")]);
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
        if dialog.show_modal() == ID_OK {
            if let Some(path) = dialog.get_path() {
                images.push(path);
            }
        }
    }
    let labels = ["Public", "Friends", "Only me"];
    let dialog =
        SingleChoiceDialog::builder(&ui.frame, "Who can see this?", "Audience", &labels).build();
    let mut audience = None;
    if dialog.show_modal() == ID_OK {
        audience = match dialog.get_selection() {
            0 => Some("public"),
            1 => Some("friends"),
            2 => Some("me"),
            _ => None,
        };
    }
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
    let mut kind = None;
    if dialog.show_modal() == ID_OK {
        kind = match dialog.get_selection() {
            1 => Some("people"),
            2 => Some("pages"),
            3 => Some("groups"),
            4 => Some("posts"),
            5 => Some("events"),
            _ => None,
        };
    }
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

fn check_updates(ui: &Ui) {
    let done_tx = ui.done_tx.clone();
    ui.status("Checking for updates…");
    thread::spawn(move || {
        let result = update::latest_release().map(|r| format!("{}\n{}", r.version, r.url));
        let _ = done_tx.send(Done { tag: Tag::UpdateCheck, result });
    });
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

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
