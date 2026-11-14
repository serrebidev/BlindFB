// Release builds are Windows GUI-subsystem programs: no
// console window opens beside the app (Brandon's order,
// 2026-11-06, the same fix as the other apps). Debug builds
// keep the console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

mod chat_dialog;
mod group_dialog;
mod player;
mod participation_dialog;
mod post_dialog;
mod reel_cache;
mod shortcuts_dialog;
mod type_ahead;
mod update;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use blindfb_core::alerts;
use blindfb_core::browsers::{self, Browser, BrowserKind};
use blindfb_core::fb::{
    chats_args, comment_args, delete_args, events_args, facebook_share_url, feed_args,
    friend_add_args, friends_args, group_apply_args, group_join_args, group_leave_args,
    groups_args, link_url, listing_message_args, marketplace_args, msg_react_args, notifs_args,
    pages_args, parse_feed, parse_notifications, parse_records, places_args, post_args,
    post_done_message, post_unconfirmed_message,
    posts_args, profile_args,
    profile_url_from_post_url, react_args, read_args, recent_feed_args, reply_args, search_args,
    saved_args, sell_args, send_args, thread_url_from_profile_url, unsend_args, view_args,
    web_url, Fb, Record,
};
use blindfb_core::posts::Link;
use blindfb_core::reels::{self, ReelSource};
use wx_utils::{confirm, prompt_text, show_error, show_warning, AboutBoxBuilder};
use wxdragon::prelude::*;

use blindfb_core::i18n::{self, Language};
use blindfb_core::tray_hotkey;
use blindfb_core::{ntf, tf};
use patois::t;

// The translations in app/locale, compiled by build.rs and baked into the binary.
patois::embed_domain!();
// wxWidgets' own words (OK, Cancel, file dialogs…) for the same languages.
patois::embed_wx_translations!();

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
const ID_MARKETPLACE: i32 = 10018;
const ID_EVENTS_VIEW: i32 = 10097;
const ID_PAGES_VIEW: i32 = 10098;
const ID_PLACES_VIEW: i32 = 10099;
const ID_SAVED_VIEW: i32 = 10131;
const ID_TRAY_TOGGLE: i32 = 10110;
const ID_TRAY_HOTKEY: i32 = 10111;
const ID_GROUP_POSTS: i32 = 10112;
const ID_TRAY_MENU_TOGGLE: i32 = 10201;
const ID_TRAY_MENU_EXIT: i32 = 10202;
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
const ID_POST_MENU: i32 = 10130;
const ID_UNSEND: i32 = 10050;
const ID_MSG_REPLY: i32 = 10090;
const ID_MSG_REACT_BASE: i32 = 10100; // + index into REACTIONS
const ID_UNLOCK: i32 = 10051;
const ID_STATUS: i32 = 10060;
const ID_LOGIN: i32 = 10061;
const ID_INSTALL_BACKEND: i32 = 10083;
const ID_INSTALL_BROWSER: i32 = 10084;
const ID_COMMENT_FRIEND: i32 = 10085;
const ID_GROUP_JOIN: i32 = 10086;
const ID_GROUP_LEAVE: i32 = 10087;
const ID_LISTING_MESSAGE: i32 = 10088;
const ID_SELL: i32 = 10089;
const ID_LISTING_OPEN: i32 = 10092;
const ID_SHARE_LINK: i32 = 10093;
const ID_STOP_DAEMON: i32 = 10062;
const ID_QUIT_DAEMON: i32 = 10063;
const ID_ABOUT: i32 = 10070;
const ID_GITHUB: i32 = 10071;
const ID_DEBUG_LOGGING: i32 = 10073;
const ID_DEBUG_REPORT: i32 = 10074;
const ID_VOICE_ALERTS: i32 = 10075;
const ID_SOUND_ALERTS: i32 = 10076;
const ID_LIST_VOICE_ALERTS: i32 = 10077;
const ID_LIST_SOUND_ALERTS: i32 = 10078;
const ID_LANG_SYSTEM: i32 = 10094;
const ID_LANG_ENGLISH: i32 = 10095;
const ID_LANG_FRENCH: i32 = 10096;
const ID_LANG_RUSSIAN: i32 = 10120;
const ID_LANG_ESTONIAN: i32 = 10121;
const ID_SHORTCUTS: i32 = 10122;

const REACTIONS: [&str; 8] = [
    "like", "love", "care", "haha", "wow", "sad", "angry", "none",
];
/// The Post menu's item for a `REACTIONS` keyword, one whole label per
/// reaction so a translation never has to agree with a separate word.
fn react_menu_label(reaction: &str) -> String {
    match reaction {
        "like" => t("React: like"),
        "love" => t("React: love"),
        "care" => t("React: care"),
        "haha" => t("React: haha"),
        "wow" => t("React: wow"),
        "sad" => t("React: sad"),
        "angry" => t("React: angry"),
        "none" => t("React: none"),
        other => other.to_owned(),
    }
}

/// A `REACTIONS` keyword as it reads in the React choice list and in
/// "Reacting … to: …" (lower case in English).
fn reaction_name(reaction: &str) -> String {
    match reaction {
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "like" => t("like"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "love" => t("love"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "care" => t("care"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "haha" => t("haha"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "wow" => t("wow"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "sad" => t("sad"),
        // TRANSLATORS: a reaction name in a list of reactions (lower case in English).
        "angry" => t("angry"),
        // TRANSLATORS: the choice that removes your reaction, in a list of reactions.
        "none" => t("none"),
        other => other.to_owned(),
    }
}

/// A `REACTIONS` keyword as a context-menu item.
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
        other => other.to_owned(),
    }
}

/// One page of a list section. The backend scrolls a live Facebook page per
/// item it has to add, so asking for 50 costs several times what asking for
/// 10 does; Load more grows the request from here.
const PAGE: u32 = 10;
/// The News feed opens with this many posts, then reads ahead while the user reads.
const FEED_FIRST: u32 = 10;
/// The News feed's read-ahead batch, asked for only near the end of the list.
/// Facebook keeps just the last ~25 posts of its feed page mounted, and a
/// post's Facebook menu (Interested / Not interested) needs its card still
/// there, so the backend must not scroll far past the post being read.
const FEED_AHEAD: u32 = 10;
/// Rows from the end of the News feed list that start the next read-ahead.
const FEED_NEAR_END: usize = 3;
/// Most Recent reads ahead by the same batch: without it every Load more
/// re-scrolls from the top in the foreground and gets slower as the list
/// grows, which is why it felt far slower than the News feed.
const RECENT_AHEAD: u32 = 30;
/// Groups read ahead like the feed: the first page paints, then the rest of
/// the list (40 groups read in about 4 s) fills in without Load more.
const GROUPS_AHEAD: u32 = 100;
/// Your own posts are few and the section is expensive to scroll, so it opens small.
const MY_POSTS_FIRST: u32 = 5;
/// A cached section younger than this is shown without asking Facebook again.
const FRESH: Duration = Duration::from_secs(60);
/// A section picked in the Views list is read once the selection rests this
/// long, so arrowing from News feed to Chats does not read every section in
/// between and make Chats wait for them. Long enough for a held arrow key to
/// run its course, short enough that a deliberate pick never feels late.
const VIEW_REST: Duration = Duration::from_millis(400);
/// How often alerts check Facebook while they are on. Each check opens two
/// live pages, so this stays gentle; it never runs while alerts are off.
const ALERT_EVERY: Duration = Duration::from_secs(120);
/// The first check waits out the launch burst of section reads.
const ALERT_FIRST: Duration = Duration::from_secs(30);

/// The section waiting out the Views-list debounce, once it has rested.
fn rested_view(pending: Option<(View, Instant)>, rest: Duration) -> Option<View> {
    pending
        .filter(|(_, at)| at.elapsed() >= rest)
        .map(|(view, _)| view)
}

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
    Marketplace,
    Events,
    Pages,
    Places,
    /// Your saved posts, read like the feed's posts: Enter opens the
    /// reader, and react, comment, share and the post menu all work.
    Saved,
    SearchResults,
    PostsOf(String),
    /// A group's posts, by group home URL (`https://www.facebook.com/groups/<id>`).
    GroupPosts(String),
    CommentsOf(String),
    ProfileOf(String),
    /// A conversation, identified by its backend ref (never its display
    /// name: names repeat, and a presence label is not a name). The second
    /// field is the display name, for the title and the web link-out.
    ChatWith(String, String),
}

impl View {
    fn title(&self) -> String {
        match self {
            View::Feed => t("News feed"),
            View::RecentFeed => t("Feeds (most recent)"),
            View::Reels(source) => source.title(),
            View::Notifications => t("Notifications"),
            View::Friends => t("Friends"),
            View::MyPosts => t("My posts"),
            View::Chats => t("Chats"),
            View::Groups => t("My groups"),
            View::Marketplace => t("Marketplace"),
            View::Events => t("Events"),
            View::Pages => t("Pages"),
            View::Places => t("Places"),
            View::Saved => t("Saved posts"),
            View::SearchResults => t("Search results"),
            View::PostsOf(who) => tf!("Posts by {who}", who = who),
            View::GroupPosts(group) => tf!("Posts in group {group}", group = group),
            View::CommentsOf(_) => t("Post and comments"),
            View::ProfileOf(who) => tf!("Profile of {who}", who = who),
            View::ChatWith(_, name) => tf!("Chat with {name}", name = name),
        }
    }
}

enum Tag {
    Post,
    List {
        view: View,
        request: u64,
        args: Vec<String>,
        unlock_attempted: bool,
        append: bool,
    },
    Text,
    /// A comment sent from the list views: success must say so, not "Done",
    /// and a comments view showing that post refreshes to show it (issue #22).
    Commented {
        post: String,
    },
    DebugReport,
    AlertNotifications,
    AlertChats,
    /// A group join finished: its output decides whether membership
    /// questions need answers (the output starts with a questions line).
    GroupJoin {
        group: String,
    },
    /// A group leave finished: refresh the joined-groups list if it is on
    /// screen, since the group is gone from it now.
    GroupLeave,
    ReloadChat(String, String, u64),
    ChatSent(u64),
    UnlockRetry {
        view: View,
        request: u64,
        args: Vec<String>,
        append: bool,
    },
    PostLoaded(u64),
    GroupPosts(u64),
    GroupMembers(u64),
    ChatMessages(u64),
    ChatWindowSent(u64),
    ReelPrepared {
        account: u64,
        target: String,
    },
    PostVideo(u64, u64),
    PostReelReacted(u64),
    PostSent(u64),
    PostParticipation(u64, bool),
    PostLiked(u64),
    PostShareReady(u64),
    PostShared(u64),
    PostNote(u64),
    PostShareClosed(u64),
    LoginRequest,
    LoginAnswered,
    /// A step of Facebook's own post menu: the labels pressed so far.
    PostMenu {
        post: String,
        path: Vec<String>,
    },
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
    ReelsLoad {
        source: ReelSource,
        append: bool,
    },
    /// Run the backend with these args (and optional stdin, for the PIN).
    Fb(Vec<String>, Option<String>),
    /// Write the debug report to this path.
    DebugReport(String),
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
    PostParticipation {
        page: String,
        token: String,
        answers: Option<serde_json::Value>,
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
    /// Open a "Someone's trying to log in" request and read it.
    LoginRead(String),
    /// Press this Facebook answer on the open login request.
    LoginAnswer(String),
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
    next_alert_check: Instant,
    alert_notifications: blindfb_core::alerts::Watch,
    alert_chats: blindfb_core::alerts::Watch,
    view: View,
    records_view: View,
    last_args: Vec<String>,
    pending_unlock: Option<RetryList>,
    unlocking: bool,
    records: Vec<Record>,
    attachments: Vec<String>,
    /// (Text, --nth) of the message the next send replies to (quotes),
    /// from the message context menu's Reply.
    reply_to: Option<(String, usize)>,
    /// A selection whose details pane update is waiting out the debounce:
    /// arrowing fast should not rebuild the details and links per row.
    pending_details: Option<(usize, Instant)>,
    /// A section picked in the Views list whose read is waiting out the
    /// debounce: arrowing down the list passes over sections, and each one
    /// passed over used to start a read that the resting one waited behind.
    pending_view: Option<(View, Instant)>,
    sending: Option<ChatSubmission>,
    account: u64,
    reel_cache: reel_cache::ReelCache,
    /// Author name -> profile URL, learned from post URLs as views load.
    /// Powers the "open on Facebook" link-outs (profiles, Messenger web).
    author_urls: HashMap<String, String>,
    /// Last parsed backend output and request per section, so switching back is instant.
    cache: HashMap<View, (Vec<Record>, Instant, Vec<String>)>,
    /// Where Backspace/Escape in the list returns to, most recent last.
    history: Vec<View>,
    /// Each list's selected record key, remembered when the list is left,
    /// so coming back (Back, or returning from a post's comments) puts the
    /// cursor on the item the user was on, not the first item (issue #15).
    selection_memory: HashMap<View, String>,
    /// Set while a Back navigation loads, so it does not push itself.
    back_nav: bool,
    prefetched: bool,
    /// The list request refreshing a cached section the user is already reading.
    background: Option<u64>,
}

#[derive(Default)]
struct ListRequests {
    next: u64,
    latest: HashMap<View, u64>,
    pending: HashSet<View>,
    /// Sections being read without the user waiting for them (launch prefetch,
    /// the feed's read-ahead). They give up the backend's read slots to any
    /// read the user just asked for.
    background: HashSet<View>,
}

impl ListRequests {
    /// A read the user is waiting for: it outranks background work.
    fn begin(&mut self, view: &View) -> u64 {
        self.next = self.next.wrapping_add(1);
        self.latest.insert(view.clone(), self.next);
        self.pending.insert(view.clone());
        self.background.remove(view);
        self.next
    }

    /// A read nobody is waiting for yet.
    fn begin_background(&mut self, view: &View) -> u64 {
        let request = self.begin(view);
        self.background.insert(view.clone());
        request
    }

    fn current(&self, view: &View, request: u64) -> bool {
        self.latest.get(view) == Some(&request)
    }

    fn finish(&mut self, view: &View, request: u64) -> bool {
        if !self.current(view, request) {
            return false;
        }
        self.pending.remove(view);
        self.background.remove(view);
        true
    }

    fn invalidate(&mut self) {
        self.latest.clear();
        self.pending.clear();
        self.background.clear();
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
    name: String,
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
        matches!(view, View::ChatWith(id, _) if id == &self.chat) && text == self.text
    }

    fn finish(pending: &mut Option<Self>, account: u64) -> Option<Self> {
        if pending
            .as_ref()
            .is_some_and(|message| message.account == account)
        {
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
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("startup.log"))
    {
        use std::io::Write;
        let _ = writeln!(file, "[{}] {line}", std::process::id());
    }
}

fn main() {
    startup_log("main: starting");
    // Before any window: every label below is looked up in the chosen language.
    i18n::apply(i18n::saved());
    std::panic::set_hook(Box::new(|info| {
        startup_log(&format!("panic: {info}"));
    }));
    let _ = wxdragon::main(|_| {
        run();
    });
    startup_log("main: window loop returned; closing browser pages");
    if let Some(fb) = Fb::discover() {
        reels::close_all(&fb);
    }
    startup_log("main: exited");
}

fn run() {
    // wxWidgets' own buttons and dialogs follow the chosen language too.
    let locale = i18n::saved().locale();
    let language = locale.split('_').next().unwrap_or("en");
    if language != "en" {
        let translations = wxdragon::translations::Translations::new();
        translations.set_loader(WxStdCatalogLoader);
        translations.set_language_str(language);
        translations.add_std_catalog();
        wxdragon::translations::Translations::set_global(translations);
    }
    startup_log("run: building the window");
    let fb = Fb::discover();
    let (job_tx, job_rx) = channel::<Job>();
    let (done_tx, done_rx) = channel::<Done>();
    let latest_list = Arc::new(Mutex::new(ListRequests::default()));
    spawn_worker(fb.clone(), job_rx, done_tx.clone(), latest_list.clone());

    let frame = Frame::builder()
        .with_title("BlindFB")
        .with_size(Size::new(1100, 760))
        .build();
    frame.create_status_bar(1, 0, -1, "status");
    frame.set_status_text(
        &if fb.is_some() {
            t("Ready. Pick a view to start.")
        } else {
            t("Setting up the Facebook backend…")
        },
        0,
    );

    let panel = Panel::builder(&frame).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let views_label = StaticText::builder(&panel).with_label(&t("Views")).build();
    let views = ListBox::builder(&panel).build();
    for name in [
        t("News feed"),
        t("Notifications"),
        t("Friends"),
        t("My posts"),
        t("Chats"),
        t("Groups"),
        t("Feeds (most recent)"),
        t("Reels"),
        t("Marketplace"),
        t("Events"),
        t("Pages"),
        t("Places"),
        t("Saved posts"),
    ] {
        views.append(&name);
    }
    views.set_selection(0, true);

    let items_label = StaticText::builder(&panel).with_label(&t("Items")).build();
    let items = ListBox::builder(&panel)
        .with_size(Size::new(-1, 260))
        .build();

    let links_label = StaticText::builder(&panel)
        .with_label(&t("Lin&ks in selected item"))
        .build();
    let links = ListBox::builder(&panel)
        .with_size(Size::new(-1, 70))
        .build();
    links.set_name(&t("Links in selected item. Enter opens the selected link in your browser"));
    let open_link = Button::builder(&panel)
        .with_label(&t("Open selected &link"))
        .build();
    open_link.enable(false);

    let details_label = StaticText::builder(&panel).with_label(&t("Details")).build();
    let details = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .with_size(Size::new(-1, 170))
        .build();

    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    let refresh_btn = Button::builder(&panel).with_label(&t("Refresh (F5)")).build();
    let open_btn = Button::builder(&panel)
        .with_label(&t("Open / read full post and links"))
        .build();
    let react_btn = Button::builder(&panel).with_label(&t("React")).build();
    let comment_btn = Button::builder(&panel).with_label(&t("Comment")).build();
    let profile_btn = Button::builder(&panel).with_label(&t("Profile")).build();
    let posts_btn = Button::builder(&panel).with_label(&t("Posts")).build();
    let message_btn = Button::builder(&panel).with_label(&t("Message")).build();
    let delete_btn = Button::builder(&panel).with_label(&t("Delete post")).build();
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
    let compose_label = StaticText::builder(&panel).with_label(&t("Message")).build();
    let compose_box = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    let attach_btn = Button::builder(&panel).with_label(&t("Attach files")).build();
    let attachments_btn = Button::builder(&panel)
        .with_label(&tf!("Attachments ({count})", count = 0))
        .build();
    attachments_btn.set_name(&ntf!("Attachments, {n} attached", "Attachments, {n} attached", 0));
    let send_btn = Button::builder(&panel).with_label(&t("Send")).build();
    compose.add(
        &compose_label,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::All,
        3,
    );
    compose.add(&compose_box, 1, SizerFlag::Expand | SizerFlag::All, 3);
    compose.add(&attach_btn, 0, SizerFlag::All, 3);
    compose.add(&attachments_btn, 0, SizerFlag::All, 3);
    compose.add(&send_btn, 0, SizerFlag::All, 3);

    root.add(&views_label, 0, SizerFlag::All, 4);
    root.add(
        &views,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        4,
    );
    root.add(&items_label, 0, SizerFlag::All, 4);
    root.add(
        &items,
        1,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        4,
    );
    root.add(&links_label, 0, SizerFlag::All, 4);
    root.add(
        &links,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        4,
    );
    root.add(&open_link, 0, SizerFlag::All, 4);
    root.add(&details_label, 0, SizerFlag::All, 4);
    root.add(
        &details,
        0,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        4,
    );
    root.add_sizer(&actions, 0, SizerFlag::Expand, 2);
    root.add_sizer(&compose, 0, SizerFlag::Expand, 2);
    panel.set_sizer(root, true);

    build_menus(&frame);
    startup_log("run: menus built");

    // System tray (issue #26). The icon lives as long as the app, so it
    // is intentionally leaked like the timers below. Its menu toggles
    // the window or exits; double-clicking the icon also toggles it
    // (Windows/Linux — macOS has no tray mouse events, its menu is the
    // restore path there). Artwork is a plain blue square placeholder,
    // the same approach as BlindRSS's tray icon.
    // The menu is leaked for the same reason: set_popup_menu keeps only
    // a reference and wxDragon's Menu destroys its wxMenu on drop, so a
    // menu owned by this stack frame was freed when run() returned and
    // opening the tray menu crashed the app (issue #26, Michal's report).
    let tray = Box::leak(Box::new(TaskBarIcon::builder().build()));
    {
        let mut rgba = Vec::with_capacity(32 * 32 * 4);
        for _ in 0..32 * 32 {
            rgba.extend_from_slice(&[30, 100, 200, 255]);
        }
        if let Some(bmp) = Bitmap::from_rgba(&rgba, 32, 32) {
            tray.set_icon(&bmp, "BlindFB");
        }
    }
    let tray_menu: &mut Menu = Box::leak(Box::new(
        Menu::builder()
            .append_item(ID_TRAY_MENU_TOGGLE, &t("Show / hide BlindFB"), "")
            .append_separator()
            .append_item(ID_TRAY_MENU_EXIT, &t("Exit BlindFB"), "")
            .build(),
    ));
    tray.set_popup_menu(tray_menu);
    {
        let frame_tray = frame;
        tray.on_menu(move |event| match event.get_id() {
            ID_TRAY_MENU_TOGGLE => toggle_tray_window(&frame_tray),
            ID_TRAY_MENU_EXIT => frame_tray.close(true),
            _ => {}
        });
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let frame_tray = frame;
        tray.on_left_double_click(move |_| toggle_tray_window(&frame_tray));
    }
    // Windows: the show/hide hotkey must also restore, and a menu accelerator cannot
    // reach a hidden window — register it system-wide (issue #26).
    #[cfg(target_os = "windows")]
    spawn_tray_hotkey_thread();

    let state = Rc::new(RefCell::new(State {
        next_alert_check: Instant::now() + ALERT_FIRST,
        alert_notifications: Default::default(),
        alert_chats: Default::default(),
        view: View::Feed,
        records_view: View::Feed,
        last_args: Vec::new(),
        pending_unlock: None,
        unlocking: false,
        records: Vec::new(),
        attachments: Vec::new(),
        reply_to: None,
        pending_details: None,
        pending_view: None,
        sending: None,
        account: 0,
        reel_cache: reel_cache::ReelCache::default(),
        author_urls: HashMap::new(),
        cache: HashMap::new(),
        history: Vec::new(),
        selection_memory: HashMap::new(),
        back_nav: false,
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
        attachments_btn,
        open_btn,
        react_btn,
        comment_btn,
        profile_btn,
        posts_btn,
        message_btn,
        delete_btn,
        attach_btn,
        send_btn,
        state: state.clone(),
        jobs: job_tx.clone(),
        done_tx,
        post_reader: Rc::new(RefCell::new(None)),
        group_window: Arc::new(Mutex::new(None)),
        chat_window: Arc::new(Mutex::new(None)),
        latest_list,
    };

    // First-run setup: check the backend status. If the backend is missing
    // entirely, the SetupStatus result sends the installer; if it is there
    // but signed out, setup offers the browser sign-in. Signed in already:
    // straight to the feed.
    ui.status(&t("Checking the Facebook backend…"));
    ui.send(Job {
        task: Task::Fb(vec!["status".into()], None),
        tag: Tag::SetupStatus,
    });

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
                8 => View::Marketplace,
                9 => View::Events,
                10 => View::Pages,
                11 => View::Places,
                12 => View::Saved,
                _ => View::Chats,
            };
            ui_views.state.borrow_mut().pending_view = Some((view, Instant::now()));
        }
    });

    let ui_sel = ui.clone();
    items.on_selection_changed(move |event| {
        if let Some(sel) = event.get_selection() {
            item_selected(&ui_sel, sel as u32);
        }
    });
    let ui_typed = ui.clone();
    type_ahead::attach(&items, move |sel| item_selected(&ui_typed, sel));
    let ui_links = ui.clone();
    links.on_selection_changed(move |_| {
        ui_links
            .open_link
            .enable(ui_links.links.get_selection().is_some());
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
            if (ui_enter.items.has_focus() || ui_enter.links.has_focus())
                && matches!(
                    key.get_key_code(),
                    Some(wxdragon::keycode::WXK_BACK | wxdragon::keycode::WXK_ESCAPE)
                )
            {
                event.skip(false);
                go_back(&ui_enter);
                return;
            }
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
    let ui_attachments = ui.clone();
    attachments_btn.on_click(move |_| manage_attachments(&ui_attachments));
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
        #[cfg(target_os = "windows")]
        if TRAY_HOTKEY_PENDING.swap(false, std::sync::atomic::Ordering::SeqCst) {
            toggle_tray_window(&ui_tick.frame);
        }
        let due = {
            let state = ui_tick.state.borrow();
            state
                .pending_details
                .filter(|(_, at)| at.elapsed() >= Duration::from_millis(160))
                .map(|(index, _)| index)
        };
        if let Some(index) = due {
            ui_tick.state.borrow_mut().pending_details = None;
            if ui_tick.items.get_selection() == Some(index as u32) {
                show_details(&ui_tick, index);
            }
        }
        let rested = rested_view(ui_tick.state.borrow().pending_view.clone(), VIEW_REST);
        if let Some(view) = rested {
            ui_tick.state.borrow_mut().pending_view = None;
            load_view(&ui_tick, view);
        }
        check_alerts_when_due(&ui_tick);
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
    frame.bind_internal(EventType::CLOSE_WINDOW, move |event| {
        startup_log("window: close requested");
        event.skip(true);
    });
    let tray_destroy: &TaskBarIcon = tray;
    frame.on_destroy(move |_| {
        startup_log("window: stopping timers before destruction");
        timer.stop();
        unlock_timer.stop();
        // Take the tray icon down with the window; a leaked icon whose
        // app has exited lingers in the tray as a ghost (issue #26).
        tray_destroy.remove_icon();
        // The tray icon's hidden window counts as an open window, so wx can keep running
        // with nothing on screen: a closed BlindFB lingered as a process, holding
        // blindfb.exe so the updater could not replace it. End the loop outright.
        if let Some(app) = wxdragon::app::get_app_instance() {
            app.exit_main_loop();
        }
    });

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
    attachments_btn: Button,
    open_btn: Button,
    react_btn: Button,
    comment_btn: Button,
    profile_btn: Button,
    posts_btn: Button,
    message_btn: Button,
    delete_btn: Button,
    attach_btn: Button,
    send_btn: Button,
    state: Rc<RefCell<State>>,
    jobs: Sender<Job>,
    done_tx: Sender<Done>,
    post_reader: Rc<RefCell<Option<post_dialog::PostReader>>>,
    group_window: Arc<Mutex<Option<group_dialog::GroupWindow>>>,
    chat_window: Arc<Mutex<Option<chat_dialog::ChatWindow>>>,
    /// The list request the user is waiting for; the worker skips older ones.
    latest_list: Arc<Mutex<ListRequests>>,
}

impl Ui {
    fn status(&self, text: &str) {
        self.frame.set_status_text(text, 0);
    }
    fn send(&self, job: Job) {
        self.status(&t("Working…"));
        let _ = self.jobs.send(job);
    }
}

/// Bounds concurrent backend reads. Every read is a tab on a live
/// Facebook page in the backend's Chrome, and the launch burst (the
/// first feed page, its read-ahead, and the section prefetch)
/// otherwise runs them all at once and pins the CPU.
///
/// Slots are handed to reads the user is waiting for first: a background
/// read never takes the last free slot while a foreground read is queued,
/// so opening a section never waits behind a prefetch that was already
/// running. Background reads also leave one slot free for the next foreground read.
struct ReadGate {
    state: Mutex<GateState>,
    freed: std::sync::Condvar,
    max: usize,
}

#[derive(Default)]
struct GateState {
    taken: usize,
    /// Foreground reads waiting for a slot.
    waiting: usize,
}

/// Whether a read may take a slot now. A background read stands aside while
/// anything the user is waiting for is queued.
fn gate_admits(taken: usize, max: usize, waiting: usize, background: bool) -> bool {
    taken < max && (!background || (waiting == 0 && taken < max.saturating_sub(1)))
}

impl ReadGate {
    fn new(max: usize) -> Self {
        Self {
            state: Mutex::new(GateState::default()),
            freed: std::sync::Condvar::new(),
            max,
        }
    }
    fn acquire(&self, mut background: impl FnMut() -> bool) -> ReadPermit<'_> {
        let mut state = self.state.lock().unwrap();
        let mut foreground = false;
        loop {
            let now_foreground = !background();
            if now_foreground != foreground {
                if now_foreground {
                    state.waiting += 1;
                } else {
                    state.waiting -= 1;
                }
                foreground = now_foreground;
            }
            if gate_admits(state.taken, self.max, state.waiting, !foreground) {
                if foreground {
                    state.waiting -= 1;
                }
                state.taken += 1;
                return ReadPermit { gate: self };
            }
            // Opening a prefetched section can promote it while it waits for a slot.
            state = self
                .freed
                .wait_timeout(state, Duration::from_millis(100))
                .unwrap()
                .0;
        }
    }
}

struct ReadPermit<'a> {
    gate: &'a ReadGate,
}

impl Drop for ReadPermit<'_> {
    fn drop(&mut self) {
        self.gate.state.lock().unwrap().taken -= 1;
        self.gate.freed.notify_all();
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
        let read_gate = std::sync::Arc::new(ReadGate::new(2));
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
                _ => None,
            };
            if let Some((view, request)) = list {
                if !latest_list.lock().unwrap().current(view, request) {
                    continue;
                }
            }
            // Group window reads count too: run inline they held the worker for a
            // minute (50 posts + 500 members), so a post opened from the group waited.
            let read = matches!(
                job.tag,
                Tag::List { .. }
                    | Tag::AlertNotifications
                    | Tag::AlertChats
                    | Tag::GroupPosts(_)
                    | Tag::GroupMembers(_)
            ) && matches!(job.task, Task::Fb(_, None));
            if read && fb.is_none() {
                fb = Fb::discover();
            }
            if let (true, Some(fb), Task::Fb(args, _)) = (read, fb.clone(), &job.task) {
                // The backend gives every command its own tab, so reads can
                // run side by side — two at a time, so a burst of reads
                // cannot pin the CPU (see ReadGate). Background reads yield
                // those slots to whatever the user just opened.
                let (args, tag, done, gate) =
                    (args.clone(), job.tag, done.clone(), read_gate.clone());
                let requests = latest_list.clone();
                thread::spawn(move || {
                    let _permit = gate.acquire(|| match &tag {
                        Tag::List { view, .. } => {
                            requests.lock().unwrap().background.contains(view)
                        }
                        // Nobody is waiting on an alert check.
                        Tag::AlertNotifications | Tag::AlertChats => true,
                        _ => false,
                    });
                    if let Tag::List { view, request, .. } = &tag {
                        if !requests.lock().unwrap().current(view, *request) {
                            return;
                        }
                    }
                    let result = fb.run(&refs(&args));
                    let _ = done.send(Done { tag, result });
                });
                continue;
            }
            if let Task::ReelsLoad { source, append } = &job.task {
                let (fb, source, append, tag, done) =
                    (fb.clone(), source.clone(), *append, job.tag, done.clone());
                thread::spawn(move || {
                    let result = match fb {
                        Some(fb) => reels::load(&fb, &source, append),
                        None => Err(t("The Facebook backend is not installed")),
                    };
                    let _ = done.send(Done { tag, result });
                });
                continue;
            }
            let video = match &job.task {
                Task::PostVideo { page, target } => Some((page.clone(), target.clone())),
                _ => None,
            };
            if let Some((page, target)) = video {
                // Stream extraction/downloads must not delay comments, reactions or closing the page.
                let (fb, page, target, tag, done) =
                    (fb.clone(), page, target, job.tag, done.clone());
                thread::spawn(move || {
                    let result = match fb {
                        #[cfg(windows)]
                        Some(fb) => blindfb_core::mpv::ensure()
                            .and_then(|_| blindfb_core::media::stream_details(&fb, &page, &target)),
                        #[cfg(not(windows))]
                        Some(fb) => blindfb_core::media::stream_details(&fb, &page, &target),
                        None => Err(t("The Facebook backend is not installed")),
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
                        None => Err(t("the backend is not installed yet")),
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
                        None => Err(t(
                            "the fb backend is not installed — use Backend → Install or update backend"
                        )),
                        Some(fb) => match &stdin {
                            Some(input) => fb.run_stdin(&refs(&args), input),
                            None => fb.run(&refs(&args)),
                        },
                    }
                }
                Task::DebugReport(path) => blindfb_core::debug::write_report(fb.as_ref(), &path),
                Task::PostLoad { page, target } => match &fb {
                    Some(fb) => blindfb_core::posts::load(fb, &page, &target),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostVideo { .. } => unreachable!("video preparation runs independently"),
                Task::PostReelReact { page, reaction } => match &fb {
                    Some(fb) => blindfb_core::posts::reel_react(fb, &page, &reaction),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostRefresh { page } => match &fb {
                    Some(fb) => blindfb_core::posts::refresh(fb, &page),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostSend { page, parent, text } => match &fb {
                    Some(fb) => {
                        blindfb_core::posts::send_comment(fb, &page, parent.as_deref(), &text)
                    }
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostLike { page, id } => match &fb {
                    Some(fb) => blindfb_core::posts::like_comment(fb, &page, &id),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostParticipation { page, token, answers } => match &fb {
                    Some(fb) => blindfb_core::posts::participation(fb, &page, &token, answers.as_ref()),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostShareOpen { page } => match &fb {
                    Some(fb) => blindfb_core::posts::share_open(fb, &page),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostShare { page, text } => match &fb {
                    Some(fb) => blindfb_core::posts::share_now(fb, &page, &text),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::PostShareCancel { page } => match &fb {
                    Some(fb) => blindfb_core::posts::share_cancel(fb, &page),
                    None => Ok(String::new()),
                },
                Task::PostClose { page } => match &fb {
                    Some(fb) => blindfb_core::posts::close(fb, &page),
                    None => Ok(String::new()),
                },
                Task::LoginRead(target) => match &fb {
                    Some(fb) => blindfb_core::login_approval::read(fb, &target),
                    None => Err(t("The Facebook backend is not installed")),
                },
                Task::LoginAnswer(choice) => match &fb {
                    Some(fb) => blindfb_core::login_approval::answer(fb, &choice),
                    None => Err(t("The Facebook backend is not installed")),
                },
            };
            if done
                .send(Done {
                    tag: job.tag,
                    result,
                })
                .is_err()
            {
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
        SignIn::Browser(browser) if browser.uses_fb_login() => {
            // The backend's guided login copies the session from Chrome
            // itself over a debugging connection, so it survives Chrome's
            // app-bound cookie encryption, which a cookie-file import
            // cannot always read. The import stays as the fallback.
            match fb.run(&["login"]) {
                Ok(_) => {
                    let status = fb.run(&["status"])?;
                    if browsers::signed_in(&status) {
                        Ok(status)
                    } else {
                        visible_fallback(fb, t("the Chrome sign-in did not finish"))
                    }
                }
                Err(why) => match browsers::import_chromium(fb, &browser) {
                    Ok(status) => Ok(status),
                    Err(_) => visible_fallback(fb, why),
                },
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
        SignIn::Install(_) => Err(t("install the browser first")),
    }
}

/// Last resort for every import path: a visible sign-in window. Reports
/// the earlier failure when even that does not sign in.
fn visible_fallback(fb: &Fb, why: String) -> Result<String, String> {
    fb.run(&["login", "--visible"]).map_err(|error| {
        tf!(
            "Session import failed: {why}\n\nDirect sign-in failed: {error}",
            why = why,
            error = error
        )
    })?;
    let status = fb.run(&["status"])?;
    if browsers::signed_in(&status) {
        Ok(status)
    } else {
        Err(why)
    }
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// Win32 RegisterHotKey spec for the tray toggle (issue #26): the saved
/// choice from `blindfb_core::tray_hotkey` (default Alt+Ctrl+F), or `None`
/// when the user turned the hotkey off. Only the Windows hotkey thread
/// calls this in production on Windows; the app test pins the default
/// spec on every platform.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn tray_hotkey_spec() -> Option<(u32, u32)> {
    tray_hotkey::saved().win_spec()
}

/// Set by the Windows hotkey thread when the hotkey is pressed; the GUI
/// timer consumes it and toggles (issue #26).
#[cfg(target_os = "windows")]
static TRAY_HOTKEY_PENDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Register the show/hide hotkey as a system-wide hotkey on its own
/// thread (Windows, issue #26). A menu accelerator cannot reach a
/// hidden window, so the v0.7.31 shortcut hid BlindFB but never
/// restored it. RegisterHotKey with no window posts WM_HOTKEY to this
/// thread's queue; the flag above hands the press to the GUI timer,
/// which toggles on the GUI thread where wx calls belong.
/// The choice lives in the saved setting (default Alt+Ctrl+F) and is
/// re-read while the thread runs, so picking a new preset — or turning
/// the hotkey off — takes effect without restarting; the thread drops
/// the old registration and registers the new one.
#[cfg(target_os = "windows")]
fn spawn_tray_hotkey_thread() {
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterHotKey(hwnd: *mut c_void, id: i32, modifiers: u32, vk: u32) -> i32;
        fn UnregisterHotKey(hwnd: *mut c_void, id: i32) -> i32;
        fn PeekMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, flags: u32) -> i32;
        fn Sleep(dw_ms: u32);
    }
    const WM_HOTKEY: u32 = 0x0312;
    const PM_REMOVE: u32 = 0x0001;
    std::thread::spawn(move || unsafe {
        let mut registered: Option<(u32, u32)> = None;
        let mut msg = Msg {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wparam: 0,
            lparam: 0,
            time: 0,
            pt: Point { x: 0, y: 0 },
        };
        // A hotkey another app owns is retried every 5 s (100 ticks) and logged once, not
        // every tick: the log grew by 20 lines a second while another copy held it.
        let mut refused = None;
        let mut ticks: u32 = 0;
        loop {
            ticks = ticks.wrapping_add(1);
            let want = tray_hotkey_spec();
            if want != registered && (want != refused || ticks.is_multiple_of(100)) {
                if registered.is_some() {
                    UnregisterHotKey(std::ptr::null_mut(), 1);
                }
                match want {
                    Some((modifiers, vk)) => {
                        if RegisterHotKey(std::ptr::null_mut(), 1, modifiers, vk) == 0 {
                            if want != refused {
                                startup_log(
                                    "hotkey: the hotkey is owned by another app; it only works while BlindFB is focused",
                                );
                            }
                            refused = want;
                            registered = None;
                        } else {
                            startup_log("hotkey: registered system-wide for the tray toggle");
                            refused = None;
                            registered = want;
                        }
                    }
                    None => {
                        startup_log("hotkey: off; the tray icon and menus toggle");
                        registered = None;
                    }
                }
            }
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) > 0 {
                if msg.message == WM_HOTKEY {
                    TRAY_HOTKEY_PENDING.store(true, Ordering::SeqCst);
                }
            }
            Sleep(50);
        }
    });
}

/// The File menu label for the tray toggle, with the current hotkey
/// after the tab (nothing after it when the hotkey is off).
fn tray_toggle_text() -> String {
    match tray_hotkey::saved().label() {
        Some(label) => format!("{}\t{label}", t("Hide to system &tray")),
        None => t("Hide to system &tray"),
    }
}

/// Let the user pick the show/hide hotkey: the presets from
/// `tray_hotkey::Hotkey::choices` (default Alt+Ctrl+F) or off. The
/// choice is remembered and the Windows thread re-registers live.
fn prompt_tray_hotkey(ui: &Ui) {
    let choices = tray_hotkey::Hotkey::choices();
    let labels: Vec<String> = choices.iter().map(|(_, label)| label.clone()).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        &t("Show/hide hotkey"),
        &t("Pick the shortcut that hides and restores BlindFB, or turn it off"),
        &refs,
    )
    .build();
    let picked = (dialog.show_modal() == ID_OK).then(|| dialog.get_selection());
    dialog.destroy();
    let Some(index) = picked.filter(|index| *index >= 0) else {
        return;
    };
    let hotkey = choices[index as usize].0;
    match tray_hotkey::save(hotkey) {
        Ok(()) => {
            // The menu label is built once; the new shortcut shows in
            // the File menu from the next launch, but the Windows
            // thread re-registers it right away.
            let words = match hotkey.label() {
                Some(label) => tf!("Show/hide hotkey: {label}", label = label),
                None => t("Show/hide hotkey off."),
            };
            ui.status(&words);
        }
        Err(error) => show_error(&ui.frame, &error, &t("Show/hide hotkey")),
    }
}

/// Hide the window to the system tray, or bring it back (issue #26).
/// One toggle for both directions. On Windows the hotkey is registered
/// system-wide (see spawn_tray_hotkey_thread), so the same shortcut
/// restores a hidden window; elsewhere the tray icon's double-click or
/// its Show / hide menu item is the restore side.
fn toggle_tray_window(frame: &Frame) {
    if frame.is_shown() {
        frame.show(false);
    } else {
        frame.iconize(false);
        frame.show(true);
        frame.raise();
    }
}

fn build_menus(frame: &Frame) {
    let file_menu = Menu::builder()
        .append_item(
            ID_OPEN_LINK,
            &shortcuts_dialog::menu_label(ID_OPEN_LINK, t("Open Facebook &link…")),
            &t("Open a Facebook post, video or reel link"),
        )
        .append_item(ID_NEW_POST, &shortcuts_dialog::menu_label(ID_NEW_POST, t("&New post…")), &t("Write a new post"))
        .append_item(
            ID_NEW_POST_IMAGES,
            &shortcuts_dialog::menu_label(ID_NEW_POST_IMAGES, t("New post with &media…")),
            &t("Write a post with photos and videos attached"),
        )
        .append_item(
            ID_SELL,
            &shortcuts_dialog::menu_label(ID_SELL, t("Sell on &Marketplace…")),
            &t("Create a Marketplace listing with photos or video"),
        )
        .append_item(
            ID_SEARCH,
            &shortcuts_dialog::menu_label(ID_SEARCH, t("&Search Facebook…")),
            &t("Search everything: posts, people, friends, photos, videos, pages, places, groups, events and Marketplace"),
        )
        .append_item(
            ID_OCR,
            &shortcuts_dialog::menu_label(ID_OCR, t("&OCR an image…")),
            &t("Read the text in an image file with the operating system's OCR engine"),
        )
        .append_item(
            ID_UPDATE,
            &shortcuts_dialog::menu_label(ID_UPDATE, t("Check for &updates…")),
            &t("Check for a newer BlindFB"),
        )
        .append_separator()
        .append_item(
            ID_TRAY_TOGGLE,
            &tray_toggle_text(),
            &t("Hide BlindFB to the system tray; the tray icon or its menu brings it back"),
        )
        .append_item(
            ID_TRAY_HOTKEY,
            &shortcuts_dialog::menu_label(ID_TRAY_HOTKEY, t("Show/hide &hotkey…")),
            &t("Pick the shortcut that hides and restores BlindFB, or turn it off"),
        )
        .append_item(
            ID_SHORTCUTS,
            &shortcuts_dialog::menu_label(ID_SHORTCUTS, t("&Keyboard shortcuts…")),
            &t("Give any command a shortcut, or change or remove one"),
        )
        .append_item(ID_EXIT, &shortcuts_dialog::menu_label(ID_EXIT, t("E&xit")), &t("Close BlindFB"))
        .build();

    let view_menu = Menu::builder()
        .append_item(ID_FEED, &shortcuts_dialog::menu_label(ID_FEED, t("News &feed")), &t("Your news feed"))
        .append_item(ID_NOTIFS, &shortcuts_dialog::menu_label(ID_NOTIFS, t("&Notifications")), &t("Your notifications"))
        .append_item(ID_FRIENDS, &shortcuts_dialog::menu_label(ID_FRIENDS, t("Fr&iends")), &t("Your friends list"))
        .append_item(ID_MYPOSTS, &shortcuts_dialog::menu_label(ID_MYPOSTS, t("&My posts")), &t("Posts on your own profile"))
        .append_item(ID_CHATS, &shortcuts_dialog::menu_label(ID_CHATS, t("&Chats")), &t("Messenger chats"))
        .append_item(ID_GROUPS_VIEW, &shortcuts_dialog::menu_label(ID_GROUPS_VIEW, t("&Groups")), &t("Groups you belong to"))
        .append_item(
            ID_RECENT_FEED,
            &shortcuts_dialog::menu_label(ID_RECENT_FEED, t("Feeds (most &recent)")),
            &t("Newest posts from followed friends, pages and groups"),
        )
        .append_item(
            ID_REELS,
            &shortcuts_dialog::menu_label(ID_REELS, t("Re&els")),
            &t("Browse reels with the accessible player"),
        )
        .append_item(
            ID_MARKETPLACE,
            &shortcuts_dialog::menu_label(ID_MARKETPLACE, t("&Marketplace")),
            &t("Browse Marketplace listings"),
        )
        .append_item(ID_EVENTS_VIEW, &shortcuts_dialog::menu_label(ID_EVENTS_VIEW, t("&Events")), &t("Events from your events page"))
        .append_item(ID_PAGES_VIEW, &shortcuts_dialog::menu_label(ID_PAGES_VIEW, t("Pa&ges")), &t("Pages from your pages page"))
        .append_item(ID_PLACES_VIEW, &shortcuts_dialog::menu_label(ID_PLACES_VIEW, t("P&laces")), &t("Places from the places page"))
        .append_item(ID_SAVED_VIEW, &shortcuts_dialog::menu_label(ID_SAVED_VIEW, t("Sa&ved posts")), &t("Posts you saved on Facebook"))
        .append_item(
            ID_UNREAD_CHATS,
            &shortcuts_dialog::menu_label(ID_UNREAD_CHATS, t("&Unread chats only")),
            &t("Chats with unread messages"),
        )
        .append_separator()
        .append_item(ID_REFRESH, &shortcuts_dialog::menu_label(ID_REFRESH, t("&Refresh")), &t("Reload the current view"))
        .append_item(
            ID_MORE,
            &shortcuts_dialog::menu_label(ID_MORE, t("&Load more")),
            &t("Append more items and keep your reading position"),
        )
        .append_separator()
        .append_check_item(
            ID_VOICE_ALERTS,
            &shortcuts_dialog::menu_label(ID_VOICE_ALERTS, t("&Voice alerts")),
            &t("Say new notifications and unread chats aloud"),
        )
        .append_check_item(
            ID_SOUND_ALERTS,
            &shortcuts_dialog::menu_label(ID_SOUND_ALERTS, t("&Sound alerts")),
            &t("Play a sound for new notifications and unread chats"),
        )
        .append_check_item(
            ID_LIST_VOICE_ALERTS,
            &shortcuts_dialog::menu_label(ID_LIST_VOICE_ALERTS, t("Voice alert when a &list loads")),
            &t("Say the name and size of a list when it finishes loading"),
        )
        .append_check_item(
            ID_LIST_SOUND_ALERTS,
            &shortcuts_dialog::menu_label(ID_LIST_SOUND_ALERTS, t("Sound alert when a list &loads")),
            &t("Play a sound when a list finishes loading"),
        )
        .build();

    let mut post_menu = Menu::builder()
        .append_item(ID_OPEN, &shortcuts_dialog::menu_label(ID_OPEN, t("&Open post / comments")), &t("Open the selected item; Enter in the items list"))
        .append_item(ID_SHARE, &shortcuts_dialog::menu_label(ID_SHARE, t("S&hare…")), &t("Share the selected post"))
        .append_item(ID_COMMENT, &shortcuts_dialog::menu_label(ID_COMMENT, t("&Comment…")), &t("Comment on the selected post"))
        .append_item(ID_POST_MENU, &shortcuts_dialog::menu_label(ID_POST_MENU, t("Facebook post &menu…")), &t("Facebook's own menu for the selected post: Not interested, Save, Snooze, Hide, Report and more"))
        .append_item(ID_PROFILE, &shortcuts_dialog::menu_label(ID_PROFILE, t("Author &profile")), &t("Show the profile of the selected item's author or owner"))
        .append_item(ID_AUTHOR_POSTS, &shortcuts_dialog::menu_label(ID_AUTHOR_POSTS, t("Author's p&osts")), &t("Posts by the selected item's author"))
        .append_item(ID_GROUP_POSTS, &shortcuts_dialog::menu_label(ID_GROUP_POSTS, t("Group's p&osts")), &t("Posts in the group the selected post belongs to"))
        .append_item(ID_MESSAGE, &shortcuts_dialog::menu_label(ID_MESSAGE, t("&Message author / friend")), &t("Open a Messenger chat with this person"))
        .append_item(ID_WEB_POST, &shortcuts_dialog::menu_label(ID_WEB_POST, t("Open post on &Facebook (web)")), &t("Open the selected post in your browser"))
        .append_item(ID_WEB_PERSON, &shortcuts_dialog::menu_label(ID_WEB_PERSON, t("Open person or group on Fa&cebook (web)")), &t("Open the selected person's profile (or the group) in your browser, where friend management lives"))
        .append_separator()
        .append_item(ID_DELETE, &shortcuts_dialog::menu_label(ID_DELETE, t("&Delete post…")), &t("Delete the selected post (your own posts only)"));
    for (i, reaction) in REACTIONS.iter().enumerate() {
        post_menu = post_menu.append_item(
            ID_REACT_BASE + i as i32,
            &shortcuts_dialog::menu_label(ID_REACT_BASE + i as i32, react_menu_label(reaction)),
            &t("Set this reaction on the selected post"),
        );
    }
    let post_menu = post_menu.build();

    let chat_menu = Menu::builder()
        .append_item(ID_OPEN, &shortcuts_dialog::menu_label(ID_OPEN, t("&Open chat")), &t("Open the selected chat"))
        .append_item(
            ID_WEB_CHAT,
            &shortcuts_dialog::menu_label(ID_WEB_CHAT, t("Open chat in &Messenger (web)")),
            &t("Open this chat in your browser — voice and video calls live there"),
        )
        .append_item(
            ID_UNSEND,
            &shortcuts_dialog::menu_label(ID_UNSEND, t("&Unsend my last message…")),
            &t("Remove your newest message in the open chat for everyone"),
        )
        .append_item(
            ID_UNLOCK,
            &shortcuts_dialog::menu_label(ID_UNLOCK, t("Unloc&k encrypted chats…")),
            &t("Enter your Messenger PIN to read end-to-end encrypted chats"),
        )
        .build();

    let backend_menu = Menu::builder()
        .append_item(
            ID_INSTALL_BACKEND,
            &shortcuts_dialog::menu_label(ID_INSTALL_BACKEND, t("&Install or update backend…")),
            &t("Download BlindFB's supported facebook-cli backend from Forgejo"),
        )
        .append_item(ID_STATUS, &shortcuts_dialog::menu_label(ID_STATUS, t("Backend &status")), &t("Show fb daemon status"))
        .append_item(ID_LOGIN, &shortcuts_dialog::menu_label(ID_LOGIN, t("&Sign in…")), &t("Sign in through your browser"))
        .append_item(
            ID_INSTALL_BROWSER,
            &shortcuts_dialog::menu_label(ID_INSTALL_BROWSER, t("Install a &browser…")),
            &t("Download a browser BlindFB can use (Chrome, Brave, Edge, Firefox)"),
        )
        .append_item(ID_STOP_DAEMON, &shortcuts_dialog::menu_label(ID_STOP_DAEMON, t("S&top daemon")), &t("Stop the fb daemon"))
        .append_item(
            ID_QUIT_DAEMON,
            &shortcuts_dialog::menu_label(ID_QUIT_DAEMON, t("&Quit daemon and Chrome")),
            &t("Stop the daemon and close its hidden Chrome"),
        )
        .build();

    let help_menu = Menu::builder()
        .append_item(
            ID_KEYS,
            &shortcuts_dialog::menu_label(ID_KEYS, t("&Keyboard commands")),
            &t("Keyboard commands and reader controls"),
        )
        .append_item(
            ID_GITHUB,
            &shortcuts_dialog::menu_label(ID_GITHUB, t("BlindFB on &Forgejo")),
            &t("Open the BlindFB repository"),
        )
        .append_separator()
        .append_check_item(
            ID_DEBUG_LOGGING,
            &shortcuts_dialog::menu_label(ID_DEBUG_LOGGING, t("&Debug logging")),
            &t("Record what BlindFB runs and the errors it gets, for a bug report"),
        )
        .append_item(
            ID_DEBUG_REPORT,
            &shortcuts_dialog::menu_label(ID_DEBUG_REPORT, t("Save debug &report…")),
            &t("Save a file you can send to the developer"),
        )
        .append_separator()
        // Language names stay in their own language, so anyone can find theirs.
        .append_radio_item(
            ID_LANG_SYSTEM,
            &shortcuts_dialog::menu_label(ID_LANG_SYSTEM, t("Language: &system default")),
            &t("Use the system language when BlindFB has it, English otherwise"),
        )
        .append_radio_item(ID_LANG_ENGLISH, "Language: &English", "Show BlindFB in English")
        .append_radio_item(
            ID_LANG_FRENCH,
            "Langue : &français",
            "Afficher BlindFB en français",
        )
        .append_radio_item(
            ID_LANG_RUSSIAN,
            "Язык: р&усский",
            "Показать BlindFB на русском",
        )
        .append_radio_item(
            ID_LANG_ESTONIAN,
            "Keel: e&esti",
            "Näita BlindFB eesti keeles",
        )
        .append_separator()
        .append_item(ID_ABOUT, &shortcuts_dialog::menu_label(ID_ABOUT, t("&About BlindFB")), &t("About this program"))
        .build();

    let menu_bar = MenuBar::builder()
        .append(file_menu, &t("&File"))
        .append(view_menu, &t("&View"))
        .append(post_menu, &t("&Post"))
        .append(chat_menu, &t("&Chat"))
        .append(backend_menu, &t("&Backend"))
        .append(help_menu, &t("&Help"))
        .build();
    menu_bar.check_item(ID_DEBUG_LOGGING, blindfb_core::debug::load());
    menu_bar.check_item(
        match i18n::saved() {
            Language::System => ID_LANG_SYSTEM,
            Language::English => ID_LANG_ENGLISH,
            Language::French => ID_LANG_FRENCH,
            Language::Russian => ID_LANG_RUSSIAN,
            Language::Estonian => ID_LANG_ESTONIAN,
        },
        true,
    );
    menu_bar.check_item(ID_VOICE_ALERTS, alerts::enabled(alerts::Kind::Voice));
    menu_bar.check_item(ID_SOUND_ALERTS, alerts::enabled(alerts::Kind::Sound));
    menu_bar.check_item(
        ID_LIST_VOICE_ALERTS,
        alerts::enabled(alerts::Kind::ListVoice),
    );
    menu_bar.check_item(
        ID_LIST_SOUND_ALERTS,
        alerts::enabled(alerts::Kind::ListSound),
    );
    frame.set_menu_bar(menu_bar);
}

/// Ask Facebook for anything new, when alerts are on and the wait is over.
/// Sent straight to the worker: a background check must not say "Working…".
fn check_alerts_when_due(ui: &Ui) {
    if ui.state.borrow().next_alert_check > Instant::now() {
        return;
    }
    ui.state.borrow_mut().next_alert_check = Instant::now() + ALERT_EVERY;
    if !alerts::any_enabled() {
        return;
    }
    let _ = ui.jobs.send(Job {
        task: Task::Fb(notifs_args(PAGE), None),
        tag: Tag::AlertNotifications,
    });
    let _ = ui.jobs.send(Job {
        task: Task::Fb(chats_args(PAGE, true), None),
        tag: Tag::AlertChats,
    });
}

/// A failed check stays silent: alerts must not interrupt with errors, and
/// the next check tries again. Locked Messenger chats fail here every time.
fn alert_on_new(
    ui: &Ui,
    items: Result<Vec<(String, String)>, String>,
    watch: impl FnOnce(&mut State) -> &mut alerts::Watch,
) {
    let Ok(items) = items else { return };
    let new = watch(&mut ui.state.borrow_mut()).check(items);
    let (voice, sound) = (alerts::enabled(alerts::Kind::Voice), alerts::enabled(alerts::Kind::Sound));
    if new.is_empty() || !(voice || sound) {
        return;
    }
    let words = alerts::spoken(&new);
    ui.status(&words);
    alerts::play(sound, voice.then_some(words));
}

/// A list the user asked for just finished loading: say or chime it, when
/// the list alerts are on (#17). Background reads (prefetch, feed read-ahead,
/// a cached section refreshing itself) never reach here.
fn alert_list_loaded(ui: &Ui, view: &View, total: usize, added: usize, append: bool) {
    let (voice, sound) = (
        alerts::enabled(alerts::Kind::ListVoice),
        alerts::enabled(alerts::Kind::ListSound),
    );
    if !(voice || sound) {
        return;
    }
    let words = alerts::loaded(&view.title(), total, added, append);
    if voice {
        ui.status(&words);
    }
    alerts::play(sound, voice.then_some(words));
}

fn toggle_alert(ui: &Ui, kind: alerts::Kind) {
    let on = !alerts::enabled(kind);
    let was_on = alerts::any_enabled();
    if let Err(error) = alerts::set_enabled(kind, on) {
        show_error(&ui.frame, &error, &t("Alerts"));
        return;
    }
    let voice = matches!(kind, alerts::Kind::Voice | alerts::Kind::ListVoice);
    let words = match (kind, on) {
        (alerts::Kind::Voice, true) => t("Voice alerts on."),
        (alerts::Kind::Voice, false) => t("Voice alerts off."),
        (alerts::Kind::Sound, true) => t("Sound alerts on."),
        (alerts::Kind::Sound, false) => t("Sound alerts off."),
        (alerts::Kind::ListVoice, true) => t("List voice alerts on."),
        (alerts::Kind::ListVoice, false) => t("List voice alerts off."),
        (alerts::Kind::ListSound, true) => t("List sound alerts on."),
        (alerts::Kind::ListSound, false) => t("List sound alerts off."),
    };
    ui.status(&words);
    if !on {
        return;
    }
    // Hearing the alert now proves it works; nothing else would for a while.
    alerts::play(!voice, voice.then_some(words));
    if matches!(kind, alerts::Kind::ListVoice | alerts::Kind::ListSound) {
        return;
    }
    if !was_on {
        // Alerts were off: anything since then is a backlog, not news.
        let mut state = ui.state.borrow_mut();
        state.alert_notifications = Default::default();
        state.alert_chats = Default::default();
        state.next_alert_check = Instant::now();
    }
}

fn activate_view(ui: &Ui, view: &View) {
    ui.latest_list.lock().unwrap().background.remove(view);
    let selected = ui.items.get_selection();
    let mut state = ui.state.borrow_mut();
    let clear = state.records_view != *view;
    if clear {
        // Remember the outgoing list's cursor before its records and the
        // list selection are cleared: show_records runs only once the
        // next list has loaded, when both are already gone, so the save
        // there never saw them and every return landed on the first
        // item (issue #23).
        let key = selection_key(&state.records_view, &state.records, selected);
        if let Some(key) = key {
            let outgoing = state.records_view.clone();
            state.selection_memory.insert(outgoing, key);
        }
        state.records.clear();
    }
    if !quote_follows_chat(&state.view, view) {
        // A reply quote names a message in one conversation: opening another
        // chat must not carry it over and quote the wrong thread.
        state.reply_to = None;
    }
    state.view = view.clone();
    drop(state);
    if clear {
        ui.items.clear();
        set_details(ui, "", None);
    }
}

/// True while the view stays on the conversation a reply quote was made in.
/// Leaving chats, or opening a different chat, drops the quote.
fn quote_follows_chat(previous: &View, next: &View) -> bool {
    matches!((previous, next), (View::ChatWith(a, _), View::ChatWith(b, _)) if a == b)
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
        tag: Tag::List {
            view: view.clone(),
            request,
            args,
            unlock_attempted: false,
            append: false,
        },
    });
    // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
    ui.status(&tf!("Loading {view}…", view = view.title()));
}

fn load_view(ui: &Ui, view: View) {
    {
        let mut state = ui.state.borrow_mut();
        // Any navigation supersedes a section still waiting out the Views debounce.
        state.pending_view = None;
        if state.back_nav {
            state.back_nav = false;
        } else if state.view != view && state.history.last() != Some(&state.view) {
            let previous = state.view.clone();
            state.history.push(previous);
            if state.history.len() > 50 {
                state.history.remove(0);
            }
        }
    }
    let cached = ui.state.borrow().cache.get(&view).cloned();
    activate_view(ui, &view);
    if let Some((records, _, args)) = &cached {
        ui.state.borrow_mut().last_args = args.clone();
        show_records(ui, &view, records.clone(), None);
    }
    if ui.latest_list.lock().unwrap().pending.contains(&view) {
        // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
        ui.status(&tf!("Loading {view}…", view = view.title()));
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
    } else if let Some(args) = cached
        .as_ref()
        .map(|(_, _, args)| args.clone())
        .or_else(|| default_args(&view))
    {
        load_args(ui, view, args);
    }
    if cached.is_some() {
        let mut state = ui.state.borrow_mut();
        state.background = ui
            .latest_list
            .lock()
            .unwrap()
            .latest
            .get(&state.view)
            .copied();
    }
}

/// Backspace/Escape in the list: return to the previous list, like a
/// browser Back. Internal loads (refresh, send reloads) never pushed, so
/// Back always lands on a list the user actually visited.
fn pop_previous(history: &mut Vec<View>, current: &View) -> Option<View> {
    loop {
        match history.pop() {
            Some(view) if view != *current => return Some(view),
            Some(_) => continue,
            None => return None,
        }
    }
}

fn go_back(ui: &Ui) {
    let target = {
        let mut state = ui.state.borrow_mut();
        let current = state.view.clone();
        let target = pop_previous(&mut state.history, &current);
        if target.is_some() {
            state.back_nav = true;
        }
        target
    };
    match target {
        Some(view) => {
            // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
            ui.status(&tf!("Back to {view}", view = view.title()));
            load_view(ui, view);
        }
        None => ui.status(&t("No previous list to go back to.")),
    }
}

/// Warm the sections a session actually starts with, so first visits are
/// instant too. Two of them are deliberately not prefetched: Recent feed and
/// My posts each scroll a long tail (20 s or more on a real account), and
/// spending that at launch delays every other read for no visible gain —
/// they load on first visit like any other section.
fn prefetch(ui: &Ui) {
    for view in [View::Notifications, View::Friends, View::Chats, View::Groups] {
        if let Some(args) = default_args(&view) {
            let request = {
                let mut requests = ui.latest_list.lock().unwrap();
                if requests.pending.contains(&view) || ui.state.borrow().cache.contains_key(&view) {
                    continue;
                }
                requests.begin_background(&view)
            };
            let _ = ui.jobs.send(Job {
                task: Task::Fb(args.clone(), None),
                tag: Tag::List {
                    view,
                    request,
                    args,
                    unlock_attempted: false,
                    append: false,
                },
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
            | View::Events
            | View::Pages
            | View::Places
            | View::Saved
            // Search has no default_args (the query is the user's), so the
            // cache is the only way Back can restore the results; without
            // it Back landed on an empty list (issue #24).
            | View::SearchResults
    ) && !args.iter().any(|a| a == "--unread")
}

fn default_args(view: &View) -> Option<Vec<String>> {
    Some(match view {
        View::Feed => feed_args(FEED_FIRST, false, true, true),
        View::RecentFeed => recent_feed_args(PAGE),
        View::Notifications => notifs_args(PAGE),
        View::Friends => friends_args(None, PAGE),
        View::MyPosts => posts_args("me", MY_POSTS_FIRST, true),
        View::Chats => chats_args(PAGE, false),
        View::Groups => groups_args(PAGE),
        View::Marketplace => marketplace_args(PAGE),
        View::Events => events_args(PAGE),
        View::Pages => pages_args(PAGE),
        View::Places => places_args(PAGE),
        View::Saved => saved_args(PAGE),
        View::SearchResults | View::Reels(_) => return None,
        View::PostsOf(who) | View::GroupPosts(who) => posts_args(who, PAGE, true),
        View::CommentsOf(post) => view_args(post, 50),
        View::ProfileOf(who) => profile_args(who),
        View::ChatWith(id, _) => read_args(id, 50),
    })
}

fn load_reels(ui: &Ui, source: ReelSource, append: bool) {
    let view = View::Reels(source.clone());
    let request = begin_list(ui, &view);
    ui.send(Job {
        task: Task::ReelsLoad { source, append },
        tag: Tag::List {
            view: view.clone(),
            request,
            args: Vec::new(),
            unlock_attempted: false,
            append,
        },
    });
    // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
    ui.status(&tf!("Loading {view}…", view = view.title()));
}

/// Ask which browser to copy the Facebook session from. The user's
/// default browser is marked; the last entry signs in directly in a new
/// window of the backend's own browser. When no supported browser is
/// installed, offer to install Google Chrome — the backend needs a
/// Chrome-family browser to run at all.
fn choose_sign_in(ui: &Ui) -> Option<SignIn> {
    let detected = browsers::detect_browsers();
    if !detected
        .iter()
        .any(|browser| browser.kind.is_chromium() && browser.exe.is_some())
    {
        if confirm(
            &ui.frame,
            t("BlindFB needs an installed Chromium browser (Chrome, Brave or Edge) to run its separate browser profile. Firefox can supply your Facebook session but cannot run the backend. Install Google Chrome now?"),
            &t("Sign in"),
        ) {
            return Some(SignIn::Install(BrowserKind::Chrome));
        }
        return None;
    }
    let mut labels: Vec<String> = detected
        .iter()
        .map(|browser| {
            if browser.is_default {
                tf!("{browser} (your default browser)", browser = browser.name())
            } else {
                browser.name().to_owned()
            }
        })
        .collect();
    labels.push(t("Sign in in a new browser window"));
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        &t("Sign in"),
        &if cfg!(windows) {
            t("Copy your Facebook session from which browser? You must be signed in to Facebook in that browser. Close the browser first if its cookie database is locked. In Chrome, Brave and Edge, tick \"Allow remote debugging\" if the browser asks. Or choose a new browser window to sign in directly.")
        } else {
            t("Copy your Facebook session from which browser? You must be signed in to Facebook in that browser. In Chrome, Brave and Edge, tick \"Allow remote debugging\" if the browser asks.")
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
        state.history.clear();
        state.pending_details = None;
        state.reply_to = None;
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
            ui.status(&tf!("Installing {browser}…", browser = kind.name()));
            ui.send(Job {
                task: Task::InstallBrowser(kind),
                tag: Tag::BrowserInstalled,
            });
            true
        }
        Some(sign) => {
            ui.send(Job {
                task: Task::LoginFlow(sign),
                tag,
            });
            ui.status(&t("Signing in through your browser…"));
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
    let kinds = [
        BrowserKind::Chrome,
        BrowserKind::Brave,
        BrowserKind::Edge,
        BrowserKind::Firefox,
    ];
    let detected = browsers::detect_browsers();
    let labels: Vec<String> = kinds
        .iter()
        .map(|kind| {
            if detected.iter().any(|browser| browser.kind == *kind) {
                tf!("{browser} (installed)", browser = kind.name())
            } else {
                kind.name().to_owned()
            }
        })
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        &t("Install a browser"),
        &t("Install which browser? Downloads come from the vendor. Chrome, Chromium, Brave and Edge can run BlindFB's separate browser profile. Firefox can supply a Facebook session."),
        &label_refs,
    )
    .build();
    if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        if index >= 0 {
            let kind = kinds[index as usize];
            ui.status(&tf!("Installing {browser}…", browser = kind.name()));
            ui.send(Job {
                task: Task::InstallBrowser(kind),
                tag: Tag::BrowserInstalledMenu,
            });
        }
    }
}

fn refresh(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    if matches!(view, View::Feed | View::Reels(_))
        && ui.latest_list.lock().unwrap().pending.contains(&view)
    {
        ui.status(&t("Wait for this section to finish loading before refreshing."));
        return;
    }
    match view {
        View::Reels(source) => load_reels(ui, source, false),
        View::Feed => load_args(ui, view, feed_args(FEED_FIRST, true, true, true)),
        View::Groups => load_args(ui, view, groups_args(PAGE)),
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

fn more_args(
    view: &View,
    previous: &[String],
    loaded: usize,
    loading: bool,
) -> Option<Vec<String>> {
    if loading || previous.is_empty() {
        return None;
    }
    match view {
        View::Feed => Some(feed_args(PAGE, false, true, true)),
        View::Notifications => Some(notifs_args(u32::try_from(loaded).ok()?.checked_add(PAGE)?)),
        View::RecentFeed
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Friends
        | View::Chats
        | View::SearchResults
        | View::Marketplace
        | View::Events
        | View::Pages
        | View::Places
        | View::Saved
        | View::Groups => {
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
        if ui
            .latest_list
            .lock()
            .unwrap()
            .pending
            .contains(&ui.state.borrow().view)
        {
            ui.status(&t("Reels are already loading…"));
        } else if !loaded {
            ui.status(&t("Load the reels list first, or refresh it to retry."));
        } else {
            load_reels(ui, source, true);
        }
        return;
    }
    let (view, args) = {
        let state = ui.state.borrow();
        if state.records_view != state.view || state.records.is_empty() {
            ui.status(&t("Wait for this section to load before requesting more items."));
            return;
        }
        let loading = ui.latest_list.lock().unwrap().pending.contains(&state.view);
        let Some(args) = more_args(&state.view, &state.last_args, state.records.len(), loading)
        else {
            ui.status(&if loading { t("Items are already loading…") } else {
                t("Load more is available for feeds, posts, search results, notifications, friends and chat lists.")
            });
            return;
        };
        (state.view.clone(), args)
    };
    let request = begin_list(ui, &view);
    ui.send(Job {
        task: Task::Fb(args.clone(), None),
        tag: Tag::List {
            view: view.clone(),
            request,
            args,
            unlock_attempted: false,
            append: true,
        },
    });
    // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
    ui.status(&tf!("Loading more {view}…", view = view.title()));
}

/// The News feed's read-ahead: the next posts arrive while the reader is still
/// near the top, so reaching the end of the list does not stall. It stays
/// background work, so a section the user opens goes first.
fn load_feed_ahead(ui: &Ui) {
    let view = View::Feed;
    if ui.latest_list.lock().unwrap().pending.contains(&view) {
        return;
    }
    activate_view(ui, &view);
    send_ahead(ui, view, feed_args(FEED_AHEAD, false, true, true));
}

/// Most Recent's read-ahead, after its first page: the next posts arrive
/// while the reader is still near the top, so Load more does not stall
/// behind a re-scroll from the top.
fn load_recent_ahead(ui: &Ui) {
    let view = View::RecentFeed;
    if ui.latest_list.lock().unwrap().pending.contains(&view) {
        return;
    }
    activate_view(ui, &view);
    send_ahead(ui, view, recent_feed_args(RECENT_AHEAD));
}

/// Queue a read-ahead as background work that appends to `view`'s rows: on
/// screen when the section is showing, else into its cache.
fn send_ahead(ui: &Ui, view: View, args: Vec<String>) {
    let request = ui.latest_list.lock().unwrap().begin_background(&view);
    let _ = ui.jobs.send(Job {
        task: Task::Fb(args.clone(), None),
        tag: Tag::List {
            view,
            request,
            args,
            unlock_attempted: false,
            append: true,
        },
    });
}

fn merge_records(view: &View, mut existing: Vec<Record>, incoming: Vec<Record>) -> Vec<Record> {
    // Rows without a ref share the placeholder ("-" or empty); keying on it
    // would collapse every ref-less row into the first one on Load more.
    let id_or_raw = |value: &str, record: &Record| {
        if value.is_empty() || value == "-" {
            record.raw.clone()
        } else {
            value.to_owned()
        }
    };
    let key = |record: &Record| match view {
        View::Friends | View::Groups | View::Events | View::Pages | View::Places => {
            id_or_raw(record.field(1), record)
        }
        View::Chats => id_or_raw(record.field(0), record),
        View::Notifications => format!("{}\t{}", record.field(2), record.field(3)),
        _ => post_target(view, record)
            .map(str::to_owned)
            .unwrap_or_else(|| record.raw.clone()),
    };
    let mut seen: HashSet<String> = existing.iter().map(&key).collect();
    let feed = matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::MyPosts
            | View::PostsOf(_)
            | View::GroupPosts(_)
            | View::Saved
    );
    let mut seen_posts: HashSet<String> = existing
        .iter()
        .map(blindfb_core::fb::feed_record_key)
        .collect();
    existing.extend(incoming.into_iter().filter(|record| {
        seen.insert(key(record))
            && (!feed || seen_posts.insert(blindfb_core::fb::feed_record_key(record)))
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
            | View::GroupPosts(_)
            | View::Saved
            | View::SearchResults
    );
    // A search row opens by kind, not by post target: profiles, groups and
    // the rest carry a ref post_target never returns, so every result read
    // "(no link)" even though Enter opened it (issue #24). A search row is
    // unopenable only when it carries no target at all.
    let no_link = if *view == View::SearchResults {
        !search_row_openable(record)
    } else {
        has_posts && post_target(view, record).is_none()
    };
    if no_link {
        // TRANSLATORS: {item} is a list row (a post); the mark says it has no link to open.
        tf!("{item} (no link)", item = label)
    } else {
        label
    }
}

/// A search result can be opened when it carries a target (a daemon ref
/// or a URL); the row's kind decides what opening it does.
fn search_row_openable(record: &Record) -> bool {
    let target = record.field(1).trim();
    !target.is_empty() && target != "-"
}

/// A profile row as it reads. The backend sends "key<TAB>value" lines;
/// name and address read bare, and every other row names what it is, so
/// the Actions row reads as the list of actions it carries (issue #27)
/// instead of a bare word that looks like a pressable item.
fn profile_row_label(record: &Record) -> String {
    let key = record.field(0).trim();
    let value = record.field(1).trim();
    if value.is_empty() {
        return key.to_owned();
    }
    match key {
        "name" | "url" => value.to_owned(),
        "actions" => tf!("Actions: {actions}", actions = value),
        "" => value.to_owned(),
        _ => {
            let mut label = key.to_owned();
            if let Some(first) = label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            format!("{label}: {value}")
        }
    }
}

fn display_for(view: &View, record: &Record) -> String {
    if matches!(view, View::ProfileOf(_)) {
        return profile_row_label(record);
    }
    // Search rows are (name, target, info, kind[, web url]) and listing
    // rows the same without the fifth field: both read "Name — info"; the
    // raw line would read the ref.
    if (matches!(view, View::SearchResults) && record.fields.len() >= 3)
        || (matches!(view, View::Marketplace) && (3..5).contains(&record.fields.len()))
    {
        let info = record.field(2);
        if info.is_empty() {
            return record.field(0).to_owned();
        }
        return format!("{} — {}", record.field(0), info);
    }
    match view {
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Saved
        | View::SearchResults => {
            if record.fields.len() >= 5 {
                // A post without text or time must not read as ". ." to a screen reader.
                let text = one_line(&record.fields[4..].join(" "));
                // In Most Recent the time is the point of the view: say it
                // right after the author, before the (possibly long) text
                // buries it at the end of the row.
                let parts: [&str; 3] = if matches!(view, View::RecentFeed) {
                    [record.field(1), record.field(2), text.as_str()]
                } else {
                    [record.field(1), text.as_str(), record.field(2)]
                };
                parts
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
                let notification = format!("{}. {}", record.field(2), record.field(0));
                if record.field(1) == "!" {
                    // TRANSLATORS: {notification} is a notification's text and time.
                    tf!("Unread. {notification}", notification = notification)
                } else {
                    notification
                }
            } else {
                record.raw.clone()
            }
        }
        View::Friends | View::Events | View::Pages | View::Places => {
            if record.fields.len() >= 3 {
                let info = record.field(2);
                if info.is_empty() {
                    record.field(0).to_owned()
                } else {
                    format!("{} — {}", record.field(0), info)
                }
            } else {
                record.raw.clone()
            }
        }
        View::Chats => {
            if record.fields.len() >= 5 {
                let flags = record.field(1);
                let name = record.field(3);
                let name = match (flags.contains('!'), flags.contains('g')) {
                    // TRANSLATORS: {name} is a chat's name in the Chats list.
                    (true, true) => tf!("{name} (unread) (group)", name = name),
                    // TRANSLATORS: {name} is a chat's name in the Chats list.
                    (true, false) => tf!("{name} (unread)", name = name),
                    // TRANSLATORS: {name} is a chat's name in the Chats list.
                    (false, true) => tf!("{name} (group)", name = name),
                    (false, false) => name.to_owned(),
                };
                format!(
                    "{} · {} — {}",
                    name,
                    record.field(2),
                    record.fields[4..].join(" ")
                )
            } else {
                record.raw.clone()
            }
        }
        View::CommentsOf(_) => {
            if record.fields.len() >= 5 && record.field(0) == "c" {
                format!(
                    "{} · {} — {}",
                    record.field(2),
                    record.field(3),
                    record.fields[4..].join(" ")
                )
            } else {
                record.raw.clone()
            }
        }
        _ => record.raw.clone(),
    }
}

fn details_for(view: &View, record: &Record) -> String {
    if *view == View::SearchResults && record.fields.len() >= 3 {
        let info = record.field(2);
        if info.is_empty() {
            return record.field(0).to_owned();
        }
        return format!("{}\n{}", record.field(0), info);
    }
    match view {
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Saved
        | View::SearchResults => {
            if record.fields.len() >= 5 {
                tf!(
                    "Author: {author}\nTime: {time}\n{counts}\n\n{text}",
                    author = record.field(1),
                    time = record.field(2),
                    counts = counts_label(record.field(3)),
                    text = record.fields[4..].join("\t")
                )
            } else {
                record.raw.clone()
            }
        }
        View::Friends => tf!(
            "Name: {name}\nRef: {reference}\n{info}",
            name = record.field(0),
            reference = record.field(1),
            info = record.field(2)
        ),
        View::Chats => {
            if record.fields.len() >= 5 {
                tf!(
                    "Chat: {name}\nTime: {time}\nFlags: {flags} (! unread, g group, e encrypted)\n\n{text}",
                    name = record.field(3),
                    time = record.field(2),
                    flags = record.field(1),
                    text = record.fields[4..].join("\t")
                )
            } else {
                record.raw.clone()
            }
        }
        View::Events | View::Pages | View::Places => {
            let info = record.field(2);
            tf!(
                "{name}\n{info}On the web: {url}",
                name = record.field(0),
                info = if info.is_empty() {
                    String::new()
                } else {
                    format!("{info}\n")
                },
                url = record.field(1)
            )
        }
        View::ProfileOf(_) => profile_row_label(record),
        View::Groups => {
            let info = record.field(2);
            tf!(
                "Group: {name}\n{info}On the web: {url}\n\nOpen the group for its window: its posts, and its full member list. Posting to a group, and group settings, live on the web page.",
                name = record.field(0),
                info = if info.is_empty() { String::new() } else { format!("{info}\n") },
                url = record.field(1)
            )
        }
        _ => record.raw.clone(),
    }
}

fn one_line(text: &str) -> String {
    text.replace('⏎', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn post_target<'a>(view: &View, record: &'a Record) -> Option<&'a str> {
    if *view == View::SearchResults {
        // Search rows are (name, target, info, kind): the target is a ref
        // for posts, a full URL for photos and videos, and neither for the
        // kinds other actions handle (profiles, groups, listings).
        let value = record.field(1).trim();
        if value.is_empty() {
            return None;
        }
        return match record.field(3) {
            "post" => Some(value),
            "photo" | "video" => {
                (value.starts_with("http") || value.starts_with('/')).then_some(value)
            }
            _ => None,
        };
    }
    let value = match view {
        View::Notifications => record.field(3),
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Saved => record.field(0),
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
            View::Friends | View::Groups | View::Events | View::Pages | View::Places => {
                record.field(1)
            }
            View::Feed
            | View::RecentFeed
            | View::Reels(_)
            | View::MyPosts
            | View::PostsOf(_)
            | View::GroupPosts(_)
            | View::Saved
            | View::SearchResults => &record.raw,
            _ => record.field(0),
        })
        .to_owned()
}

/// The identity of the record at `selected`, for selection memory: a row
/// with no identity (empty key, or the "-" placeholder) remembers nothing.
fn selection_key(view: &View, records: &[Record], selected: Option<u32>) -> Option<String> {
    selected
        .and_then(|i| records.get(i as usize))
        .map(|r| record_key(view, r))
        .filter(|k| !k.is_empty() && k != "-")
}

// shortcut: extract URLs written in the text; named attachment links are available in the full reader.
fn links_in_text(text: &str, post: Option<&str>) -> Vec<Link> {
    let mut links = Vec::new();
    if let Some(url) = post {
        links.push(Link {
            label: t("Full post on Facebook"),
            url: url.to_owned(),
            profile: None,
        });
    }
    for word in text.split(|c: char| c.is_whitespace() || c == '⏎') {
        let mut url = word.trim_start_matches(['(', '[', '<', '"', '\'']);
        loop {
            let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?', '"', '\'', '>']);
            let extra_closing = [('(', ')'), ('[', ']')].iter().any(|(open, close)| {
                trimmed.ends_with(*close)
                    && trimmed.matches(*close).count() > trimmed.matches(*open).count()
            });
            if extra_closing {
                url = &trimmed[..trimmed.len() - 1];
            } else {
                url = trimmed;
                break;
            }
        }
        let Some((scheme, rest)) = url.split_once("://") else {
            continue;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
            || authority.is_empty()
            || url
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | '"' | '\'' | '<' | '>'))
        {
            continue;
        }
        if !links
            .iter()
            .any(|link| link.url.trim_end_matches('/') == url.trim_end_matches('/'))
        {
            links.push(Link {
                label: url.to_owned(),
                url: url.to_owned(),
                profile: None,
            });
        }
    }
    match post {
        Some(post) => blindfb_core::posts::ordered_links(links, post),
        None => links,
    }
}

fn set_details(ui: &Ui, text: &str, post: Option<&str>) {
    set_details_linked(ui, text, post, &[]);
}

/// `extra`: links the record itself carries (a comment's own links from the
/// backend's `cl` lines), which the details text cannot hold — Facebook
/// renders a comment link's label trimmed, so the target only exists here.
fn set_details_linked(ui: &Ui, text: &str, post: Option<&str>, extra: &[Link]) {
    if ui.details.get_value() != text {
        ui.details.change_value(text);
    }
    let mut links = links_in_text(text, post);
    for link in extra.iter().rev() {
        if !links.iter().any(|shown| shown.url == link.url) {
            links.insert(0, link.clone());
        }
    }
    if *ui.link_targets.borrow() == links {
        return;
    }
    let selected = ui.links.get_selection().and_then(|i| {
        ui.link_targets
            .borrow()
            .get(i as usize)
            .map(|link| link.url.clone())
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
    if let Some(index) = selected
        .and_then(|url| links.iter().position(|link| link.url == url))
        .or_else(|| (!links.is_empty()).then_some(0))
    {
        ui.links.set_selection(index as u32, true);
    }
    *ui.link_targets.borrow_mut() = links;
    ui.open_link.enable(ui.links.get_selection().is_some());
}

fn open_detail_link(ui: &Ui) {
    if let Some(url) = ui.links.get_selection().and_then(|i| {
        ui.link_targets
            .borrow()
            .get(i as usize)
            .map(|link| link.url.clone())
    }) {
        open_url(&url);
    }
}

fn append_links_menu(menu: &Menu, links: &[Link]) {
    if links.is_empty() {
        return;
    }
    const BASE: i32 = 20000;
    let submenu = Menu::builder().build();
    for (index, link) in links.iter().enumerate() {
        let label = link
            .label
            .replace('&', "&&")
            .replace(['\t', '\r', '\n'], " ");
        submenu.append(
            BASE + index as i32,
            &format!("{}. {label}", index + 1),
            &link.url,
            ItemKind::Normal,
        );
    }
    // Keep the URLs shown when the menu opened, even if a background refresh finishes.
    let targets = links.to_vec();
    submenu.on_selected(move |event| {
        event.event.skip(false);
        if let Some(link) = event
            .get_id()
            .checked_sub(BASE)
            .and_then(|index| targets.get(index as usize))
        {
            open_url(&link.url);
        }
    });
    menu.append_submenu(submenu, &t("&Links"), &t("Open a link in your browser"));
}

/// A row became selected, by arrows or by type-ahead.
fn item_selected(ui: &Ui, sel: u32) {
    // Debounce the details pane: it rebuilds the text and the link
    // list, which is real work per row and makes a screen reader
    // process a flood of changes while arrowing. The resting row
    // pays it once; rows passed over pay nothing.
    ui.state.borrow_mut().pending_details = Some((sel as usize, Instant::now()));
    refresh_controls(ui);
    prepare_reels(ui);
    let near_end = {
        let state = ui.state.borrow();
        state.view == View::Feed
            && state.records_view == View::Feed
            && !state.records.is_empty()
            && sel as usize + FEED_NEAR_END >= state.records.len()
    };
    if near_end {
        load_feed_ahead(ui);
    }
}

fn prepare_reels(ui: &Ui) {
    let Some(index) = ui.items.get_selection() else {
        return;
    };
    let (account, page, targets) = {
        let mut state = ui.state.borrow_mut();
        if !matches!(state.records_view, View::Reels(_)) || state.view != state.records_view {
            return;
        }
        let View::Reels(source) = &state.records_view else {
            return;
        };
        let page = reels::page_name(source);
        let targets: Vec<_> = state
            .records
            .iter()
            .skip(index as usize)
            .take(2)
            .map(|record| record.field(0).to_owned())
            .collect();
        let targets = state.reel_cache.plan(&targets, Instant::now());
        (state.account, page, targets)
    };
    for target in targets {
        let _ = ui.jobs.send(Job {
            task: Task::PostVideo {
                page: page.clone(),
                target: target.clone(),
            },
            tag: Tag::ReelPrepared { account, target },
        });
    }
}

fn set_reel_title(record: &mut Record, target: &str, title: &str) -> bool {
    if title.is_empty()
        || record.field(0) != target
        || !record.field(4).trim().is_empty()
        || record.fields.len() < 5
    {
        return false;
    }
    record.fields[4] = title.to_owned();
    record.raw = record.fields.join("\t");
    true
}

fn update_reel_titles(ui: &Ui, target: &str, title: &str) {
    let changed = {
        let mut state = ui.state.borrow_mut();
        for (view, (records, _, _)) in &mut state.cache {
            if matches!(view, View::Reels(_)) {
                for record in records {
                    set_reel_title(record, target, title);
                }
            }
        }
        if !matches!(state.records_view, View::Reels(_)) {
            return;
        }
        let current = state.view == state.records_view;
        let view = state.records_view.clone();
        state
            .records
            .iter_mut()
            .enumerate()
            .filter_map(|(index, record)| {
                (set_reel_title(record, target, title) && current)
                    .then(|| (index, list_label(&view, record)))
            })
            .collect::<Vec<_>>()
    };
    for (index, label) in changed {
        ui.items.set_string(index as u32, &label);
        if ui.items.get_selection() == Some(index as u32) {
            show_details(ui, index);
        }
    }
}

fn show_details(ui: &Ui, index: usize) {
    let state = ui.state.borrow();
    if is_more_row(&state.records_view, state.records.len(), index) {
        let label = more_row_label(&state.records_view, state.records.len()).unwrap();
        // TRANSLATORS: {action} is a "Load more…" row label such as "Load more posts".
        let text = tf!("{action}. Press Enter to load the next batch.", action = label);
        drop(state);
        set_details(ui, &text, None);
        return;
    }
    if let Some(record) = state.records.get(index) {
        let text = details_for(&state.view, record);
        let post = post_target(&state.view, record)
            .filter(|target| target.starts_with("https://") || target.starts_with('/'))
            .map(web_url);
        let extra = record.links.clone();
        drop(state);
        set_details_linked(ui, &text, post.as_deref(), &extra);
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

fn more_row_label(view: &View, count: usize) -> Option<String> {
    if count == 0 {
        return None;
    }
    match view {
        View::Feed
        | View::RecentFeed
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::SearchResults => Some(t("Load more posts")),
        View::Reels(_) => Some(t("Load more reels")),
        View::Notifications => Some(t("Load more notifications")),
        View::Friends => Some(t("Load more friends")),
        View::Groups => Some(t("Load more groups")),
        View::Marketplace => Some(t("Load more listings")),
        View::Chats => Some(t("Load more conversations")),
        _ => None,
    }
}

/// The feed's counts field is a bare "reactions/comments/shares" triple
/// ("0/0/0"); in the details pane name each number, as Michal asked
/// (issue #16). Anything that is not a numeric triple stays as it came.
fn counts_label(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('/').map(str::trim).collect();
    let numeric = |p: &str| {
        !p.is_empty()
            && p.chars().all(|c| {
                c.is_ascii_digit()
                    || matches!(
                        c,
                        '.' | ',' | 'K' | 'M' | 'k' | 'm' | ' ' | '\u{a0}' | '\u{202f}'
                    )
            })
    };
    if parts.len() == 3 && parts.iter().all(|p| numeric(p)) {
        tf!(
            "Reactions: {reactions}, comments: {comments}, shares: {shares}",
            reactions = parts[0],
            comments = parts[1],
            shares = parts[2]
        )
    } else {
        tf!("Reactions / comments / shares: {counts}", counts = raw)
    }
}

fn is_more_row(view: &View, count: usize, index: usize) -> bool {
    index == count && more_row_label(view, count).is_some()
}

fn more_selection(
    view: &View,
    old_count: usize,
    selected: Option<u32>,
    count: usize,
) -> Option<usize> {
    selected
        .filter(|index| count > 0 && is_more_row(view, old_count, *index as usize))
        .map(|index| (index as usize).min(count))
}

/// Show a section's backend output in the Items list, updated in place so the
/// selection and the screen reader's position survive.
fn show_records(ui: &Ui, view: &View, records: Vec<Record>, body: Option<String>) {
    if matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::MyPosts
            | View::PostsOf(_)
            | View::GroupPosts(_)
    ) {
        let mut state = ui.state.borrow_mut();
        for record in &records {
            let author = record.field(1);
            if !author.is_empty() {
                // The author's own profile path when the post carried one
                // (group post URLs name the group, so they derive nothing);
                // else the profile derived from the post URL.
                let profile = record
                    .author_ref
                    .as_deref()
                    .filter(|r| r.starts_with('/'))
                    .map(|r| format!("https://www.facebook.com{r}"))
                    .or_else(|| profile_url_from_post_url(record.field(0)));
                if let Some(profile) = profile {
                    state.author_urls.insert(author.to_owned(), profile);
                }
            }
        }
        drop(state);
    }
    let old_index = ui.items.get_selection();
    let (mut old_labels, selected_key, more_selected, remembered_key) = {
        let mut state = ui.state.borrow_mut();
        let labels: Vec<_> = state
            .records
            .iter()
            .map(|r| list_label(&state.records_view, r))
            .collect();
        let key = selection_key(&state.records_view, &state.records, old_index);
        // Leaving a list: remember where the cursor was, so returning to
        // that list restores it instead of landing on the first item.
        if state.records_view != *view {
            if let Some(key) = &key {
                let outgoing = state.records_view.clone();
                state.selection_memory.insert(outgoing, key.clone());
            }
        }
        let remembered = if state.records_view != *view {
            state.selection_memory.get(view).cloned()
        } else {
            None
        };
        let more_selected = if state.records_view == *view {
            more_selection(view, state.records.len(), old_index, records.len())
        } else {
            None
        };
        (labels, key, more_selected, remembered)
    };
    if let Some(label) = more_row_label(&ui.state.borrow().records_view, old_labels.len()) {
        old_labels.push(label);
    }
    let count = records.len();
    let mut labels: Vec<_> = records.iter().map(|r| list_label(view, r)).collect();
    if let Some(label) = more_row_label(view, count) {
        labels.push(label);
    }
    let selected = more_selected.or_else(|| {
        selected_key
            .and_then(|key| records.iter().position(|r| record_key(view, r) == key))
            .or_else(|| {
                remembered_key
                    .and_then(|key| records.iter().position(|r| record_key(view, r) == key))
            })
            .or_else(|| (!records.is_empty()).then_some(0))
    });
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
        if more_selected.is_some() || ui.items.get_selection() != Some(selected as u32) {
            ui.items.set_selection(selected as u32, true);
        }
    }
    if let Some(body) = body {
        set_details(ui, &body, None);
    } else if ui.items.get_count() > 0 {
        show_details(ui, selected.unwrap_or(0));
    } else {
        set_details(ui, &t("(nothing here)"), None);
    }
    // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
    ui.status(&ntf!("{view} — {n} item", "{view} — {n} items", count, view = view.title()));
    refresh_controls(ui);
    prepare_reels(ui);
}

/// Enable only the controls that mean something in the current view.
/// Every button used to sit live everywhere — React, Comment and Delete
/// in a chat list, Send outside a chat — which made the control set noise
/// instead of guidance. The layout stays fixed (a screen reader's spatial
/// memory matters); availability follows the view and the selection.
fn refresh_controls(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    let has = ui.items.get_selection().is_some();
    let post_list = matches!(
        view,
        View::Feed
            | View::RecentFeed
            | View::Reels(_)
            | View::MyPosts
            | View::PostsOf(_)
            | View::GroupPosts(_)
            | View::SearchResults
    );
    let people = post_list || matches!(view, View::Friends);
    let comments = matches!(view, View::CommentsOf(_));
    let in_chat = matches!(view, View::ChatWith(..));
    let openable = post_list
        || matches!(
            view,
            View::Notifications | View::Friends | View::Chats | View::Groups
        );
    ui.open_btn.enable(has && openable);
    ui.react_btn.enable(has && post_list);
    ui.comment_btn.enable(has && post_list);
    ui.profile_btn.enable(has && (people || comments));
    ui.posts_btn.enable(has && people);
    ui.message_btn.enable(has && (people || comments));
    ui.delete_btn.enable(has && matches!(view, View::MyPosts));
    ui.compose_box.set_editable(in_chat);
    ui.attach_btn.enable(in_chat);
    ui.attachments_btn.enable(in_chat);
    ui.send_btn.enable(in_chat);
}

/// Which modal window is currently up, most specific first: the reader
/// opens over the group and chat windows, so it owns input while it lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModalKind {
    Reader,
    Group,
    Chat,
}

fn modal_kind(reader: bool, group: bool, chat: bool) -> Option<ModalKind> {
    if reader {
        Some(ModalKind::Reader)
    } else if group {
        Some(ModalKind::Group)
    } else if chat {
        Some(ModalKind::Chat)
    } else {
        None
    }
}

/// The dialog a popup must be parented to right now. The results timer
/// fires while a modal dialog is up; a popup parented to the main frame
/// then opens behind the modal and the app looks frozen (a "post not
/// found" error while the reader is open did exactly that). The dialog
/// widget is a Copy handle, so this copies it out and holds no borrow
/// while the caller blocks on the popup.
fn modal_dialog(ui: &Ui) -> Option<Dialog> {
    let reader = ui
        .post_reader
        .borrow()
        .as_ref()
        .map(|reader| reader.dialog());
    let group = ui
        .group_window
        .lock()
        .unwrap()
        .as_ref()
        .map(|window| window.dialog());
    let chat = ui
        .chat_window
        .lock()
        .unwrap()
        .as_ref()
        .map(|window| window.dialog());
    match modal_kind(reader.is_some(), group.is_some(), chat.is_some()) {
        Some(ModalKind::Reader) => reader,
        Some(ModalKind::Group) => group,
        Some(ModalKind::Chat) => chat,
        None => None,
    }
}

fn apply_done(ui: &Ui, done: Done) {
    if let Err(error) = &done.result {
        blindfb_core::debug::note_failure(error);
    }
    let modal = modal_dialog(ui);
    let parent: &dyn WxWidget = if let Some(dialog) = &modal {
        dialog
    } else {
        &ui.frame
    };
    match done.tag {
        Tag::List {
            view,
            request,
            args,
            unlock_attempted,
            append,
        } => {
            let current = current_list(ui, &view, request);
            // Captured before finish() clears it: background reads (prefetch,
            // feed read-ahead) and a cached section refreshing itself must
            // not announce a list nobody is waiting for (#17).
            let background_read = ui.latest_list.lock().unwrap().background.contains(&view)
                || ui.state.borrow().background == Some(request);
            if !ui.latest_list.lock().unwrap().finish(&view, request) {
                return;
            }
            match done.result.and_then(|raw| parse_for(&view, &raw)) {
                Ok((records, body)) => {
                    let previous = {
                        let state = ui.state.borrow();
                        if current {
                            state.records.clone()
                        } else {
                            state
                                .cache
                                .get(&view)
                                .map(|(rows, _, _)| rows.clone())
                                .unwrap_or_default()
                        }
                    };
                    let before = previous.len();
                    let records = if append {
                        merge_records(&view, previous, records)
                    } else {
                        records
                    };
                    let added = records.len().saturating_sub(before);
                    if cacheable(&view, &args) {
                        ui.state.borrow_mut().cache.insert(
                            view.clone(),
                            (records.clone(), Instant::now(), args.clone()),
                        );
                    }
                    // A full first page of groups: read the rest ahead, even when
                    // it was a prefetch, so opening Groups shows the whole list.
                    if view == View::Groups
                        && args == groups_args(PAGE)
                        && records.len() >= PAGE as usize
                        && !ui.latest_list.lock().unwrap().pending.contains(&view)
                    {
                        send_ahead(ui, View::Groups, groups_args(GROUPS_AHEAD));
                    }
                    if !current {
                        return;
                    }
                    ui.state.borrow_mut().last_args = args;
                    let empty = records.is_empty();
                    let total = records.len();
                    show_records(ui, &view, records, body);
                    if !append && empty {
                        // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
                        ui.status(&tf!("{view}: no items to show.", view = view.title()));
                    }
                    if append {
                        let total = ui.state.borrow().records.len();
                        ui.status(&format!(
                            "{} {}",
                            ntf!("{n} new item loaded.", "{n} new items loaded.", added),
                            ntf!("{n} item total.", "{n} items total.", total)
                        ));
                    }
                    if !background_read {
                        alert_list_loaded(ui, &view, total, added, append);
                    }
                    if view == View::Feed && !append {
                        // Warm the other sections first: the feed's read-ahead
                        // re-scrolls the whole column, and a switch to Chats or
                        // Notifications should not queue behind that scroll.
                        if !ui.state.borrow().prefetched {
                            ui.state.borrow_mut().prefetched = true;
                            prefetch(ui);
                        }
                    }
                    if view == View::RecentFeed && !append && ui.items.get_count() > 0 {
                        load_recent_ahead(ui);
                    }
                }
                Err(e) => {
                    if !current {
                        return;
                    }
                    if matches!(view, View::Chats | View::ChatWith(..))
                        && needs_messenger_unlock(&e)
                    {
                        let mut state = ui.state.borrow_mut();
                        if !unlock_attempted && !state.unlocking {
                            state.unlocking = true;
                            state.pending_unlock = Some(RetryList {
                                view,
                                request,
                                args,
                                append,
                            });
                            drop(state);
                            ui.status(&t("Messenger needs its history restored. Enter your PIN in the unlock dialog."));
                        } else {
                            drop(state);
                            ui.status(&t("Messenger history is still locked. Use Chat → Unlock encrypted chats to retry."));
                            if unlock_attempted {
                                show_error(
                                    parent,
                                    t("Messenger still blocks input after unlocking. Check your PIN or restore message history in Messenger, then retry."),
                                    &t("Messenger history"),
                                );
                            }
                        }
                        return;
                    }
                    if ui.state.borrow().background == Some(request) {
                        // TRANSLATORS: {view} is a view title like "News feed" or "Chat with Ann".
                        ui.status(&tf!(
                            "Could not update {view}. Showing earlier results.",
                            view = view.title()
                        ));
                        return;
                    }
                    ui.status(&t("Error"));
                    show_error(parent, &e, "BlindFB");
                }
            }
        }
        Tag::PostVideo(session, request) => post_dialog::stream(ui, session, request, done.result),
        Tag::PostReelReacted(session) => {
            post_dialog::apply(ui, session, done.result, &t("Reaction set."), false)
        }
        Tag::ReelPrepared { account, target } => {
            {
                let mut state = ui.state.borrow_mut();
                if state.account != account {
                    return;
                }
                state
                    .reel_cache
                    .finish(&target, &done.result, Instant::now());
            }
            if let Some(info) = done
                .result
                .as_ref()
                .ok()
                .and_then(|data| blindfb_core::media::parse_stream(data).ok())
            {
                update_reel_titles(ui, &target, &info.title);
            }
            post_dialog::prepared(ui, &target, done.result);
            prepare_reels(ui);
        }
        Tag::GroupPosts(session) => group_dialog::deliver_posts(ui, session, done.result),
        Tag::GroupMembers(session) => group_dialog::deliver_members(ui, session, done.result),
        Tag::ChatMessages(session) => chat_dialog::deliver_messages(ui, session, done.result),
        Tag::ChatWindowSent(session) => chat_dialog::deliver_sent(ui, session, done.result),
        Tag::PostLoaded(session) => {
            post_dialog::apply(ui, session, done.result, &t("Post ready."), false)
        }
        Tag::PostSent(session) => {
            post_dialog::apply(ui, session, done.result, &t("Comment sent."), true)
        }
        Tag::PostParticipation(session, submitted) => {
            post_dialog::participation_done(ui, session, done.result, submitted)
        }
        Tag::PostLiked(session) => {
            post_dialog::apply(ui, session, done.result, &t("Like changed."), false)
        }
        Tag::PostShareReady(session) => post_dialog::share_ready(ui, session, done.result),
        Tag::PostShared(session) => post_dialog::note(
            ui,
            session,
            done.result.map(|_| t("Shared to your feed.")),
            true,
        ),
        Tag::PostShareClosed(session) => post_dialog::note(
            ui,
            session,
            done.result
                .map(|_| t("Share cancelled. Nothing was shared.")),
            true,
        ),
        Tag::PostNote(session) => post_dialog::note(
            ui,
            session,
            done.result.map(|_| t("Reaction set.")),
            false,
        ),
        Tag::Silent => {}
        Tag::UnlockRetry {
            view,
            request,
            args,
            append,
        } => {
            ui.state.borrow_mut().unlocking = false;
            match done.result {
                Ok(text) if !args.is_empty() && current_list(ui, &view, request) => {
                    ui.latest_list.lock().unwrap().pending.insert(view.clone());
                    ui.send(Job {
                        task: Task::Fb(args.clone(), None),
                        tag: Tag::List {
                            view,
                            request,
                            args,
                            unlock_attempted: true,
                            append,
                        },
                    });
                    // The backend distinguishes a finished restore from a
                    // PIN Facebook accepted and is still verifying: report
                    // the second honestly instead of promising chats now.
                    ui.status(&if text.contains("still restoring") {
                        t("PIN accepted. Facebook is still restoring your chat history — loading your chats…")
                    } else {
                        t("Messenger unlocked. Loading your chats…")
                    });
                }
                Ok(text) => {
                    ui.latest_list.lock().unwrap().finish(&view, request);
                    ui.status(&if text.contains("still restoring") {
                        t("PIN accepted. Facebook is still restoring your chat history; chats unlock as it finishes.")
                    } else {
                        t("Messenger unlocked.")
                    });
                }
                Err(e) => {
                    ui.latest_list.lock().unwrap().finish(&view, request);
                    ui.status(&t("Messenger could not be unlocked."));
                    show_error(
                        parent,
                        tf!(
                            "Messenger could not restore your history. Check the PIN, or restore history in Messenger and retry.\n\n{error}",
                            error = e
                        ),
                        &t("Unlock encrypted chats"),
                    );
                }
            }
        }
        Tag::AlertNotifications => {
            let items = done.result.and_then(|output| alerts::notifications(&output));
            alert_on_new(ui, items, |state| &mut state.alert_notifications);
        }
        Tag::AlertChats => {
            let items = done.result.map(|output| alerts::chats(&output));
            alert_on_new(ui, items, |state| &mut state.alert_chats);
        }
        Tag::LoginRequest => {
            let request = done.result.and_then(|raw| blindfb_core::login_approval::parse(&raw));
            match request {
                Ok(request) if request.choices.is_empty() => {
                    ui.send(Job {
                        task: Task::Fb(
                            ["page", blindfb_core::login_approval::PAGE, "close"]
                                .map(str::to_owned)
                                .to_vec(),
                            None,
                        ),
                        tag: Tag::Silent,
                    });
                    ui.status(&t("No login request is waiting."));
                    let text = tf!(
                        "This login request is no longer waiting for an answer. It may have expired or been answered already.\n\n{page}",
                        page = request.text
                    );
                    show_warning(parent, text.trim(), &t("Login request"));
                }
                Ok(request) => {
                    ui.status(&t("Login request"));
                    let labels: Vec<&str> = request.choices.iter().map(String::as_str).collect();
                    let dialog = SingleChoiceDialog::builder(
                        parent,
                        &request.text,
                        &t("Login request"),
                        &labels,
                    )
                    .build();
                    // Facebook's own advice: when unsure, the last answer ("No, it's not me").
                    dialog.set_selection(labels.len() as i32 - 1);
                    let choice = (dialog.show_modal() == ID_OK)
                        .then(|| dialog.get_string_selection())
                        .flatten();
                    dialog.destroy();
                    match choice {
                        Some(choice) => {
                            ui.status(&tf!("Answering: {answer}…", answer = choice));
                            ui.send(Job {
                                task: Task::LoginAnswer(choice),
                                tag: Tag::LoginAnswered,
                            });
                        }
                        None => {
                            ui.send(Job {
                                task: Task::Fb(
                                    ["page", blindfb_core::login_approval::PAGE, "close"]
                                        .map(str::to_owned)
                                        .to_vec(),
                                    None,
                                ),
                                tag: Tag::Silent,
                            });
                            ui.status(&t("Login request left unanswered."));
                        }
                    }
                }
                Err(e) => {
                    ui.status(&t("Error"));
                    show_error(parent, &e, &t("Login request"));
                }
            }
        }
        Tag::PostMenu { post, path } => match done.result {
            Ok(output) => post_menu_step(ui, post, path, &output),
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, &t("Facebook post menu"));
            }
        },
        Tag::LoginAnswered => {
            match done.result.and_then(|raw| blindfb_core::login_approval::parse(&raw)) {
                Ok(result) => {
                    let message = if result.answered {
                        t("Facebook took your answer.")
                    } else {
                        t("Your answer was pressed, but Facebook's login request is still showing. Open the notification again to check.")
                    };
                    ui.status(&message);
                    let text = format!("{message}\n\n{}", result.text);
                    let dialog =
                        MessageDialog::builder(parent, text.trim(), &t("Login request")).build();
                    dialog.show_modal();
                    dialog.destroy();
                }
                Err(e) => {
                    ui.status(&t("Error"));
                    show_error(parent, &e, &t("Login request"));
                }
            }
        }
        Tag::DebugReport => match done.result {
            Ok(text) => {
                ui.status(&t("Debug report saved."));
                let dialog = MessageDialog::builder(parent, &text, &t("Debug report")).build();
                dialog.show_modal();
                dialog.destroy();
            }
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, &t("Debug report"));
            }
        },
        Tag::Post => match done.result {
            Ok(text) => {
                let message = post_done_message(&text);
                set_details(ui, &message, None);
                ui.status(&message);
            }
            Err(e) => {
                ui.status(&t("Error"));
                match post_unconfirmed_message(&e) {
                    Some(message) => show_warning(parent, message, &t("New post")),
                    None => show_error(parent, &e, "BlindFB"),
                }
            }
        },
        Tag::Text => match done.result {
            Ok(text) => {
                set_details(ui, text.trim(), None);
                ui.status(&t("Done"));
            }
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, "BlindFB");
            }
        },
        Tag::Commented { post } => match done.result {
            Ok(text) => {
                set_details(ui, text.trim(), None);
                ui.status(&t("Comment sent."));
                if matches!(&ui.state.borrow().view, View::CommentsOf(current) if current == &post)
                {
                    refresh(ui);
                }
            }
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, "BlindFB");
            }
        },
        Tag::GroupJoin { group } => match done.result {
            Ok(text) => {
                let text = text.trim().to_owned();
                if text.starts_with("questions\t") {
                    answer_group_questions(ui, &group, &text);
                } else {
                    set_details(ui, &text, None);
                    ui.status(&t("Done"));
                }
            }
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, &t("Join group"));
            }
        },
        Tag::GroupLeave => match done.result {
            Ok(text) => {
                set_details(ui, text.trim(), None);
                ui.status(&t("Done"));
                if ui.state.borrow().view == View::Groups {
                    load_view(ui, View::Groups);
                }
            }
            Err(e) => {
                ui.status(&t("Error"));
                show_error(parent, &e, &t("Leave group"));
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
                            &view,
                            &ui.compose_box.get_value(),
                            &mut state.attachments,
                        );
                        (
                            clear,
                            matches!(&view, View::ChatWith(id, _) if id == &submitted.chat),
                        )
                    };
                    if clear {
                        ui.compose_box.clear();
                    }
                    refresh_attachment_count(ui);
                    ui.state.borrow_mut().reply_to = None;
                    ui.status(&t("Sent"));
                    if current {
                        load_view(
                            ui,
                            View::ChatWith(submitted.chat.clone(), submitted.name.clone()),
                        );
                    }
                }
                Err(e) => {
                    let kept = ui.state.borrow().attachments.len();
                    ui.status(&if kept > 0 {
                        t("Message could not be sent. Your draft and attachments have been kept. Use the Attachments button to remove a file, for example one over Messenger's 25 MB limit.")
                    } else {
                        t("Message could not be sent. Your draft has been kept.")
                    });
                    show_error(parent, &e, &t("Send message"));
                }
            }
        }
        Tag::ReloadChat(chat, name, account) => {
            if ui.state.borrow().account != account {
                return;
            }
            match done.result {
                Ok(_) => {
                    ui.status(&t("Message removed"));
                    if matches!(&ui.state.borrow().view, View::ChatWith(id, _) if id == &chat) {
                        load_view(ui, View::ChatWith(chat, name));
                    }
                }
                Err(e) => {
                    ui.status(&t("Error"));
                    show_error(parent, &e, "BlindFB");
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
                ui.status(&t("The backend is installed but not signed in."));
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status(&t("Signed out. Use Backend → Sign in when ready."));
                }
            }
            Err(_) if Fb::discover().is_none() => {
                startup_log("setup: backend missing — installing");
                ui.status(&t("Installing the Facebook backend…"));
                ui.send(Job {
                    task: Task::InstallBackend,
                    tag: Tag::SetupInstall,
                });
            }
            Err(error) => {
                ui.status(&t("The Facebook backend could not start."));
                show_error(parent, error, &t("Backend startup"));
                offer_sign_in(ui, Tag::SetupLogin);
            }
        },
        Tag::SetupInstall => match done.result {
            Ok(summary) => {
                ui.status(&summary);
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status(&t("Signed out. Use Backend → Sign in when ready."));
                }
            }
            Err(error) => {
                show_error(
                    parent,
                    tf!(
                        "The backend could not be installed automatically: {error}\nYou can retry from Backend → Install or update backend.",
                        error = error
                    ),
                    &t("Backend install failed"),
                );
            }
        },
        Tag::SetupLogin => match done.result {
            Ok(out) if out.contains("logged in") && !out.contains("not logged in") => {
                ui.status(&t("Signed in."));
                load_view(ui, View::Feed);
            }
            Ok(out) => {
                set_details(ui, out.trim(), None);
                ui.status(&t("Sign-in did not finish — see the details, or try Backend → Sign in."));
                show_warning(parent, out.trim(), &t("Sign-in did not finish"));
            }
            Err(error) => {
                show_error(parent, error, &t("Sign in"));
                ui.status(&t("Sign-in failed. Use Backend → Sign in to retry."));
            }
        },
        Tag::BrowserInstalled => match done.result {
            Ok(summary) => {
                ui.status(&summary);
                if !offer_sign_in(ui, Tag::SetupLogin) {
                    ui.status(&t("Signed out. Use Backend → Sign in when ready."));
                }
            }
            Err(error) => {
                show_error(parent, error, &t("Browser install"));
                ui.status(&t("Browser installation failed."));
            }
        },
        Tag::BrowserInstalledMenu => match done.result {
            Ok(summary) => ui.status(&summary),
            Err(error) => show_error(parent, error, &t("Browser install")),
        },
    }
}

fn parse_for(view: &View, raw: &str) -> Result<(Vec<Record>, Option<String>), String> {
    Ok(match view {
        View::Notifications => (parse_notifications(raw)?, None),
        View::Reels(_) => reels::parse(raw)?,
        View::Feed
        | View::RecentFeed
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Saved => (parse_feed(raw), None),
        View::ChatWith(..) => (
            raw.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| Record {
                    author_ref: None,
                    links: Vec::new(),
                    fields: vec![l.to_owned()],
                    raw: l.to_owned(),
                })
                .collect(),
            None,
        ),
        View::CommentsOf(_) => {
            let mut body_lines = Vec::new();
            let mut comments: Vec<Record> = Vec::new();
            // A `cu` line directly before a comment carries the commenter's
            // profile path; it becomes the comment record's second field.
            let mut pending_ref = String::new();
            for line in raw.lines() {
                if let Some(rest) = line.strip_prefix("cl\t") {
                    // A `cl` line directly after a comment carries one of
                    // the links inside that comment's text.
                    if let Some(comment) = comments.last_mut() {
                        let mut parts = rest.splitn(2, '\t');
                        let url = parts.next().unwrap_or("");
                        if !url.is_empty() {
                            comment.links.push(Link {
                                label: parts.next().unwrap_or(url).to_owned(),
                                url: url.to_owned(),
                                profile: None,
                            });
                        }
                    }
                } else if let Some(ref_) = line.strip_prefix("cu\t") {
                    pending_ref = ref_.to_owned();
                } else if line.starts_with("c\t") {
                    let mut fields: Vec<String> = line.split('\t').map(str::to_owned).collect();
                    fields.insert(1, std::mem::take(&mut pending_ref));
                    comments.push(Record {
                        author_ref: None,
                        links: Vec::new(),
                        fields,
                        raw: line.to_owned(),
                    });
                } else if !line.trim().is_empty() {
                    // A ref belongs to the comment directly after it; a body
                    // line in between means it was not a comment's ref.
                    pending_ref.clear();
                    body_lines.push(line);
                }
            }
            (comments, Some(body_lines.join("\n")))
        }
        View::ProfileOf(_) => (
            raw.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    // Backend profile lines are "key<TAB>value" (name,
                    // url, friends, intro, actions, one row per info
                    // list); keep the two fields apart so the row can
                    // read and act as what it is (issue #27).
                    let mut parts = l.splitn(2, '\t');
                    let key = parts.next().unwrap_or("").to_owned();
                    let value = parts.next().unwrap_or("").to_owned();
                    Record {
                        author_ref: None,
                        links: Vec::new(),
                        fields: vec![key, value],
                        raw: l.to_owned(),
                    }
                })
                .collect(),
            Some(raw.trim().to_owned()),
        ),
        _ => (parse_records(raw), None),
    })
}

/// The backend's errors while Messenger history is locked: the restore PIN
/// prompt covering the page, or a thread that never loaded because of it.
/// Any of them should offer the unlock dialog instead of a dead end.
fn needs_messenger_unlock(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("history prompt blocked input")
        || error.contains("e2ee chat locked")
        || error.contains("messenger is locked")
}

/// Ask for the Messenger PIN (stdin only, never stored). With `retry`, the
/// failed list request runs once more after a successful unlock.
fn unlock_messenger(ui: &Ui, retry: Option<RetryList>) {
    let modal = modal_dialog(ui);
    let parent: &dyn WxWidget = if let Some(dialog) = &modal {
        dialog
    } else {
        &ui.frame
    };
    let dialog = TextEntryDialog::builder(parent, &t("Messenger PIN:"), &t("Unlock encrypted chats"))
        .password()
        .build();
    let pin = (dialog.show_modal() == ID_OK)
        .then(|| dialog.get_value())
        .flatten();
    dialog.destroy();
    let Some(pin) = pin.filter(|p| !p.is_empty()) else {
        ui.state.borrow_mut().unlocking = false;
        ui.status(&t("Unlock cancelled."));
        return;
    };
    // The automatic path passes the load it was resuming. The manual menu item
    // passes nothing, so reload the section the user is looking at; otherwise
    // entering the PIN leaves the chat list as empty as it was, with the only
    // hint in a status bar a screen reader does not announce.
    let retry = retry.unwrap_or_else(|| {
        let view = ui.state.borrow().view.clone();
        let args = default_args(&view).unwrap_or_default();
        let request = if args.is_empty() {
            0
        } else {
            begin_list(ui, &view)
        };
        RetryList {
            view,
            request,
            args,
            append: false,
        }
    });
    ui.state.borrow_mut().unlocking = true;
    ui.status(&t("Unlocking Messenger…"));
    ui.send(Job {
        task: Task::Fb(vec!["unlock".into(), "-".into()], Some(format!("{pin}\n"))),
        tag: Tag::UnlockRetry {
            view: retry.view,
            request: retry.request,
            args: retry.args,
            append: retry.append,
        },
    });
}

fn open_facebook_link(ui: &Ui) {
    let dialog = TextEntryDialog::builder(
        &ui.frame,
        &t("Facebook post, video or reel link (full https:// URL):"),
        &t("Open Facebook link"),
    )
    .build();
    let target = (dialog.show_modal() == ID_OK)
        .then(|| dialog.get_value())
        .flatten();
    dialog.destroy();
    let Some(target) = target else { return };
    let target = target.trim();
    match (target.starts_with("https://"), blindfb_core::posts::reader_url(target)) {
        (true, Some(url)) => post_dialog::open(ui, &url, false),
        _ => show_error(
            &ui.frame,
            t("Paste a full https:// Facebook post, video or reel link. This link is invalid or unsupported."),
            &t("Open Facebook link"),
        ),
    }
}

fn open_selected(ui: &Ui) {
    let more = ui.items.get_selection().is_some_and(|index| {
        let state = ui.state.borrow();
        state.view == state.records_view
            && is_more_row(&state.view, state.records.len(), index as usize)
    });
    if more {
        load_more(ui);
        return;
    }
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    if view == View::Notifications {
        if blindfb_core::login_approval::is_request(record.field(3)) {
            ui.status(&t("Reading the login request…"));
            ui.send(Job {
                task: Task::LoginRead(record.field(3).to_owned()),
                tag: Tag::LoginRequest,
            });
            return;
        }
        if let Some(source) = reels::notification_source(&record) {
            load_view(ui, View::Reels(source));
            return;
        }
        // "Ann shared 9 posts" links one of them: show the whole list instead.
        match blindfb_core::posts::notification_posts(record.field(2), record.field(3)) {
            Some(blindfb_core::posts::NotificationPosts::Person(who)) => {
                load_view(ui, View::PostsOf(who));
                return;
            }
            Some(blindfb_core::posts::NotificationPosts::Group(group)) => {
                load_view(ui, View::GroupPosts(group));
                return;
            }
            None => {}
        }
    }
    if matches!(view, View::SearchResults | View::Marketplace) {
        open_search_result(ui, &record);
        return;
    }
    match view {
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_)
        | View::Saved
        | View::Notifications => {
            let Some(target) = post_target(&view, &record) else {
                show_warning(
                    &ui.frame,
                    t("This row has no link, so there is no post to open. Sponsored and suggested rows usually have none."),
                    &t("Open post"),
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
                        (
                            state
                                .reel_cache
                                .ready(&url, Instant::now())
                                .map(str::to_owned),
                            state.reel_cache.is_preparing(&url),
                        )
                    };
                    post_dialog::open_reel(ui, &url, stream, preparing);
                }
                Some(url) => post_dialog::open(ui, &url, false),
                None if view == View::Notifications => {
                    open_url(&web_url(target));
                    ui.status(&t("Opened notification on Facebook."));
                }
                None => {
                    // Photo albums and other pages the reader cannot parse yet.
                    if confirm(
                        &ui.frame,
                        t("BlindFB cannot read this kind of item yet (for example a photo album). Open it in your web browser?"),
                        &t("Open post"),
                    ) {
                        open_url(&web_url(target));
                    }
                }
            }
        }
        View::Chats => chat_dialog::open(ui, &record),
        View::Friends => profile_selected(ui),
        View::Groups => group_dialog::open(ui, &record),
        View::Pages => {
            // A page's posts read like a profile's posts.
            let url = record.field(1).to_owned();
            if url.is_empty() || url == "-" {
                show_warning(
                    &ui.frame,
                    t("This page has no link, so there is nothing to open."),
                    &t("Open page"),
                );
            } else {
                load_view(ui, View::PostsOf(url));
            }
        }
        View::Events | View::Places => {
            // Events and places have no reader view yet: open the page
            // itself in the browser, like a search row of the same kind.
            let url = record.field(1).to_owned();
            if url.is_empty() || url == "-" {
                show_warning(
                    &ui.frame,
                    t("This row has no link, so there is nothing to open."),
                    &t("Open"),
                );
            } else {
                open_url(&web_url(&url));
                ui.status(&t("Opened on Facebook in your browser."));
            }
        }
        View::ProfileOf(who) => match record.field(0) {
            // The Actions row lists the actions Facebook shows on this
            // profile (issue #27): Enter presents the real actions menu
            // instead of doing nothing in silence.
            "actions" => show_item_menu(ui),
            "url" => {
                let url = record.field(1).to_owned();
                if url.is_empty() {
                    ui.status(&t("This profile row has no link to open."));
                } else {
                    open_url(&web_url(&url));
                    ui.status(&t("Opened on Facebook in your browser."));
                }
            }
            "name" => load_view(ui, View::PostsOf(who.clone())),
            _ => ui.status(&t(
                "Information only. Applications or Shift+F10 opens this person's actions.",
            )),
        },
        _ => {}
    }
}

/// The selected post's target, or a spoken warning saying why there is none.
fn post_or_warn(ui: &Ui, title: &str) -> Option<String> {
    let post = selected_post_ref(ui);
    if post.is_none() {
        let why = if selected(ui).is_some() {
            t("This row has no link, so BlindFB cannot open, react to or comment on it. Sponsored and suggested rows usually have none.")
        } else {
            t("Select a post first.")
        };
        show_warning(&ui.frame, why, title);
    }
    post
}

fn react_popup(ui: &Ui) {
    let Some(post) = post_or_warn(ui, &t("React")) else {
        return;
    };
    let names: Vec<String> = REACTIONS.iter().map(|reaction| reaction_name(reaction)).collect();
    let labels: Vec<&str> = names.iter().map(String::as_str).collect();
    let dialog =
        SingleChoiceDialog::builder(&ui.frame, &t("Reaction"), &t("React"), &labels).build();
    let reaction = if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        (index >= 0).then(|| REACTIONS[index as usize])
    } else {
        None
    };
    dialog.destroy();
    if let Some(reaction) = reaction {
        react_to(ui, &post, reaction);
    }
}

fn react_to(ui: &Ui, post: &str, reaction: &str) {
    if post_dialog::is_video_page(post) {
        post_dialog::react(ui, post, reaction);
    } else {
        ui.send(Job {
            task: Task::Fb(react_args(post, reaction), None),
            tag: Tag::Text,
        });
    }
}

fn comment_selected(ui: &Ui) {
    let Some(post) = post_or_warn(ui, &t("Comment")) else {
        return;
    };
    if post_dialog::is_video_page(&post) {
        post_dialog::comment(ui, &post);
        return;
    }
    if let Some(text) = prompt_text(&ui.frame, t("Your comment:"), &t("Comment")) {
        if !text.trim().is_empty() {
            ui.send(Job {
                task: Task::Fb(comment_args(&post, &text), None),
                tag: Tag::Commented { post },
            });
        }
    }
}

/// The poster named by a feed author line. A joined line ("Sharer /
/// Original author", a shared or "is with" post) names two people, so a
/// name search for the whole line matches nobody and the backend answers
/// "no person matches" (issue #14); the first name is the poster.
fn poster_name(author: &str) -> &str {
    author.split(" / ").next().unwrap_or(author).trim()
}

/// The person a selected record is about: a friend record's ref, a post's
/// author name, or a chat's name.
fn person_of(ui: &Ui, record: &Record) -> Option<String> {
    let view = ui.state.borrow().view.clone();
    match view {
        View::ProfileOf(who) => Some(who.clone()),
        View::Friends => Some(record.field(1).to_owned()),
        View::Chats => Some(record.field(3).to_owned()),
        View::CommentsOf(_) => {
            let ref_ = record.field(1);
            if ref_.is_empty() {
                Some(record.field(2).to_owned())
            } else {
                Some(ref_.to_owned())
            }
        }
        _ => {
            // A feed post's author ref is the person's identity: the name
            // alone cannot resolve a group post's author (the group name is
            // merged into it), and a name search can pick the wrong person.
            if let Some(ref_) = record.author_ref.as_deref().filter(|r| !r.is_empty()) {
                return Some(ref_.to_owned());
            }
            let author = record.field(1);
            if author.is_empty() {
                None
            } else {
                Some(poster_name(author).to_owned())
            }
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

/// Send a friend request to the person the selected record is about (a
/// comment's author, from the comments menu). The backend reports existing
/// friends and pending requests instead of re-sending.
fn friend_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let Some(who) = person_of(ui, &record) else {
        return;
    };
    if who.is_empty() {
        return;
    }
    let name = match ui.state.borrow().view {
        View::CommentsOf(_) => record.field(2).to_owned(),
        _ => who.clone(),
    };
    ui.status(&tf!("Sending a friend request to {name}…", name = name));
    ui.send(Job {
        task: Task::Fb(friend_add_args(&who), None),
        tag: Tag::Text,
    });
}

fn group_posts_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    if let Some(target) = post_target(&view, &record) {
        if let Some(group) = blindfb_core::posts::group_url(target) {
            load_view(ui, View::GroupPosts(group));
        }
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

/// The Messenger target of the selected record: the backend chat spec and the
/// label to show. The spec is a ref when the row has one, because names repeat
/// across chats and a name is not an identity; `fb` resolves a person's ref to
/// their thread.
fn message_target(view: &View, record: &Record) -> (String, String) {
    match view {
        View::ProfileOf(who) => (who.clone(), who.clone()),
        View::Friends => (record.field(1).to_owned(), record.field(0).to_owned()),
        View::SearchResults => (record.field(1).to_owned(), record.field(0).to_owned()),
        View::Chats => (record.field(0).to_owned(), record.field(3).to_owned()),
        View::CommentsOf(_) => {
            let id = record.field(1);
            let id = if id.is_empty() { record.field(2) } else { id };
            (id.to_owned(), record.field(2).to_owned())
        }
        _ => {
            let name = record.field(1).to_owned();
            (name.clone(), name)
        }
    }
}

fn message_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    let (id, name) = message_target(&view, &record);
    if !name.is_empty() {
        load_view(ui, View::ChatWith(id, name));
    }
}

/// Link out to the selected post on the web. Feed and post views load with
/// --urls, so a post record's first field is its web address.
fn post_menu_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let view = ui.state.borrow().view.clone();
    let Some(post) = post_target(&view, &record).map(str::to_owned) else {
        show_warning(
            &ui.frame,
            t("This row has no link, so there is no post menu to open."),
            &t("Facebook post menu"),
        );
        return;
    };
    send_post_menu(ui, post, Vec::new());
}

fn send_post_menu(ui: &Ui, post: String, path: Vec<String>) {
    ui.status(&t("Opening Facebook's post menu…"));
    ui.send(Job {
        task: Task::Fb(blindfb_core::post_menu::args(&post, &path), None),
        tag: Tag::PostMenu { post, path },
    });
}

/// Show what Facebook's post menu shows now, and press the user's pick on the next step.
fn post_menu_step(ui: &Ui, post: String, path: Vec<String>, output: &str) {
    let step = blindfb_core::post_menu::parse(output);
    let caption = t("Facebook post menu");
    if step.done || step.choices.is_empty() {
        let mut message = match path.last() {
            Some(last) => tf!("Done: {choice}.", choice = last),
            None => t("Facebook showed no menu for this post."),
        };
        for note in &step.notes {
            message.push('\n');
            message.push_str(note);
        }
        ui.status(&message);
        let dialog = MessageDialog::builder(&ui.frame, &message, &caption).build();
        dialog.show_modal();
        dialog.destroy();
        return;
    }
    let mut message: Vec<String> = Vec::new();
    if path.is_empty() && !step.feed_card {
        message.push(t("This is the post's own menu. Interested and Not interested exist only on the post's card in the News feed, and Facebook has already dropped this card from its feed page; reload the News feed to get them for newer posts."));
    }
    if !step.title.is_empty() {
        message.push(step.title.clone());
    }
    message.extend(step.text.iter().cloned());
    message.extend(step.notes.iter().cloned());
    let labels: Vec<String> = step
        .choices
        .iter()
        .map(|choice| {
            let mut label = choice.label.clone();
            if choice.on {
                // TRANSLATORS: {choice} is a menu item Facebook shows as switched on.
                label = tf!("{choice} (on)", choice = label);
            }
            if !choice.detail.is_empty() {
                label = format!("{label}: {}", choice.detail);
            }
            label
        })
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let text = if message.is_empty() { t("Choose an action") } else { message.join("\n") };
    let dialog = SingleChoiceDialog::builder(&ui.frame, &text, &caption, &label_refs).build();
    let picked = (dialog.show_modal() == ID_OK).then(|| dialog.get_selection());
    dialog.destroy();
    match picked.and_then(|index| usize::try_from(index).ok()).and_then(|i| step.choices.get(i)) {
        Some(choice) => {
            let mut path = path;
            path.push(choice.label.clone());
            send_post_menu(ui, post, path);
        }
        None => ui.status(&t("Facebook post menu closed.")),
    }
}

fn open_post_web(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let target = match ui.state.borrow().view {
        View::Notifications => record.field(3),
        View::SearchResults => search_web_target(&record),
        _ => record.field(0),
    };
    if target.starts_with("http") || target.starts_with('/') {
        open_url(&web_url(target));
    } else {
        show_warning(
            &ui.frame,
            t("This row has no web link (it is a suggestion or ad row)."),
            &t("Open on Facebook"),
        );
    }
}

/// A search row's web address: the backend's fifth field when present.
/// Rows from an older backend fall back to the target, which is a URL for
/// photos and videos but a daemon ref — not a link — for the other kinds.
fn search_web_target(record: &Record) -> &str {
    let url = record.field(4).trim();
    if !url.is_empty() {
        return url;
    }
    record.field(1).trim()
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
    if let View::ProfileOf(who) = &view {
        // The profile itself is the person: open it directly when it is
        // a path or address, else fall back to a people search by name.
        let who = who.clone();
        if who.starts_with("http") || who.starts_with('/') {
            open_url(&web_url(&who));
        } else if let Some(url) = ui.state.borrow().author_urls.get(&who).cloned() {
            open_url(&url);
        } else {
            let query = who.replace(' ', "%20");
            open_url(&format!(
                "https://www.facebook.com/search/people/?q={query}"
            ));
        }
        return;
    }
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
            open_url(&format!(
                "https://www.facebook.com/search/people/?q={query}"
            ));
        }
    }
}

/// Link out to the selected chat in Messenger on the web, which is where
/// voice and video calls live. Deep-links the thread when the person's
/// profile URL is known, else opens the Messenger inbox.
fn open_chat_web(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    let name = match &view {
        View::ChatWith(_, name) => name.clone(),
        View::Chats => match selected(ui) {
            Some((_, record)) => record.field(3).to_owned(),
            None => return,
        },
        _ => {
            show_warning(
                &ui.frame,
                t("Select or open a chat first."),
                &t("Open in Messenger"),
            );
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
        show_warning(&ui.frame, t("Select one of your posts first."), &t("Delete post"));
        return;
    };
    if confirm(
        &ui.frame,
        t("Delete this post? Facebook moves it to the trash (permanent removal after 30 days)."),
        &t("Delete post"),
    ) {
        ui.send(Job {
            task: Task::Fb(delete_args(&post), None),
            tag: Tag::Text,
        });
    }
}

fn attach_files(ui: &Ui) {
    let dialog = FileDialog::builder(&ui.frame).build();
    if dialog.show_modal() == ID_OK {
        if let Some(path) = dialog.get_path() {
            let mut state = ui.state.borrow_mut();
            state.attachments.push(path);
            ui.status(&ntf!("{n} file attached", "{n} files attached", state.attachments.len()));
        }
    }
    refresh_attachment_count(ui);
}

/// The attachment manager: queued files with a way to remove one or all.
/// Attachments used to be add-only, so a single file over Messenger's
/// 25 MB limit poisoned every later send in the chat — the error came
/// back even for a plain message, with no way out but reopening the chat.
fn manage_files_dialog(parent: &dyn WxWidget, files: &mut Vec<String>) {
    if files.is_empty() {
        return;
    }
    let dialog = Dialog::builder(parent, &t("Attached files"))
        .with_size(640, 420)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&dialog)
        .with_label(&t("Attached &files"))
        .build();
    let list = ListBox::builder(&dialog).build();
    list.set_name(&t("Attached files"));
    let fill = |list: &ListBox, files: &[String]| {
        list.clear();
        for path in files {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            list.append(&name);
        }
        if !files.is_empty() {
            list.set_selection(0, true);
        }
    };
    fill(&list, files);
    let remove = Button::builder(&dialog)
        .with_label(&t("&Remove selected"))
        .build();
    let remove_all = Button::builder(&dialog).with_label(&t("Remove &all")).build();
    let close = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("C&lose"))
        .build();
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    actions.add(&remove, 0, SizerFlag::All, 4);
    actions.add(&remove_all, 0, SizerFlag::All, 4);
    actions.add(&close, 0, SizerFlag::All, 4);
    root.add(&label, 0, SizerFlag::All, 4);
    root.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 4);
    dialog.set_sizer(root, true);
    let working = Rc::new(RefCell::new(files.clone()));
    let (w, l) = (working.clone(), list);
    remove.on_click(move |_| {
        if let Some(index) = l.get_selection() {
            let mut files = w.borrow_mut();
            if (index as usize) < files.len() {
                files.remove(index as usize);
            }
            fill(&l, &files);
        }
    });
    let (w, l) = (working.clone(), list);
    remove_all.on_click(move |_| {
        w.borrow_mut().clear();
        fill(&l, &[]);
    });
    close.on_click(move |_| dialog.end_modal(ID_CANCEL));
    let d = dialog;
    dialog.bind_internal(EventType::CLOSE_WINDOW, move |event| {
        event.skip(false);
        d.end_modal(ID_CANCEL);
    });
    list.set_focus();
    dialog.show_modal();
    dialog.destroy();
    *files = working.borrow().clone();
}

fn manage_attachments(ui: &Ui) {
    let mut files = ui.state.borrow().attachments.clone();
    if files.is_empty() {
        ui.status(&t("No files are attached."));
        return;
    }
    manage_files_dialog(&ui.frame, &mut files);
    let count = files.len();
    ui.state.borrow_mut().attachments = files;
    refresh_attachment_count(ui);
    ui.status(&ntf!("{n} file attached", "{n} files attached", count));
}

fn refresh_attachment_count(ui: &Ui) {
    let count = ui.state.borrow().attachments.len();
    ui.attachments_btn
        .set_label(&tf!("Attachments ({count})", count = count));
    ui.attachments_btn
        .set_name(&ntf!("Attachments, {n} attached", "Attachments, {n} attached", count));
}

/// The message text a chat line is matched by: `fb read` lines are
/// "HH:MM Sender: text {reactions}"; the backend matches the bare text.
fn message_match_text(line: &str) -> String {
    let after_time = line.split_once(' ').map(|(_, rest)| rest).unwrap_or(line);
    let text = after_time
        .split_once(": ")
        .map(|(_, rest)| rest)
        .unwrap_or(after_time);
    let text = match text.rfind(" {") {
        Some(i) if text.ends_with('}') => &text[..i],
        _ => text,
    };
    text.trim().to_owned()
}

/// The picked message's match text plus its `--nth`: how many NEWER
/// messages (below it in the list) the backend's contains-match would
/// count before reaching it. Duplicate texts ("ok", "thanks", a link
/// sent twice) otherwise always act on the newest copy, not the picked
/// one.
fn match_nth(texts: &[String], index: usize) -> Option<(String, usize)> {
    let target = texts.get(index)?.clone();
    if target.is_empty() {
        return None;
    }
    let nth = texts[index + 1..]
        .iter()
        .filter(|t| !t.is_empty() && t.contains(target.as_str()))
        .count();
    Some((target, nth))
}

fn selected_message_match(ui: &Ui) -> Option<(String, usize)> {
    let (index, _) = selected(ui)?;
    let state = ui.state.borrow();
    let texts: Vec<String> = state
        .records
        .iter()
        .map(|r| message_match_text(r.field(0)))
        .collect();
    match_nth(&texts, index)
}

fn reply_selected(ui: &Ui) {
    if ui.state.borrow().reply_to.is_some() {
        ui.state.borrow_mut().reply_to = None;
        ui.status(&t("Reply cancelled. Your next message sends normally."));
        return;
    }
    let Some((text, nth)) = selected_message_match(ui) else {
        ui.status(&t("That message has no text to reply to."));
        return;
    };
    ui.state.borrow_mut().reply_to = Some((text.clone(), nth));
    ui.status(&tf!(
        "Replying to: {text}. Write your reply in the message box and send as usual.",
        text = text
    ));
    ui.compose_box.set_focus();
}

fn msg_react_selected(ui: &Ui, reaction: &str) {
    let View::ChatWith(chat, _) = ui.state.borrow().view.clone() else {
        return;
    };
    let Some((text, nth)) = selected_message_match(ui) else {
        ui.status(&t("That message has no text to react to."));
        return;
    };
    ui.send(Job {
        task: Task::Fb(msg_react_args(&chat, reaction, Some(&text), nth), None),
        tag: Tag::Text,
    });
    ui.status(&tf!(
        "Reacting {reaction} to: {text}",
        reaction = reaction_name(reaction),
        text = text
    ));
}

fn send_composed(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    let View::ChatWith(chat, name) = view else {
        show_warning(&ui.frame, t("Open a chat first, then write and send."), &t("Send"));
        return;
    };
    let text = ui.compose_box.get_value();
    let files = ui.state.borrow().attachments.clone();
    if text.trim().is_empty() && files.is_empty() {
        return;
    }
    let account = ui.state.borrow().account;
    let submitted = ChatSubmission {
        account,
        chat: chat.clone(),
        name: name.clone(),
        text: text.clone(),
        files: files.clone(),
    };
    if !ChatSubmission::begin(&mut ui.state.borrow_mut().sending, submitted) {
        ui.status(&t("Wait for your message to finish sending."));
        return;
    }
    let reply_to = ui.state.borrow().reply_to.clone();
    let args = match &reply_to {
        Some((target, nth)) => reply_args(&chat, &text, &files, target, *nth),
        None => send_args(&chat, &text, &files),
    };
    ui.send(Job {
        task: Task::Fb(args, None),
        tag: Tag::ChatSent(account),
    });
}

fn handle_menu(ui: &Ui, id: i32) {
    startup_log(&format!("menu: command {id}"));
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
        ID_MARKETPLACE => load_view(ui, View::Marketplace),
        ID_EVENTS_VIEW => load_view(ui, View::Events),
        ID_PAGES_VIEW => load_view(ui, View::Pages),
        ID_PLACES_VIEW => load_view(ui, View::Places),
        ID_SAVED_VIEW => load_view(ui, View::Saved),
        ID_TRAY_TOGGLE => toggle_tray_window(&ui.frame),
        ID_TRAY_HOTKEY => prompt_tray_hotkey(ui),
        ID_GROUP_POSTS => group_posts_selected(ui),
        ID_WEB_POST => open_post_web(ui),
        ID_POST_MENU => post_menu_selected(ui),
        ID_WEB_PERSON => open_person_web(ui),
        ID_WEB_CHAT => open_chat_web(ui),
        ID_UNREAD_CHATS => load_args(ui, View::Chats, chats_args(PAGE, true)),
        ID_REFRESH => refresh(ui),
        ID_MORE => load_more(ui),
        ID_OPEN => open_selected(ui),
        ID_OPEN_LINK => open_facebook_link(ui),
        ID_COMMENT => comment_selected(ui),
        ID_SHARE => {
            if let Some(post) = post_or_warn(ui, &t("Share")) {
                match blindfb_core::posts::reader_url(&post) {
                    Some(url) => post_dialog::open(ui, &url, true),
                    None => show_warning(
                        &ui.frame,
                        t("BlindFB cannot share this kind of item yet. Open it on Facebook to share it."),
                        &t("Share post"),
                    ),
                }
            }
        }
        ID_PROFILE => profile_selected(ui),
        ID_COMMENT_FRIEND => friend_selected(ui),
        ID_AUTHOR_POSTS => posts_selected(ui),
        ID_MESSAGE => message_selected(ui),
        ID_DELETE => delete_selected(ui),
        ID_COPY_TEXT => {
            if let Some((_, record)) = selected(ui) {
                let text = display_for(&ui.state.borrow().view, &record);
                copy_text(ui, &text, &t("Text copied."));
            }
        }
        ID_COPY_LINK => match selected_link(ui) {
            Some(link) => copy_text(ui, &link, &t("Link copied.")),
            None => show_warning(&ui.frame, t("This item has no link to copy."), &t("Copy link")),
        },
        ID_SHARE_LINK => match selected_share_url(ui) {
            Some(url) => {
                open_url(&url);
                ui.status(&t("Opened Facebook's share page in your browser."));
            }
            None => show_warning(
                &ui.frame,
                t("This item has no link to share."),
                &t("Share on Facebook"),
            ),
        },
        ID_NEW_POST => new_post(ui, false),
        ID_NEW_POST_IMAGES => new_post(ui, true),
        ID_SELL => sell_dialog(ui),
        ID_GROUP_JOIN => group_join_selected(ui),
        ID_GROUP_LEAVE => group_leave_selected(ui),
        ID_LISTING_MESSAGE => listing_message_selected(ui),
        ID_LISTING_OPEN => {
            if let Some((_, record)) = selected(ui) {
                let url = record.field(1).to_owned();
                if url.starts_with("http") {
                    open_url(&url);
                    ui.status(&t("Opened the listing in your browser."));
                }
            }
        }
        ID_SEARCH => search_dialog(ui),
        ID_OCR => ocr_dialog(ui),
        ID_UPDATE => update::check(&ui.frame, ship_shape::ui::CheckTrigger::Manual),
        ID_SHORTCUTS => {
            shortcuts_dialog::show(&ui.frame);
            // Menu accelerators come from the item labels, so new labels apply the change.
            build_menus(&ui.frame);
        }
        ID_STATUS => ui.send(Job {
            task: Task::Fb(vec!["status".into()], None),
            tag: Tag::Text,
        }),
        ID_STOP_DAEMON => ui.send(Job {
            task: Task::Fb(vec!["stop".into()], None),
            tag: Tag::Text,
        }),
        ID_QUIT_DAEMON => ui.send(Job {
            task: Task::Fb(vec!["quit".into()], None),
            tag: Tag::Text,
        }),
        ID_INSTALL_BACKEND => {
            ui.status(&t("Downloading the Facebook backend…"));
            ui.send(Job {
                task: Task::InstallBackend,
                tag: Tag::Text,
            });
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
            if let View::ChatWith(chat, name) = view {
                if confirm(
                    &ui.frame,
                    t("Remove your newest message in this chat for everyone?"),
                    &t("Unsend"),
                ) {
                    ui.send(Job {
                        task: Task::Fb(unsend_args(&chat, None), None),
                        tag: Tag::ReloadChat(chat, name, ui.state.borrow().account),
                    });
                }
            } else {
                show_warning(&ui.frame, t("Open a chat first."), &t("Unsend"));
            }
        }
        ID_KEYS => {
            let dialog = MessageDialog::builder(
                &ui.frame,
                &format!(
                    "{}\n{}",
                    t("Ctrl+O: open a Facebook post, video or reel link.\n\
                 Enter on an item: open it. Applications or Shift+F10: item actions.\n\
                 End then Enter on Load more posts or Load more reels loads the next batch.\n\
                 Ctrl+1 through Ctrl+9: switch views. Ctrl+8 opens Reels, Ctrl+9 opens Marketplace. The Views list also has Events, Pages and Places. F5: refresh. Ctrl+L: load more.\n\
                 Alt+Ctrl+F (default, changeable in File, Show/hide hotkey): hide BlindFB to the system tray. Bring it back with the same shortcut, from the tray icon (double-click it, or its Show / hide menu item).\n\
                 Reels stay cached until F5. Arrows change the selected reel; Enter opens it and starts playback automatically. Escape returns to the list.\n\
                 Tab from Items to Links in selected item; Enter opens its selected link. External links come first and Full post on Facebook is last.\n\
                 The post reader starts on Post links. Applications or Shift+F10 on a post includes a Links submenu. Tab reaches post text, video controls, comments and the composer.\n\
                 Enter on a selected link or Open link opens your browser.\n\
                 Tab to Play/Pause, Stop, Back 10 seconds or Forward 10 seconds, then press Space.\n\
                 Arrow keys on Video volume change its percentage.\n\
                 Applications or Shift+F10 on post text: comment, react or share. On a comment: reply or like.\n\
                 Ctrl+Enter in the comment box sends. Escape closes the reader and stops playback."),
                    t("File, Keyboard shortcuts: change any of these shortcuts, or give any menu command one.")
                ),
                &t("Keyboard commands"),
            ).build();
            dialog.show_modal();
            dialog.destroy();
        }
        ID_ABOUT => {
            AboutBoxBuilder::new(&ui.frame)
                .name("BlindFB")
                .version(env!("CARGO_PKG_VERSION"))
                .description(t(
                    "An accessible Facebook and Messenger client for screen reader users.\n\
                     Built with wxDragon on the facebook-cli (fb) backend."
                ))
                .copyright("Copyright (c) 2026 Brandon Bracey")
                .website("https://git.serrebiradio.com/serrebi/BlindFB")
                .licence("MIT")
                .add_developer("Brandon Bracey")
                .show();
        }
        ID_GITHUB => open_url("https://git.serrebiradio.com/serrebi/BlindFB"),
        ID_VOICE_ALERTS => toggle_alert(ui, alerts::Kind::Voice),
        ID_SOUND_ALERTS => toggle_alert(ui, alerts::Kind::Sound),
        ID_LIST_VOICE_ALERTS => toggle_alert(ui, alerts::Kind::ListVoice),
        ID_LIST_SOUND_ALERTS => toggle_alert(ui, alerts::Kind::ListSound),
        ID_LANG_SYSTEM | ID_LANG_ENGLISH | ID_LANG_FRENCH | ID_LANG_RUSSIAN | ID_LANG_ESTONIAN => {
            let language = match id {
                ID_LANG_ENGLISH => Language::English,
                ID_LANG_FRENCH => Language::French,
                ID_LANG_RUSSIAN => Language::Russian,
                ID_LANG_ESTONIAN => Language::Estonian,
                _ => Language::System,
            };
            match i18n::save(language) {
                // Menus and dialogs are built once, so the new language starts with the next launch.
                Ok(()) => ui.status(&t("Language saved. Restart BlindFB to use it.")),
                Err(error) => show_error(&ui.frame, &error, &t("Language")),
            }
        }
        ID_DEBUG_LOGGING => {
            let on = !blindfb_core::debug::enabled();
            match blindfb_core::debug::set_enabled(on) {
                Ok(()) if on => ui.status(&t(
                    "Debug logging on. Repeat what failed, then use Help, Save debug report."
                )),
                Ok(()) => ui.status(&t("Debug logging off.")),
                Err(error) => show_error(&ui.frame, &error, &t("Debug logging")),
            }
        }
        ID_DEBUG_REPORT => {
            let dialog = FileDialog::builder(&ui.frame)
                .with_message(&t("Save debug report"))
                .with_default_file("BlindFB-debug-report.txt")
                // TRANSLATORS: a file-type filter; translate "Text files" only, keep "(*.txt)|*.txt".
                .with_wildcard(&t("Text files (*.txt)|*.txt"))
                .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
                .build();
            if dialog.show_modal() == ID_OK {
                if let Some(path) = dialog.get_path() {
                    ui.status(&t("Saving the debug report…"));
                    ui.send(Job {
                        task: Task::DebugReport(path),
                        tag: Tag::DebugReport,
                    });
                }
            }
        }
        ID_MSG_REPLY => reply_selected(ui),
        _ => {
            if (ID_MSG_REACT_BASE..ID_MSG_REACT_BASE + REACTIONS.len() as i32).contains(&id) {
                let reaction = REACTIONS[(id - ID_MSG_REACT_BASE) as usize];
                msg_react_selected(ui, reaction);
            } else if (ID_REACT_BASE..ID_REACT_BASE + REACTIONS.len() as i32).contains(&id) {
                if let Some(post) = post_or_warn(ui, &t("React")) {
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
        show_error(&ui.frame, t("Could not copy to the clipboard."), &t("Copy"));
    }
}

/// The selected item's web link, when it has one (short refs have none).
/// The target a list row points at, in the form its view stores it: a
/// daemon ref (`u1234`, `c1234`), a Facebook path, or a full URL. Rows
/// with no target of their own (comments) and rows still carrying the
/// `-`/empty placeholder return None.
fn item_link_target(view: &View, record: &Record) -> Option<String> {
    let target = match view {
        View::CommentsOf(_) => return None,
        View::ProfileOf(person) => person.as_str(),
        View::Notifications => record.field(3),
        View::Marketplace | View::SearchResults | View::Friends | View::Groups => record.field(1),
        _ => record.field(0),
    };
    (!target.is_empty() && target != "-").then(|| target.to_owned())
}

fn selected_target(ui: &Ui) -> Option<String> {
    let (_, record) = selected(ui)?;
    let view = ui.state.borrow().view.clone();
    item_link_target(&view, &record)
}

/// The selected item's target as a real web link, when it has one; daemon
/// refs are not links (see `fb::link_url`).
fn selected_link(ui: &Ui) -> Option<String> {
    selected_target(ui).and_then(|target| link_url(&target))
}

/// Facebook's browser share composer for the selected item's link.
fn selected_share_url(ui: &Ui) -> Option<String> {
    selected_target(ui).and_then(|target| facebook_share_url(&target))
}

/// Applications / Shift+F10 on an item. The ids are the menu bar's, so `handle_menu` runs them.
fn show_item_menu(ui: &Ui) {
    if selected(ui).is_none() {
        return;
    }
    let view = ui.state.borrow().view.clone();
    let mut menu = Menu::builder().build();
    let add = |menu: &Menu, items: &[(i32, String)]| {
        for (id, label) in items {
            if *id == 0 {
                menu.append_separator();
            } else {
                menu.append(*id, label, "", ItemKind::Normal);
            }
        }
    };
    match view {
        View::SearchResults | View::Marketplace => {
            let (_, record) = selected(ui).unwrap();
            match record.field(3) {
                "group" => add(
                    &menu,
                    &[
                        (ID_OPEN, t("&Open group")),
                        (ID_GROUP_JOIN, t("&Join group")),
                        (ID_WEB_POST, t("Open group on &Facebook (web)")),
                        (ID_COPY_TEXT, t("Cop&y text")),
                    ],
                ),
                "profile" | "page" => add(
                    &menu,
                    &[
                        (ID_PROFILE, t("&Profile")),
                        (ID_AUTHOR_POSTS, t("P&osts")),
                        (ID_MESSAGE, t("&Message")),
                        (ID_COMMENT_FRIEND, t("Add &friend")),
                        (ID_WEB_POST, t("Open on &Facebook (web)")),
                        (ID_COPY_TEXT, t("Cop&y text")),
                    ],
                ),
                "listing" => add(
                    &menu,
                    &[
                        (ID_LISTING_OPEN, t("&Open listing in browser")),
                        (ID_LISTING_MESSAGE, t("&Message seller…")),
                        (ID_COPY_LINK, t("Copy &link")),
                        (ID_SHARE_LINK, t("Share on &Facebook…")),
                        (ID_COPY_TEXT, t("Cop&y text")),
                    ],
                ),
                _ => add(
                    &menu,
                    &[
                        (ID_OPEN, t("&Open")),
                        (ID_SHARE, t("S&hare…")),
                        (ID_COMMENT, t("&Comment…")),
                        (ID_COPY_TEXT, t("Cop&y text")),
                        (ID_COPY_LINK, t("Copy &link")),
                        (ID_WEB_POST, t("Open on &Facebook (web)")),
                    ],
                ),
            }
        }
        View::Feed
        | View::RecentFeed
        | View::Reels(_)
        | View::MyPosts
        | View::PostsOf(_)
        | View::GroupPosts(_) => {
            add(
                &menu,
                &[
                    (ID_OPEN, t("&Open post and comments")),
                    (ID_SHARE, t("S&hare…")),
                    (ID_COMMENT, t("&Comment…")),
                    (ID_POST_MENU, t("Facebook post &menu…")),
                ],
            );
            // A group post's author line names the group but resolves no
            // profile; open the group itself instead (author posts only
            // works for real authors).
            let (_, record) = selected(ui).unwrap();
            if let Some(target) = post_target(&view, &record) {
                if blindfb_core::posts::group_url(target).is_some() {
                    add(&menu, &[(ID_GROUP_POSTS, t("Group's p&osts"))]);
                }
            }
            let react = Menu::builder().build();
            for (i, reaction) in REACTIONS.iter().enumerate() {
                let label = reaction_label(reaction);
                react.append(ID_REACT_BASE + i as i32, &label, "", ItemKind::Normal);
            }
            menu.append_submenu(react, &t("&React"), "");
            append_links_menu(&menu, &ui.link_targets.borrow());
            add(
                &menu,
                &[
                    (0, String::new()),
                    (ID_PROFILE, t("Author &profile")),
                    (ID_AUTHOR_POSTS, t("Author's p&osts")),
                    (ID_MESSAGE, t("&Message author")),
                    (0, String::new()),
                    (ID_COPY_TEXT, t("Cop&y text")),
                    (ID_COPY_LINK, t("Copy &link")),
                    (ID_WEB_POST, t("Open post on &Facebook (web)")),
                    (ID_WEB_PERSON, t("Open author on Fa&cebook (web)")),
                ],
            );
            if view == View::MyPosts {
                add(&menu, &[(0, String::new()), (ID_DELETE, t("&Delete post…"))]);
            }
        }
        View::Notifications => add(
            &menu,
            &[
                (ID_OPEN, t("&Open")),
                (ID_SHARE, t("S&hare…")),
                (ID_COPY_TEXT, t("Cop&y text")),
                (ID_COPY_LINK, t("Copy &link")),
                (ID_WEB_POST, t("Open on &Facebook (web)")),
            ],
        ),
        View::Friends => add(
            &menu,
            &[
                (ID_PROFILE, t("&Profile")),
                (ID_AUTHOR_POSTS, t("P&osts")),
                (ID_MESSAGE, t("&Message")),
                (ID_WEB_PERSON, t("Open on &Facebook (web)")),
            ],
        ),
        View::Chats => add(
            &menu,
            &[
                (ID_OPEN, t("&Open chat")),
                (ID_WEB_CHAT, t("Open in &Messenger (web)")),
            ],
        ),
        View::ChatWith(..) => {
            let replying = ui.state.borrow().reply_to.is_some();
            add(
                &menu,
                &[(
                    ID_MSG_REPLY,
                    if replying { t("Cancel &reply") } else { t("&Reply") },
                )],
            );
            let react = Menu::builder().build();
            for (i, reaction) in REACTIONS.iter().enumerate() {
                if *reaction == "none" {
                    continue;
                }
                let label = reaction_label(reaction);
                react.append(ID_MSG_REACT_BASE + i as i32, &label, "", ItemKind::Normal);
            }
            menu.append_submenu(react, &t("Re&act"), &t("React to this message"));
            add(
                &menu,
                &[
                    (ID_COPY_TEXT, t("Cop&y message")),
                    (ID_UNSEND, t("&Unsend my last message…")),
                    (ID_WEB_CHAT, t("Open in &Messenger (web)")),
                ],
            );
        }
        View::CommentsOf(_) => add(
            &menu,
            &[
                (ID_PROFILE, t("Commenter &profile")),
                (ID_COMMENT_FRIEND, t("Add &friend")),
                (ID_MESSAGE, t("&Message commenter")),
                (ID_COPY_TEXT, t("Cop&y text")),
            ],
        ),
        View::Groups => add(
            &menu,
            &[
                (ID_OPEN, t("&Open group")),
                (ID_COPY_LINK, t("Copy &link")),
                (ID_WEB_PERSON, t("Open group on &Facebook (web)")),
                (0, String::new()),
                (ID_GROUP_LEAVE, t("&Leave group…")),
            ],
        ),
        View::ProfileOf(_) => add(
            &menu,
            &[
                (ID_AUTHOR_POSTS, t("P&osts")),
                (ID_MESSAGE, t("&Message")),
                (ID_COMMENT_FRIEND, t("Add &friend")),
                (ID_WEB_PERSON, t("Open on &Facebook (web)")),
                (ID_COPY_TEXT, t("Cop&y text")),
            ],
        ),
        _ => add(&menu, &[(ID_COPY_TEXT, t("Cop&y text"))]),
    }
    ui.items.popup_menu(&mut menu, None);
}

/// Pick photo/video files and split them by extension: the backend's post
/// and sell commands take --img and --video separately.
fn pick_media(ui: &Ui) -> Option<(Vec<String>, Vec<String>)> {
    let dialog = FileDialog::builder(&ui.frame)
        .with_style(FileDialogStyle::Multiple)
        .build();
    if dialog.show_modal() != ID_OK {
        return None;
    }
    let paths = dialog.get_paths();
    if paths.is_empty() {
        return None;
    }
    let mut images = Vec::new();
    let mut videos = Vec::new();
    for path in paths {
        if is_video_path(&path) {
            videos.push(path);
        } else {
            images.push(path);
        }
    }
    Some((images, videos))
}

fn is_video_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".mp4", ".mov", ".webm", ".mkv", ".avi", ".m4v"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn new_post(ui: &Ui, with_media: bool) {
    let Some(text) = prompt_text(&ui.frame, t("Post text:"), &t("New post")) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let (mut images, mut videos) = (Vec::new(), Vec::new());
    if with_media {
        let Some((picked_images, picked_videos)) = pick_media(ui) else {
            return;
        };
        images = picked_images;
        videos = picked_videos;
    }
    let labels = [t("Public"), t("Friends"), t("Only me")];
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog =
        SingleChoiceDialog::builder(&ui.frame, &t("Who can see this?"), &t("Audience"), &labels)
            .build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    let audience = match dialog.get_selection() {
        0 => Some("public"),
        1 => Some("friends"),
        2 => Some("me"),
        _ => return,
    };
    ui.send(Job {
        task: Task::Fb(post_args(&text, audience, &images, &videos), None),
        tag: Tag::Post,
    });
}

/// Enter on a search result: what opens depends on the row's kind, which
/// the backend stamps on every row (post, profile, page, group, photo,
/// video, event, place, listing).
fn open_search_result(ui: &Ui, record: &Record) {
    let kind = record.field(3);
    let target = record.field(1).to_owned();
    match kind {
        "profile" | "page" => profile_selected(ui),
        "group" => group_dialog::open(ui, record),
        "listing" => {
            if target.starts_with("http") {
                open_url(&target);
                ui.status(&t("Opened the listing in your browser."));
            } else {
                ui.status(&t("This listing row has no link to open."));
            }
        }
        "post" => {
            if target.is_empty() {
                show_warning(
                    &ui.frame,
                    t("This row has no link, so there is no post to open."),
                    &t("Open post"),
                );
            } else {
                load_view(ui, View::CommentsOf(target));
            }
        }
        _ => {
            if target.is_empty() {
                show_warning(
                    &ui.frame,
                    t("This row has no link, so it cannot be opened."),
                    &t("Open"),
                );
                return;
            }
            match blindfb_core::posts::reader_url(&target) {
                Some(url) => post_dialog::open(ui, &url, false),
                None => {
                    if confirm(
                        &ui.frame,
                        t("BlindFB cannot read this kind of item yet. Open it in your web browser?"),
                        &t("Open"),
                    ) {
                        open_url(&web_url(&target));
                    }
                }
            }
        }
    }
}

fn group_join_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let group = record.field(1).to_owned();
    if group.is_empty() {
        ui.status(&t("This group row has no link, so it cannot be joined."));
        return;
    }
    ui.status(&tf!("Joining {group}…", group = record.field(0)));
    ui.send(Job {
        task: Task::Fb(group_join_args(&group), None),
        tag: Tag::GroupJoin { group },
    });
}

/// The backend's questions output: a "questions<TAB>n" line, then one
/// "q<TAB>question" line per question. Ask each in turn and submit the
/// answers as one group-apply.
fn answer_group_questions(ui: &Ui, group: &str, output: &str) {
    let questions: Vec<&str> = output
        .lines()
        .filter_map(|line| line.strip_prefix("q\t"))
        .collect();
    if questions.is_empty() {
        set_details(ui, output, None);
        ui.status(&t("Done"));
        return;
    }
    let mut answers = Vec::new();
    for question in &questions {
        let Some(answer) =
            // TRANSLATORS: {question} is a group's membership question, asked in a text box.
            prompt_text(&ui.frame, tf!("{question}:", question = question), &t("Membership questions"))
        else {
            ui.status(&t("Join cancelled: the membership questions were not answered."));
            return;
        };
        answers.push(answer);
    }
    ui.status(&t("Submitting your answers…"));
    ui.send(Job {
        task: Task::Fb(group_apply_args(group, &answers), None),
        tag: Tag::Text,
    });
}

fn group_leave_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let group = record.field(1).to_owned();
    if group.is_empty() {
        ui.status(&t("This group row has no link, so it cannot be left."));
        return;
    }
    let name = record.field(0).to_owned();
    if !confirm(
        &ui.frame,
        tf!(
            "Leave the group {name}? You can rejoin later, but private groups will need approval again.",
            name = name
        ),
        &t("Leave group"),
    ) {
        return;
    }
    ui.status(&tf!("Leaving {name}…", name = name));
    ui.send(Job {
        task: Task::Fb(group_leave_args(&group), None),
        tag: Tag::GroupLeave,
    });
}

fn listing_message_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else {
        return;
    };
    let listing = record.field(1).to_owned();
    if listing.is_empty() {
        ui.status(&t("This listing row has no link, so its seller cannot be messaged."));
        return;
    }
    let Some(text) = prompt_text(
        &ui.frame,
        tf!("Message to the seller of {listing}:", listing = record.field(0)),
        &t("Message seller"),
    ) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    ui.status(&t("Messaging the seller…"));
    ui.send(Job {
        task: Task::Fb(listing_message_args(&listing, &text), None),
        tag: Tag::Text,
    });
}

/// The Marketplace sell form: title, price, condition, category,
/// description and the photos/videos to attach (at least one file).
fn sell_dialog(ui: &Ui) {
    let Some(title) = prompt_text(&ui.frame, t("Listing title:"), &t("Sell on Marketplace")) else {
        return;
    };
    if title.trim().is_empty() {
        return;
    }
    let Some(price) = prompt_text(
        &ui.frame,
        t("Price (0 lists it as free):"),
        &t("Sell on Marketplace"),
    ) else {
        return;
    };
    // A French price reads "12,50": the decimal comma becomes the point the backend expects.
    let price = price.trim().replace(',', ".");
    if price.is_empty() || !price.chars().all(|c| c.is_ascii_digit() || c == '.') {
        show_warning(
            &ui.frame,
            t("Enter the price as a number, for example 40 or 12.50. Use 0 for a free listing."),
            &t("Sell on Marketplace"),
        );
        return;
    }
    let labels = [
        t("New"),
        t("Used - like new"),
        t("Used - good"),
        t("Used - fair"),
    ];
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dialog = SingleChoiceDialog::builder(
        &ui.frame,
        &t("Condition:"),
        &t("Sell on Marketplace"),
        &labels,
    )
    .build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    let condition = match dialog.get_selection() {
        0 => Some("new"),
        1 => Some("like-new"),
        2 => Some("good"),
        3 => Some("fair"),
        _ => return,
    };
    let category = prompt_text(
        &ui.frame,
        t("Category (leave empty to skip):"),
        &t("Sell on Marketplace"),
    );
    let category = category.filter(|c| !c.trim().is_empty());
    let description = prompt_text(
        &ui.frame,
        t("Description (leave empty to skip):"),
        &t("Sell on Marketplace"),
    );
    let description = description.filter(|d| !d.trim().is_empty());
    let Some((images, videos)) = pick_media(ui) else {
        return;
    };
    if images.len() + videos.len() > 10 {
        show_warning(
            &ui.frame,
            t("Marketplace allows at most 10 photos and videos per listing."),
            &t("Sell on Marketplace"),
        );
        return;
    }
    ui.status(&tf!("Listing {title} on Marketplace…", title = title));
    ui.send(Job {
        task: Task::Fb(
            sell_args(
                title.trim(),
                &price,
                condition,
                category.as_deref(),
                description.as_deref(),
                &images,
                &videos,
            ),
            None,
        ),
        tag: Tag::Text,
    });
}

/// The search filters' display labels, in combo-box order; `search_kind`
/// maps each index to its backend type. Index 0 is the unfiltered "top"
/// search (no --type).
fn search_filters() -> [String; 11] {
    [
        t("Everything"),
        t("Posts"),
        t("People"),
        t("Friends"),
        t("Photos"),
        t("Videos"),
        t("Pages"),
        t("Places"),
        t("Groups"),
        t("Events"),
        t("Marketplace"),
    ]
}

fn search_kind(index: u32) -> Option<&'static str> {
    match index {
        0 => None,
        1 => Some("posts"),
        2 => Some("people"),
        3 => Some("friends"),
        4 => Some("photos"),
        5 => Some("videos"),
        6 => Some("pages"),
        7 => Some("places"),
        8 => Some("groups"),
        9 => Some("events"),
        10 => Some("marketplace"),
        _ => None,
    }
}

/// One dialog for search (Brandon, 2026-10-31: "Search can be only one
/// dialogue with filter combo box") — the query field and the filter
/// combo together, replacing the old prompt-then-pick-list two-step.
fn search_dialog(ui: &Ui) {
    let dialog = Dialog::builder(&ui.frame, &t("Search Facebook"))
        .with_size(460, 210)
        .with_style(DialogStyle::DefaultDialogStyle)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let query_label = StaticText::builder(&dialog)
        .with_label(&t("Search &for:"))
        .build();
    let query = TextCtrl::builder(&dialog)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
    query.set_name(&t("Search for"));
    let filter_label = StaticText::builder(&dialog)
        .with_label(&t("Search &in:"))
        .build();
    let filters = search_filters();
    let filters: Vec<&str> = filters.iter().map(String::as_str).collect();
    let filter = ComboBox::builder(&dialog)
        .with_style(ComboBoxStyle::ReadOnly)
        .with_string_choices(&filters)
        .build();
    filter.set_name(&t("Search in"));
    filter.set_selection(0);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let search = Button::builder(&dialog)
        .with_id(ID_OK)
        .with_label(&t("&Search"))
        .build();
    search.set_name(&t("Search"));
    search.set_default();
    let cancel = Button::builder(&dialog)
        .with_id(ID_CANCEL)
        .with_label(&t("Cancel"))
        .build();
    cancel.set_name(&t("Cancel"));
    buttons.add(&search, 0, SizerFlag::All, 4);
    buttons.add(&cancel, 0, SizerFlag::All, 4);
    root.add(&query_label, 0, SizerFlag::All, 4);
    root.add(&query, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add(&filter_label, 0, SizerFlag::All, 4);
    root.add(&filter, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 4);
    dialog.set_sizer(root, true);
    query.set_focus();
    if dialog.show_modal() != ID_OK {
        dialog.destroy();
        return;
    }
    let text = query.get_value();
    let kind = filter.get_selection().and_then(search_kind);
    dialog.destroy();
    if text.trim().is_empty() {
        return;
    }
    load_args(ui, View::SearchResults, search_args(&text, kind, PAGE));
}

fn ocr_dialog(ui: &Ui) {
    if !native_ocr::is_available() {
        show_warning(
            &ui.frame,
            t("No OCR engine is available on this system. On Linux, install tesseract-ocr."),
            &t("OCR an image"),
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
    ui.status(&t("Reading image text…"));
    thread::spawn(move || {
        let result = ocr_file(&path);
        let _ = done_tx.send(Done {
            tag: Tag::Text,
            result,
        });
    });
}

fn ocr_file(path: &str) -> Result<String, String> {
    let img = image::open(PathBuf::from(path))
        .map_err(|e| tf!("could not open image: {error}", error = e))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    native_ocr::recognize_rgba(rgba.as_raw(), width, height)
        .map_err(|e| tf!("OCR failed: {error}", error = e))
}

/// Open a link in the default browser. Facebook paths are made absolute
/// first. The platform browser launcher does the work — the old Windows
/// path handed the URL to explorer.exe, which treats it as a folder path
/// and opened the Documents folder instead of the browser.
fn open_url(url: &str) {
    let url = web_url(url);
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return;
    }
    if launch_default_browser(&url, BrowserLaunchFlags::Default) {
        return;
    }
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(&url)
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
}

#[cfg(test)]
mod tests {
    #[test]
    fn modal_priority_is_reader_then_group_then_chat() {
        // The reader opens over the group and chat windows, so it owns
        // input while it lasts; popups must parent to the topmost modal.
        assert_eq!(modal_kind(true, true, true), Some(ModalKind::Reader));
        assert_eq!(modal_kind(true, false, false), Some(ModalKind::Reader));
        assert_eq!(modal_kind(false, true, true), Some(ModalKind::Group));
        assert_eq!(modal_kind(false, false, true), Some(ModalKind::Chat));
        assert_eq!(modal_kind(false, false, false), None);
    }

    #[test]
    fn the_french_catalog_is_embedded() {
        assert!(patois::available_locales(i18n::DOMAIN).contains(&"fr"));
    }

    #[test]
    fn the_russian_and_estonian_catalogs_are_embedded() {
        let locales = patois::available_locales(i18n::DOMAIN);
        assert!(locales.contains(&"ru"));
        assert!(locales.contains(&"et"));
    }

    /// Every message in the French catalog has its translation; `cargo xtask i18n`
    /// adds new English messages with an empty one, which this test then reports.
    #[test]
    fn every_french_message_is_translated() {
        let po = include_str!("../locale/fr/LC_MESSAGES/blindfb.po");
        let mut missing = Vec::new();
        let mut id = String::new();
        for block in po.split("\n\n") {
            let lines: Vec<&str> = block.lines().filter(|l| !l.starts_with('#')).collect();
            if let Some(first) = lines.iter().find(|l| l.starts_with("msgid ")) {
                id = first.trim_start_matches("msgid ").to_owned();
            }
            let empty = lines.iter().any(|l| {
                (l.starts_with("msgstr \"\"") || l.starts_with("msgstr[")
                    && l.ends_with("\"\""))
                    && !lines.iter().skip_while(|x| !x.starts_with("msgstr")).skip(1).any(|x| x.starts_with('"'))
            });
            if empty && id != "\"\"" {
                missing.push(id.clone());
            }
        }
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }
    use super::*;

    #[test]
    fn match_nth_counts_newer_duplicates() {
        let texts: Vec<String> = ["ok", "later", "ok", "okay"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // "okay" contains "ok", so the backend counts it as a match too.
        assert_eq!(match_nth(&texts, 0), Some(("ok".to_string(), 2)));
        assert_eq!(match_nth(&texts, 2), Some(("ok".to_string(), 1)));
        assert_eq!(match_nth(&texts, 3), Some(("okay".to_string(), 0)));
        assert_eq!(match_nth(&texts, 9), None);
        let blank: Vec<String> = vec![String::new()];
        assert_eq!(match_nth(&blank, 0), None);
    }

    #[test]
    fn message_match_text_strips_time_sender_and_reactions() {
        assert_eq!(
            message_match_text("10:00 Alice: see you at 5"),
            "see you at 5"
        );
        assert_eq!(message_match_text("10:00 You: ok {👍❤️}"), "ok");
        assert_eq!(
            message_match_text("9:41 Bob: note: with colon"),
            "note: with colon"
        );
        assert_eq!(message_match_text("10:00 You: {only}"), "{only}");
    }

    #[test]
    fn back_pops_to_the_previous_different_view() {
        let mut history = vec![View::Feed, View::Chats, View::Chats];
        assert_eq!(pop_previous(&mut history, &View::Chats), Some(View::Feed));
        assert!(history.is_empty());
        assert_eq!(pop_previous(&mut history, &View::Feed), None);
    }

    #[test]
    fn selection_key_names_the_selected_record() {
        // Issue #23: the key captured when a list is left is what the
        // return restores; no selection, a stale index, and a "-" row
        // (no identity) all remember nothing.
        let rows = vec![
            Record {
                author_ref: None,
                links: Vec::new(),
                fields: vec!["p12345".into(), "Alice".into()],
                raw: String::new(),
            },
            Record {
                author_ref: None,
                links: Vec::new(),
                fields: vec!["p67890".into(), "Bob".into()],
                raw: String::new(),
            },
        ];
        assert_eq!(
            selection_key(&View::Feed, &rows, Some(1)).as_deref(),
            Some("p67890")
        );
        assert_eq!(selection_key(&View::Feed, &rows, None), None);
        assert_eq!(selection_key(&View::Feed, &rows, Some(9)), None);
        let dash = vec![Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec!["-".into()],
            raw: "-".into(),
        }];
        assert_eq!(selection_key(&View::Feed, &dash, Some(0)), None);
    }

    #[test]
    fn poster_name_is_the_first_of_a_joined_author_line() {
        // Issue #14: searching the whole joined line matched nobody.
        assert_eq!(poster_name("Alice Smith / Bob Jones"), "Alice Smith");
        assert_eq!(poster_name("Alice Smith"), "Alice Smith");
    }

    #[test]
    fn feed_details_name_each_count() {
        // Issue #16: the details pane showed a bare "0/0/0" after
        // "Reactions / comments / shares:"; each count is now named.
        assert_eq!(
            counts_label("0/0/0"),
            "Reactions: 0, comments: 0, shares: 0"
        );
        assert_eq!(
            counts_label("12/3/1.2K"),
            "Reactions: 12, comments: 3, shares: 1.2K"
        );
        // French Facebook writes "1,9 K" and "4 890" with a space.
        assert_eq!(
            counts_label("1,9 K/49/4\u{202f}890"),
            "Reactions: 1,9 K, comments: 49, shares: 4\u{202f}890"
        );
        // Not a numeric triple: shown as it came, never dropped.
        assert_eq!(
            counts_label("many"),
            "Reactions / comments / shares: many"
        );
        let post = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec![
                "p1".into(),
                "Alice".into(),
                "2h".into(),
                "4/5/6".into(),
                "Hello".into(),
            ],
            raw: String::new(),
        };
        let details = details_for(&View::Feed, &post);
        assert!(details.contains("Reactions: 4, comments: 5, shares: 6"));
    }

    #[test]
    fn chat_details_survive_a_short_row() {
        // A chats line with fewer than five fields must not panic the reader.
        let short = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec!["c1".into()],
            raw: "c1".into(),
        };
        assert_eq!(details_for(&View::Chats, &short), "c1");
        let full = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec![
                "c1".into(),
                "!".into(),
                "now".into(),
                "Alice".into(),
                "hi".into(),
            ],
            raw: String::new(),
        };
        let details = details_for(&View::Chats, &full);
        assert!(details.contains("Alice"));
        assert!(details.contains("hi"));
    }

    #[test]
    fn messaging_addresses_a_person_by_ref_not_by_name() {
        // friends: name, ref, mutual friends. The ref, not the name, is the chat spec.
        let friend = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec![
                "Alice Smith".into(),
                "u1abc".into(),
                "3 mutual friends".into(),
            ],
            raw: String::new(),
        };
        assert_eq!(
            message_target(&View::Friends, &friend),
            ("u1abc".to_owned(), "Alice Smith".to_owned())
        );
        // chats: ref, flags, time, name, preview.
        let chat = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec![
                "c8x2f".into(),
                "!".into(),
                "now".into(),
                "Bob".into(),
                "hi".into(),
            ],
            raw: String::new(),
        };
        assert_eq!(
            message_target(&View::Chats, &chat),
            ("c8x2f".to_owned(), "Bob".to_owned())
        );
    }

    #[test]
    fn reels_with_matching_captions_keep_their_distinct_urls() {
        let view = View::Reels(ReelSource::Recommended);
        assert!(cacheable(&view, &[]));
        let (rows, _) = reels::parse(r#"{"rows":[{"url":"https://www.facebook.com/reel/1/","author":"A","text":"Same"},{"url":"https://www.facebook.com/reel/2/","author":"A","text":"Same"}]}"#).unwrap();
        assert_eq!(
            post_target(&view, &rows[0]),
            Some("https://www.facebook.com/reel/1/")
        );
        assert_eq!(merge_records(&view, rows.clone(), rows).len(), 2);
    }

    #[test]
    fn text_links_trim_punctuation_keep_balanced_urls_and_reject_other_schemes() {
        let links = links_in_text(
            "Article (https://example.org/story). https://example.org/a_(b), http://example.net/x?y=1&z=2. \"https://example.org/story\" https:// file:///secret javascript:alert(1) https://bad\\host/x https://example.net/a⏎https://example.net/b [https://[::1]]",
            None,
        );
        let urls: Vec<_> = links.iter().map(|link| link.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://example.org/story",
                "https://example.org/a_(b)",
                "http://example.net/x?y=1&z=2",
                "https://example.net/a",
                "https://example.net/b",
                "https://[::1]"
            ]
        );
        let post = "https://www.facebook.com/a/posts/1";
        let links = links_in_text(
            &format!("{post}/ {post} https://example.org/story"),
            Some(post),
        );
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
    fn a_section_is_read_once_the_views_selection_rests() {
        assert_eq!(rested_view(None, VIEW_REST), None);
        // the selection just moved: wait
        assert_eq!(
            rested_view(Some((View::Chats, Instant::now())), VIEW_REST),
            None
        );
        let rested = Instant::now() - (VIEW_REST + Duration::from_millis(50));
        assert_eq!(
            rested_view(Some((View::Chats, rested)), VIEW_REST),
            Some(View::Chats)
        );
    }

    #[test]
    fn background_reads_stand_aside_for_anything_the_user_waits_for() {
        // A background read takes a free slot only while nothing is queued.
        assert!(gate_admits(0, 2, 0, true));
        assert!(!gate_admits(1, 2, 0, true));
        assert!(gate_admits(1, 2, 0, false));
        assert!(!gate_admits(0, 2, 1, true));
        assert!(gate_admits(0, 2, 1, false));
        // But it still respects the ceiling.
        assert!(!gate_admits(2, 2, 0, true));
        assert!(!gate_admits(2, 2, 0, false));
    }

    #[test]
    fn queued_prefetch_can_use_the_reserved_slot_when_opened() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let gate = Arc::new(ReadGate::new(2));
        let background = Arc::new(AtomicBool::new(true));
        let active_prefetch = gate.acquire(|| true);
        let (started, waiting) = std::sync::mpsc::channel();
        let (admitted, ready) = std::sync::mpsc::channel();
        let queued_gate = gate.clone();
        let queued_background = background.clone();
        let queued = thread::spawn(move || {
            let _permit = queued_gate.acquire(|| {
                let _ = started.send(());
                queued_background.load(Ordering::SeqCst)
            });
            admitted.send(()).unwrap();
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(ready.try_recv().is_err());
        background.store(false, Ordering::SeqCst);
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(active_prefetch);
        queued.join().unwrap();
    }

    #[test]
    fn tray_hotkey_default_is_alt_ctrl_f_without_repeat() {
        // One shortcut toggles hide/restore. MOD_ALT | MOD_CONTROL |
        // MOD_NOREPEAT, VK_F — the default the Windows hotkey thread
        // registers system-wide so it also fires while hidden. The
        // saved setting decides the real spec (off returns None).
        assert_eq!(
            blindfb_core::tray_hotkey::Hotkey::AltCtrlF.win_spec(),
            Some((0x4003, 0x46))
        );
        assert_eq!(blindfb_core::tray_hotkey::Hotkey::Off.win_spec(), None);
        assert!(tray_hotkey::Hotkey::choices().len() == 7);
    }

    #[test]
    fn tray_menu_outlives_the_frame_that_sets_it() {
        // Issue #26 regression: wxDragon's Menu destroys its wxMenu on
        // drop and set_popup_menu keeps only a reference, so the tray
        // menu must be leaked like the icon — a stack-owned menu was
        // freed when run() returned and opening the tray menu crashed
        // the app. The destroy path must also remove the icon so a
        // clean exit leaves no ghost in the tray.
        // Normalize line endings: a Windows checkout is CRLF.
        let source = include_str!("main.rs").replace("\r\n", "\n");
        assert!(source.contains("Box::leak(Box::new(\n        Menu::builder()"));
        assert!(source.contains("tray_destroy.remove_icon()"));
    }

    #[test]
    fn opening_a_prefetched_section_makes_its_read_foreground() {
        let mut requests = ListRequests::default();
        let warm = requests.begin_background(&View::Chats);
        assert!(requests.background.contains(&View::Chats));
        // The user opens the section while the warm read is still running.
        requests.background.remove(&View::Chats);
        assert!(!requests.background.contains(&View::Chats));
        assert!(requests.current(&View::Chats, warm));
        assert!(requests.finish(&View::Chats, warm));
        assert!(requests.pending.is_empty());
    }

    #[test]
    fn item_links_use_each_views_target_instead_of_its_label() {
        let row = |fields: &[&str]| Record {
            author_ref: None,
            links: Vec::new(),
            fields: fields.iter().map(|s| (*s).to_owned()).collect(),
            raw: String::new(),
        };
        assert_eq!(
            item_link_target(
                &View::Marketplace,
                &row(&[
                    "A chair",
                    "https://www.facebook.com/marketplace/item/123/",
                    "$5",
                    "listing"
                ])
            )
            .as_deref(),
            Some("https://www.facebook.com/marketplace/item/123/")
        );
        assert_eq!(
            item_link_target(
                &View::SearchResults,
                &row(&["A person", "u1234", "", "profile"])
            )
            .as_deref(),
            Some("u1234")
        );
        assert_eq!(
            item_link_target(
                &View::Friends,
                &row(&["Alice", "u1234", "2 mutual friends"])
            )
            .as_deref(),
            Some("u1234")
        );
        assert_eq!(
            item_link_target(
                &View::Notifications,
                &row(&["now", "!", "A post", "/a/posts/1"])
            )
            .as_deref(),
            Some("/a/posts/1")
        );
        assert_eq!(
            item_link_target(&View::Chats, &row(&["c1234", "!", "now", "Alice", "Hi"])).as_deref(),
            Some("c1234")
        );
        assert_eq!(
            item_link_target(&View::ProfileOf("u1234".into()), &row(&["name", "Alice"])).as_deref(),
            Some("u1234")
        );
        assert!(item_link_target(&View::Feed, &row(&["-", "Sponsor", "", "", "Ad"])).is_none());
        assert!(
            item_link_target(&View::SearchResults, &row(&["No link", "", "", "listing"])).is_none()
        );
        assert!(item_link_target(
            &View::CommentsOf("p1234".into()),
            &row(&["c", "u1234", "Alice", "now", "Hi"])
        )
        .is_none());
    }

    #[test]
    fn comment_rows_carry_their_cl_links() {
        let raw = "by\tAlice\tnow\ncl\thttps://stray.example/\tStray\ncu\t/commenter\nc\tCommenter One\t2h\tGreat post, see example.com/story\ncl\thttps://example.com/story?a=1\texample.com/story\ncl\thttps://othersite.org/page\tOther Site\nc\tSecond\t3h\tNo links here\n";
        let (records, _) = parse_for(&View::CommentsOf("p1".into()), raw).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].links,
            [
                Link {
                    label: "example.com/story".to_owned(),
                    url: "https://example.com/story?a=1".to_owned(),
                    profile: None,
                },
                Link {
                    label: "Other Site".to_owned(),
                    url: "https://othersite.org/page".to_owned(),
                    profile: None,
                },
            ]
        );
        assert!(records[1].links.is_empty());
    }

    #[test]
    fn a_page_is_small_enough_that_opening_a_section_is_not_a_long_scroll() {
        const { assert!(PAGE == 10) };
        const { assert!(FEED_FIRST == PAGE && FEED_AHEAD == PAGE && FEED_NEAR_END < PAGE as usize) };
        const { assert!(MY_POSTS_FIRST < PAGE) };
        let args = default_args(&View::Feed).unwrap();
        assert!(args.windows(2).any(|w| w == ["-n", "10"]));
        for view in [
            View::Chats,
            View::Friends,
            View::Groups,
            View::RecentFeed,
            View::Saved,
            View::PostsOf("someone".into()),
            View::Marketplace,
        ] {
            let args = default_args(&view).unwrap();
            assert!(args.windows(2).any(|w| w == ["-n", "10"]), "{view:?}");
            let more = more_args(&view, &args, 10, false).unwrap();
            assert!(more.windows(2).any(|w| w == ["-n", "20"]));
        }
        assert_eq!(default_args(&View::Notifications), Some(notifs_args(10)));
        let args = default_args(&View::MyPosts).unwrap();
        assert!(args.windows(2).any(|w| w == ["-n", "5"]));
    }

    #[test]
    fn recent_feed_reads_ahead_like_the_news_feed() {
        // Most Recent re-scrolls from the top per request, so like the News
        // feed it reads ahead in the background and Load more grows from
        // the read-ahead page instead of waiting behind the user's tap.
        const { assert!(RECENT_AHEAD > PAGE) };
        let ahead = recent_feed_args(RECENT_AHEAD);
        assert!(ahead.windows(2).any(|w| w == ["-n", "30"]));
        assert_eq!(
            more_args(&View::RecentFeed, &ahead, RECENT_AHEAD as usize, false),
            Some(recent_feed_args(RECENT_AHEAD + PAGE))
        );
    }

    #[test]
    fn refresh_args_re_ask_for_the_same_page_as_the_first_load() {
        for view in [View::Feed, View::Chats, View::Notifications] {
            let args = default_args(&view).unwrap();
            assert_eq!(refresh_args(&view, &args), Some(args));
        }
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
        let message = ChatSubmission {
            account: 0,
            chat: "Alice".into(),
            name: "Alice".into(),
            text: "Hi".into(),
            files: vec![],
        };
        let mut pending = None;
        assert!(ChatSubmission::begin(&mut pending, message.clone()));
        assert!(!ChatSubmission::begin(&mut pending, message.clone()));
        pending.take();
        assert!(ChatSubmission::begin(&mut pending, message));
    }

    #[test]
    fn old_account_completion_cannot_take_new_accounts_pending_message() {
        let mut pending = Some(ChatSubmission {
            account: 1,
            chat: "Bob".into(),
            name: "Bob".into(),
            text: "New account draft".into(),
            files: vec![],
        });
        assert!(ChatSubmission::finish(&mut pending, 0).is_none());
        assert_eq!(pending.as_ref().unwrap().text, "New account draft");
        assert!(ChatSubmission::finish(&mut pending, 1).is_some());
        assert!(pending.is_none());
    }

    #[test]
    fn sent_message_preserves_new_text_files_and_other_chat_drafts() {
        let message = ChatSubmission {
            account: 0,
            chat: "c2".into(),
            name: "Alice".into(),
            text: "First".into(),
            files: vec!["first.png".into()],
        };
        let mut files = vec!["first.png".into(), "next.png".into()];
        assert!(!message.clear_sent(
            &View::ChatWith("c1".into(), "Bob".into()),
            "First",
            &mut files
        ));
        assert_eq!(files, ["next.png"]);
        assert!(!message.clear_sent(
            &View::ChatWith("c2".into(), "Alice".into()),
            "Next draft",
            &mut files
        ));
        assert_eq!(files, ["next.png"]);
        assert!(message.clear_sent(
            &View::ChatWith("c2".into(), "Alice".into()),
            "First",
            &mut files
        ));
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
        assert_eq!(
            more_args(
                &View::Feed,
                &feed_args(PAGE, true, true, true),
                PAGE as usize,
                false
            ),
            Some(feed_args(PAGE, false, true, true))
        );
        assert_eq!(
            more_args(&View::Chats, &chats_args(PAGE, true), 38, false),
            Some(chats_args(PAGE * 2, true))
        );
        assert_eq!(
            more_args(
                &View::SearchResults,
                &search_args("rust", Some("posts"), PAGE),
                40,
                false
            ),
            Some(search_args("rust", Some("posts"), PAGE * 2))
        );
        assert_eq!(
            more_args(&View::Notifications, &notifs_args(PAGE), 40, false),
            Some(notifs_args(40 + PAGE))
        );
        assert_eq!(
            more_args(&View::Groups, &groups_args(PAGE), 40, false),
            Some(groups_args(PAGE * 2))
        );
        assert_eq!(more_args(&View::Feed, &[], 0, false), None);
        assert_eq!(
            more_args(
                &View::SearchResults,
                &search_args("new query", Some("posts"), PAGE),
                40,
                true
            ),
            None
        );
    }

    #[test]
    fn load_more_is_a_final_action_row_only_in_paged_sections() {
        assert_eq!(more_row_label(&View::Feed, 100).as_deref(), Some("Load more posts"));
        assert_eq!(
            more_row_label(&View::Reels(ReelSource::Recommended), 20).as_deref(),
            Some("Load more reels")
        );
        assert!(more_row_label(&View::Feed, 0).is_none());
        assert_eq!(more_row_label(&View::Groups, 10).as_deref(), Some("Load more groups"));
        assert!(is_more_row(&View::Feed, 100, 100));
        assert!(!is_more_row(&View::Feed, 100, 99));
        assert!(!is_more_row(&View::Feed, 100, 101));
        assert_eq!(more_selection(&View::Feed, 100, Some(100), 200), Some(100));
        assert_eq!(more_selection(&View::Feed, 100, Some(100), 100), Some(100));
        assert_eq!(more_selection(&View::Feed, 100, Some(99), 200), None);
    }

    #[test]
    fn a_video_title_fills_only_the_matching_reels_empty_caption() {
        let (mut rows, _) = reels::parse(r#"{"rows":[{"url":"https://www.facebook.com/reel/1/","author":"Rock Bottom","text":""},{"url":"https://www.facebook.com/reel/2/","author":"Other","text":"Original caption"}]}"#).unwrap();
        assert!(!set_reel_title(
            &mut rows[1],
            "https://www.facebook.com/reel/1/",
            "Wrong reel"
        ));
        assert!(set_reel_title(
            &mut rows[0],
            "https://www.facebook.com/reel/1/",
            "Dedicated To Bart Simpson"
        ));
        assert_eq!(rows[0].field(4), "Dedicated To Bart Simpson");
        assert!(!set_reel_title(
            &mut rows[0],
            "https://www.facebook.com/reel/1/",
            "Replacement"
        ));
        assert_eq!(rows[1].field(4), "Original caption");
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
    fn more_keeps_distinct_refless_chat_and_friend_rows() {
        let rec = |fields: &[&str]| Record {
            author_ref: None,
            links: Vec::new(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            raw: fields.join("\t"),
        };
        let old = vec![rec(&["-", "", "1h", "Alice"])];
        let new = vec![rec(&["-", "", "1h", "Alice"]), rec(&["-", "", "2h", "Bob"])];
        let merged = merge_records(&View::Chats, old, new);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].field(3), "Bob");
        let old = vec![rec(&["Ann", "", "Friend"])];
        let new = vec![rec(&["Ann", "", "Friend"]), rec(&["Ben", "", "Friend"])];
        let merged = merge_records(&View::Friends, old, new);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].field(0), "Ben");
        // Rows WITH refs still dedupe on the ref.
        let old = vec![rec(&["/messages/t/1", "", "1h", "Alice"])];
        let new = vec![
            rec(&["/messages/t/1", "", "2h", "Alice"]),
            rec(&["/messages/t/2", "", "1h", "Bob"]),
        ];
        let merged = merge_records(&View::Chats, old, new);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn a_commenter_ref_only_attaches_to_the_comment_right_after_it() {
        let view = View::CommentsOf("post".to_owned());
        let raw = "cu\t/alice\nPost body line\nc\tBob\t1h\tHello";
        let (records, _) = parse_for(&view, raw).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].field(1), "");
        let raw = "cu\t/alice\nc\tAlice\t1h\tHi";
        let (records, _) = parse_for(&view, raw).unwrap();
        assert_eq!(records[0].field(1), "/alice");
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
            author_ref: None,
            links: Vec::new(),
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
        assert!(
            !list_label(&View::Chats, &row(&["c1", "-", "5m", "Bea", "hi"])).contains("no link")
        );
        assert_eq!(display_for(&View::Feed, &linked), "Alice. Hello. 5 min");
        assert_eq!(
            display_for(
                &View::Feed,
                &row(&["/a/posts/2", "Carol", "1m", "0/0/0", ""])
            ),
            "Carol. 1m"
        );
        assert_eq!(
            display_for(&View::Feed, &row(&["/a/posts/3", "Carol", "", "0/0/0", ""])),
            "Carol"
        );
    }

    #[test]
    fn routes_only_post_targets_and_detects_messenger_lock() {
        let row = |fields: &[&str]| Record {
            author_ref: None,
            links: Vec::new(),
            fields: fields.iter().map(|f| f.to_string()).collect(),
            raw: String::new(),
        };
        let url = "https://www.facebook.com/a/posts/1";
        assert_eq!(
            post_target(&View::Notifications, &row(&["1h", "!", "x", url])),
            Some(url)
        );
        assert_eq!(
            post_target(&View::Notifications, &row(&["1h", "!", "x", "2h"])),
            None
        );
        assert_eq!(
            post_target(&View::Feed, &row(&["p3k9a", "A"])),
            Some("p3k9a")
        );
        assert_eq!(post_target(&View::Feed, &row(&["-", "Ad"])), None);
        assert_eq!(post_target(&View::Chats, &row(&[url])), None);
        assert!(needs_messenger_unlock(
            "Error: Messenger history prompt blocked input; use `fb unlock`"
        ));
        // A locked thread that never loaded reports this instead; it must
        // offer the same unlock path rather than a bare error.
        assert!(needs_messenger_unlock(
            "thread did not load (e2ee chat locked? run `fb unlock`)"
        ));
        assert!(!needs_messenger_unlock("network error"));
    }

    #[test]
    fn a_reply_quote_never_follows_into_another_chat() {
        let a = View::ChatWith("/messages/t/1".into(), "Alice".into());
        let b = View::ChatWith("/messages/t/2".into(), "Bob".into());
        assert!(quote_follows_chat(&a, &a.clone()));
        assert!(!quote_follows_chat(&a, &b));
        assert!(!quote_follows_chat(&a, &View::Chats));
        assert!(!quote_follows_chat(&View::Chats, &a));
    }

    #[test]
    fn search_filter_combo_maps_every_label_to_a_backend_type() {
        let filters = search_filters();
        assert_eq!(filters.len(), 11);
        assert_eq!(filters[0], "Everything");
        assert_eq!(filters[10], "Marketplace");
        assert_eq!(search_kind(0), None);
        assert_eq!(search_kind(1), Some("posts"));
        assert_eq!(search_kind(3), Some("friends"));
        assert_eq!(search_kind(10), Some("marketplace"));
        for i in 0..filters.len() as u32 {
            // Every combo entry except Everything names a real --type.
            assert!(i == 0 || search_kind(i).is_some(), "filter {i} unmapped");
        }
    }

    #[test]
    fn marketplace_rows_read_like_search_listings() {
        let listing = Record {
            author_ref: None,
            links: Vec::new(),
            fields: vec![
                "Wooden chair".to_owned(),
                "https://www.facebook.com/marketplace/item/555".to_owned(),
                "$40 · Vancouver".to_owned(),
                "listing".to_owned(),
            ],
            raw: String::new(),
        };
        assert_eq!(
            display_for(&View::Marketplace, &listing),
            "Wooden chair — $40 · Vancouver"
        );
        // Listings are not post targets: Enter opens them in the browser
        // through the search-result dispatcher, and the row must not be
        // badged "(no link)".
        assert_eq!(post_target(&View::Marketplace, &listing), None);
        assert_eq!(
            list_label(&View::Marketplace, &listing),
            "Wooden chair — $40 · Vancouver"
        );
        assert_eq!(
            more_row_label(&View::Marketplace, 3).as_deref(),
            Some("Load more listings")
        );
        assert_eq!(default_args(&View::Marketplace), Some(marketplace_args(10)));
        assert_eq!(
            more_args(
                &View::Marketplace,
                &marketplace_args(PAGE),
                PAGE as usize,
                false
            ),
            Some(marketplace_args(PAGE + PAGE))
        );
    }

    #[test]
    fn search_rows_route_by_kind() {
        let row = |fields: &[&str]| Record {
            author_ref: None,
            links: Vec::new(),
            fields: fields.iter().map(|f| f.to_string()).collect(),
            raw: String::new(),
        };
        // A post row's target is its ref; the row reads "Name — info".
        let post = row(&["Alice", "u1abc", "Hello world", "post"]);
        assert_eq!(post_target(&View::SearchResults, &post), Some("u1abc"));
        assert_eq!(
            display_for(&View::SearchResults, &post),
            "Alice — Hello world"
        );
        // A photo row's target is its URL, so the reader can open it.
        let photo_url = "https://www.facebook.com/photo.php?fbid=9";
        let photo = row(&["Photo of a chair", photo_url, "", "photo"]);
        assert_eq!(post_target(&View::SearchResults, &photo), Some(photo_url));
        // Profiles, groups and listings are not post targets: their own
        // actions (profile view, group window, browser) handle them.
        assert_eq!(
            post_target(
                &View::SearchResults,
                &row(&["Bea", "u2def", "Friend", "profile"])
            ),
            None
        );
        assert_eq!(
            post_target(
                &View::SearchResults,
                &row(&["Chairs", "u3ghi", "Public group", "group"])
            ),
            None
        );
        assert_eq!(
            post_target(
                &View::SearchResults,
                &row(&[
                    "Wooden chair",
                    "https://www.facebook.com/marketplace/item/5/",
                    "$40",
                    "listing"
                ])
            ),
            None
        );
        // Rows from an older backend have no kind: no target, plain display.
        let legacy = row(&["Carol", "u4jkl", "Page"]);
        assert_eq!(post_target(&View::SearchResults, &legacy), None);
        assert_eq!(display_for(&View::SearchResults, &legacy), "Carol — Page");
    }

    #[test]
    fn search_rows_read_openable_and_carry_a_web_address() {
        let row = |fields: &[&str]| Record {
            author_ref: None,
            links: Vec::new(),
            fields: fields.iter().map(|f| f.to_string()).collect(),
            raw: String::new(),
        };
        // Issue #24: a profile result opens through its ref, so it must not
        // be labelled "(no link)"; only a targetless row earns the mark.
        let profile = row(&[
            "Bea",
            "u2def",
            "Friend",
            "profile",
            "https://www.facebook.com/bea",
        ]);
        assert_eq!(
            list_label(&View::SearchResults, &profile),
            "Bea — Friend"
        );
        let stranded = row(&["Nobody", "", "Nowhere", "profile"]);
        assert!(list_label(&View::SearchResults, &stranded).ends_with("(no link)"));
        // The fifth field never reaches the row's text or details.
        assert_eq!(
            display_for(&View::SearchResults, &profile),
            "Bea — Friend"
        );
        assert_eq!(
            details_for(&View::SearchResults, &profile),
            "Bea\nFriend"
        );
        // Open on web uses the row's own address, not the ref; rows from an
        // older backend fall back to the target.
        assert_eq!(
            search_web_target(&profile),
            "https://www.facebook.com/bea"
        );
        let photo = row(&[
            "Photo of a chair",
            "https://www.facebook.com/photo.php?fbid=9",
            "",
            "photo",
        ]);
        assert_eq!(
            search_web_target(&photo),
            "https://www.facebook.com/photo.php?fbid=9"
        );
        // Back from a result restores the search: the view is cacheable and
        // a refresh re-runs the last search's arguments.
        assert!(cacheable(
            &View::SearchResults,
            &search_args("bea", None, PAGE)
        ));
        assert_eq!(
            refresh_args(&View::SearchResults, &search_args("bea", None, PAGE)),
            Some(search_args("bea", None, PAGE))
        );
    }

    #[test]
    fn profile_rows_split_key_and_value_and_actions_read_as_actions() {
        // Issue #27: the profile's last row read as a bare "actions" and
        // Enter on it did nothing. The backend's key<TAB>value lines now
        // split into fields, and each row reads as what it is.
        let view = View::ProfileOf("u1234".into());
        let (records, _) = parse_for(
            &view,
            "name\tAlice Example\nurl\thttps://www.facebook.com/alice\nfriends\t312 friends\nintro\tHello there\nactions\tMessage | Add friend | Follow\n",
        )
        .unwrap();
        assert_eq!(records.len(), 5);
        assert_eq!(records[4].field(0), "actions");
        assert_eq!(records[4].field(1), "Message | Add friend | Follow");
        assert_eq!(display_for(&view, &records[0]), "Alice Example");
        assert_eq!(
            display_for(&view, &records[1]),
            "https://www.facebook.com/alice"
        );
        assert_eq!(display_for(&view, &records[2]), "Friends: 312 friends");
        assert_eq!(display_for(&view, &records[3]), "Intro: Hello there");
        assert_eq!(
            display_for(&view, &records[4]),
            "Actions: Message | Add friend | Follow"
        );
        assert_eq!(
            details_for(&view, &records[4]),
            "Actions: Message | Add friend | Follow"
        );
    }
}
