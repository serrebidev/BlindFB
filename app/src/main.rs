//! BlindFB: an accessible Facebook and Messenger client for screen reader
//! users, built with wxDragon (native wxWidgets controls throughout, so
//! lists, menus and dialogs are the real platform widgets) on top of the
//! `fb` command-line backend (hieu-lee/facebook-cli).
//!
//! Everything Facebook-related goes through `blindfb_core::fb`. Commands run
//! on one worker thread; results come back over a channel that a timer
//! drains, so the UI never blocks on the network or on Chrome.
//!
//! Not supported, because the backend has no such commands: voice and video
//! calls, and adding/removing friends. The friends list can be browsed and
//! searched, and any friend can be profiled, read and messaged.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use blindfb_core::fb::{
    self, Fb, Record, chats_args, comment_args, delete_args, feed_args, friends_args, notifs_args,
    parse_records, post_args, posts_args, profile_args, react_args, read_args, search_args,
    send_args, unsend_args, view_args,
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
const ID_UNSEND: i32 = 10050;
const ID_UNLOCK: i32 = 10051;
const ID_STATUS: i32 = 10060;
const ID_LOGIN: i32 = 10061;
const ID_STOP_DAEMON: i32 = 10062;
const ID_QUIT_DAEMON: i32 = 10063;
const ID_ABOUT: i32 = 10070;
const ID_GITHUB: i32 = 10071;

const REACTIONS: [&str; 8] = ["like", "love", "care", "haha", "wow", "sad", "angry", "none"];
const PAGE: u32 = 40;

#[derive(Debug, Clone, PartialEq)]
enum View {
    Feed,
    Notifications,
    Friends,
    MyPosts,
    Chats,
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
            View::Notifications => "Notifications".to_owned(),
            View::Friends => "Friends".to_owned(),
            View::MyPosts => "My posts".to_owned(),
            View::Chats => "Chats".to_owned(),
            View::SearchResults => "Search results".to_owned(),
            View::PostsOf(who) => format!("Posts by {who}"),
            View::CommentsOf(_) => "Post and comments".to_owned(),
            View::ProfileOf(who) => format!("Profile of {who}"),
            View::ChatWith(name) => format!("Chat with {name}"),
        }
    }
}

enum Tag {
    List(View),
    Text,
    ReloadChat(String),
    UpdateCheck,
}

struct Job {
    args: Vec<String>,
    stdin: Option<String>,
    tag: Tag,
}

struct Done {
    tag: Tag,
    result: Result<String, String>,
}

struct State {
    view: View,
    records: Vec<Record>,
    who: Option<String>,
    attachments: Vec<String>,
}

fn main() {
    let _ = wxdragon::main(|_| {
        run();
    });
}

fn run() {
    let fb = Fb::discover();
    let (job_tx, job_rx) = channel::<Job>();
    let (done_tx, done_rx) = channel::<Done>();
    spawn_worker(fb.clone(), job_rx, done_tx.clone());

    let frame = Frame::builder()
        .with_title("BlindFB")
        .with_size(Size::new(1100, 760))
        .build();
    frame.create_status_bar(1, 0, ID_ANY, "status");
    frame.set_status_text(
        if fb.is_some() {
            "Ready. Pick a view to start."
        } else {
            "The fb backend was not found. See the README for install steps, or set BLINDFB_FB."
        },
        0,
    );

    let panel = Panel::builder(&frame).build();
    let root = BoxSizer::builder(Orientation::Vertical).build();

    let views_label = StaticText::builder(&panel).with_label("Views").build();
    let views = ListBox::builder(&panel).build();
    for name in ["News feed", "Notifications", "Friends", "My posts", "Chats"] {
        views.append(name);
    }
    views.set_selection(0, true);

    let items_label = StaticText::builder(&panel).with_label("Items").build();
    let items = ListBox::builder(&panel)
        .with_size(Size::new(-1, 260))
        .build();

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
        &refresh_btn, &open_btn, &react_btn, &comment_btn, &profile_btn, &posts_btn, &message_btn,
        &delete_btn,
    ] {
        actions.add(b, 0, SizerFlag::All, 3);
    }

    let compose = BoxSizer::builder(Orientation::Horizontal).build();
    let compose_label = StaticText::builder(&panel).with_label("Message").build();
    let compose_box = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::ProcessEnter)
        .build();
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
    panel.set_sizer(root);

    build_menus(&frame);

    let state = Rc::new(RefCell::new(State {
        view: View::Feed,
        records: Vec::new(),
        who: None,
        attachments: Vec::new(),
    }));
    let ui = Ui {
        frame,
        items,
        details,
        compose_box,
        state: state.clone(),
        jobs: job_tx.clone(),
        done_tx,
    };

    // Feed is the home view; load it up front when the backend exists.
    if fb.is_some() {
        load_view(&ui, View::Feed);
    }

    let ui_views = ui.clone();
    views.on_selection_changed(move |event| {
        if let Some(sel) = event.get_selection() {
            let view = match sel {
                0 => View::Feed,
                1 => View::Notifications,
                2 => View::Friends,
                3 => View::MyPosts,
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
    timer.on_tick(move |_| {
        while let Ok(done) = done_rx.try_recv() {
            apply_done(&ui_tick, done);
        }
    });
    timer.start(120, false);

    frame.centre();
    frame.show(true);
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

fn spawn_worker(fb: Option<Fb>, jobs: Receiver<Job>, done: Sender<Done>) {
    thread::spawn(move || {
        while let Ok(job) = jobs.recv() {
            let result = match &fb {
                None => Err(
                    "the fb backend was not found; install facebook-cli or set BLINDFB_FB"
                        .to_owned(),
                ),
                Some(fb) => match &job.stdin {
                    Some(input) => fb.run_stdin(&refs(&job.args), input),
                    None => fb.run(&refs(&job.args)),
                },
            };
            if done.send(Done { tag: job.tag, result }).is_err() {
                break;
            }
        }
    });
}

fn refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

fn build_menus(frame: &Frame) {
    let file_menu = Menu::builder()
        .append_item(ID_NEW_POST, "&New post…\tCtrl+N", "Write a new post")
        .append_item(ID_NEW_POST_IMAGES, "New post with &images…", "Write a post with images attached")
        .append_item(ID_SEARCH, "&Search Facebook…\tCtrl+F", "Search people, pages, groups, posts and events")
        .append_item(ID_OCR, "&OCR an image…", "Read the text in an image file with the operating system's OCR engine")
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
        .append_item(ID_UNREAD_CHATS, "&Unread chats only", "Chats with unread messages")
        .append_separator()
        .append_item(ID_REFRESH, "&Refresh\tF5", "Reload the current view")
        .build();

    let mut post_menu = Menu::builder()
        .append_item(ID_OPEN, "&Open post / comments\tEnter", "Open the selected item")
        .append_item(ID_COMMENT, "&Comment…", "Comment on the selected post")
        .append_item(ID_PROFILE, "Author &profile", "Show the profile of the selected item's author or owner")
        .append_item(ID_AUTHOR_POSTS, "Author's p&osts", "Posts by the selected item's author")
        .append_item(ID_MESSAGE, "&Message author / friend", "Open a Messenger chat with this person")
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
        .append_item(ID_UNSEND, "&Unsend my last message…", "Remove your newest message in the open chat for everyone")
        .append_item(ID_UNLOCK, "Unloc&k encrypted chats…", "Enter your Messenger PIN to read end-to-end encrypted chats")
        .build();

    let backend_menu = Menu::builder()
        .append_item(ID_STATUS, "Backend &status", "Show fb daemon status")
        .append_item(ID_LOGIN, "&Log in…", "Import your Facebook session from Chrome")
        .append_item(ID_STOP_DAEMON, "S&top daemon", "Stop the fb daemon")
        .append_item(ID_QUIT_DAEMON, "&Quit daemon and Chrome", "Stop the daemon and close its hidden Chrome")
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

fn load_args(ui: &Ui, view: View, args: Vec<String>) {
    ui.state.borrow_mut().view = view.clone();
    ui.status(&format!("Loading {}…", view.title()));
    ui.send(Job { args, stdin: None, tag: Tag::List(view) });
}

fn load_view(ui: &Ui, view: View) {
    let args = match &view {
        View::Feed => feed_args(PAGE, false, false),
        View::Notifications => notifs_args(PAGE),
        View::Friends => friends_args(None, 100),
        View::MyPosts => posts_args("me", PAGE),
        View::Chats => chats_args(PAGE, false),
        View::SearchResults => return,
        View::PostsOf(who) => posts_args(who, PAGE),
        View::CommentsOf(post) => view_args(post, 50),
        View::ProfileOf(who) => profile_args(who),
        View::ChatWith(name) => read_args(name, 50),
    };
    load_args(ui, view, args);
}

fn refresh(ui: &Ui) {
    let view = ui.state.borrow().view.clone();
    if view == View::Feed {
        load_args(ui, view, feed_args(PAGE, true, false));
    } else {
        load_view(ui, view);
    }
}

fn display_for(view: &View, record: &Record) -> String {
    match view {
        View::Feed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
            if record.fields.len() >= 5 {
                format!(
                    "{} · {} · {} — {}",
                    record.field(1),
                    record.field(2),
                    record.field(3),
                    truncate(&record.fields[4..].join(" "), 150)
                )
            } else {
                record.raw.clone()
            }
        }
        View::Notifications => {
            if record.fields.len() >= 3 {
                format!("{} {}", record.field(0), record.fields[2..].join(" "))
            } else {
                record.raw.clone()
            }
        }
        View::Friends => {
            if record.fields.len() >= 3 {
                format!("{} — {} mutual friends", record.field(0), record.field(2))
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
        View::Feed | View::MyPosts | View::PostsOf(_) | View::SearchResults => {
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
            "Name: {}\nRef: {}\nMutual friends: {}",
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
        _ => record.raw.clone(),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max).collect();
        format!("{cut}…")
    }
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
    state.records.get(index).cloned().map(|r| (index, r))
}

fn selected_post_ref(ui: &Ui) -> Option<String> {
    let (_, record) = selected(ui)?;
    record.refr().map(str::to_owned)
}

fn apply_done(ui: &Ui, done: Done) {
    match done.tag {
        Tag::List(view) => match done.result {
            Ok(raw) => {
                let (records, body) = parse_for(&view, &raw);
                ui.items.clear();
                for record in &records {
                    ui.items.append(&display_for(&view, record));
                }
                if !records.is_empty() {
                    ui.items.set_selection(0, true);
                }
                let mut state = ui.state.borrow_mut();
                state.records = records;
                state.view = view.clone();
                drop(state);
                if let Some(body) = body {
                    ui.details.set_value(&body);
                } else if ui.items.get_count() > 0 {
                    show_details(ui, 0);
                } else {
                    ui.details.set_value("(nothing here)");
                }
                ui.status(&format!("{} — {} item(s)", view.title(), ui.items.get_count()));
            }
            Err(e) => {
                ui.status("Error");
                show_error(&ui.frame, &e, "BlindFB");
            }
        },
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
                        &format!("You have the latest BlindFB ({}).", env!("CARGO_PKG_VERSION")),
                        "Check for updates",
                    );
                }
            }
            Err(e) => show_error(&ui.frame, &e, "Check for updates"),
        },
    }
}

fn parse_for(view: &View, raw: &str) -> (Vec<Record>, Option<String>) {
    match view {
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
        _ => (parse_records(raw), None),
    }
}

fn open_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else { return };
    let view = ui.state.borrow().view.clone();
    match view {
        View::Feed | View::MyPosts | View::PostsOf(_) | View::SearchResults
        | View::Notifications => {
            if let Some(post) = record.refr() {
                load_view(ui, View::CommentsOf(post.to_owned()));
            }
        }
        View::Chats => {
            let name = record.field(3).to_owned();
            if !name.is_empty() {
                load_view(ui, View::ChatWith(name));
            }
        }
        View::Friends => profile_selected(ui),
        _ => {}
    }
}

fn react_popup(ui: &Ui) {
    let Some(post) = selected_post_ref(ui) else {
        show_warning(&ui.frame, "Select a post first.", "React");
        return;
    };
    let labels: Vec<&str> = REACTIONS.to_vec();
    let dialog = SingleChoiceDialog::builder(&ui.frame, "Reaction", "React", &labels).build();
    if dialog.show_modal() == ID_OK {
        let index = dialog.get_selection();
        if index >= 0 {
            let reaction = REACTIONS[index as usize];
            ui.send(Job {
                args: react_args(&post, reaction),
                stdin: None,
                tag: Tag::Text,
            });
        }
    }
}

fn comment_selected(ui: &Ui) {
    let Some(post) = selected_post_ref(ui) else {
        show_warning(&ui.frame, "Select a post first.", "Comment");
        return;
    };
    if let Some(text) = prompt_text(&ui.frame, "Your comment:", "Comment") {
        if !text.trim().is_empty() {
            ui.send(Job { args: comment_args(&post, &text), stdin: None, tag: Tag::Text });
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
    let Some((_, record)) = selected(ui) else { return };
    if let Some(who) = person_of(ui, &record) {
        load_view(ui, View::ProfileOf(who));
    }
}

fn posts_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else { return };
    if let Some(who) = person_of(ui, &record) {
        load_view(ui, View::PostsOf(who));
    }
}

fn message_selected(ui: &Ui) {
    let Some((_, record)) = selected(ui) else { return };
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
        ui.send(Job { args: delete_args(&post), stdin: None, tag: Tag::Text });
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
        args: send_args(&chat, &text, &files),
        stdin: None,
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
        ID_UNREAD_CHATS => load_args(ui, View::Chats, chats_args(PAGE, true)),
        ID_REFRESH => refresh(ui),
        ID_OPEN => open_selected(ui),
        ID_COMMENT => comment_selected(ui),
        ID_PROFILE => profile_selected(ui),
        ID_AUTHOR_POSTS => posts_selected(ui),
        ID_MESSAGE => message_selected(ui),
        ID_DELETE => delete_selected(ui),
        ID_NEW_POST => new_post(ui, false),
        ID_NEW_POST_IMAGES => new_post(ui, true),
        ID_SEARCH => search_dialog(ui),
        ID_OCR => ocr_dialog(ui),
        ID_UPDATE => check_updates(ui),
        ID_STATUS => ui.send(Job { args: vec!["status".into()], stdin: None, tag: Tag::Text }),
        ID_STOP_DAEMON => ui.send(Job { args: vec!["stop".into()], stdin: None, tag: Tag::Text }),
        ID_QUIT_DAEMON => ui.send(Job { args: vec!["quit".into()], stdin: None, tag: Tag::Text }),
        ID_LOGIN => {
            if confirm(
                &ui.frame,
                "Log in imports your Facebook session from your normal Chrome. If Chrome asks \"Allow remote debugging?\", click Allow. Continue?",
                "Log in",
            ) {
                ui.send(Job { args: vec!["login".into()], stdin: None, tag: Tag::Text });
            }
        }
        ID_UNLOCK => {
            let dialog = TextEntryDialog::builder(&ui.frame, "Messenger PIN:", "Unlock encrypted chats")
                .password()
                .build();
            if dialog.show_modal() == ID_OK {
                if let Some(pin) = dialog.get_value() {
                    ui.send(Job {
                        args: vec!["unlock".into(), "-".into()],
                        stdin: Some(format!("{pin}\n")),
                        tag: Tag::Text,
                    });
                }
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
                        args: unsend_args(&chat, None),
                        stdin: None,
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
                if let Some(post) = selected_post_ref(ui) {
                    let reaction = REACTIONS[(id - ID_REACT_BASE) as usize];
                    ui.send(Job {
                        args: react_args(&post, reaction),
                        stdin: None,
                        tag: Tag::Text,
                    });
                }
            }
        }
    }
}

fn new_post(ui: &Ui, with_images: bool) {
    let Some(text) = prompt_text(&ui.frame, "Post text:", "New post") else { return };
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
    let dialog = SingleChoiceDialog::builder(&ui.frame, "Who can see this?", "Audience", &labels).build();
    let mut audience = None;
    if dialog.show_modal() == ID_OK {
        audience = match dialog.get_selection() {
            0 => Some("public"),
            1 => Some("friends"),
            2 => Some("me"),
            _ => None,
        };
    }
    ui.send(Job { args: post_args(&text, audience, &images), stdin: None, tag: Tag::Text });
}

fn search_dialog(ui: &Ui) {
    let Some(query) = prompt_text(&ui.frame, "Search for:", "Search Facebook") else { return };
    if query.trim().is_empty() {
        return;
    }
    let labels = ["Everything", "People", "Pages", "Groups", "Posts", "Events"];
    let dialog = SingleChoiceDialog::builder(&ui.frame, "Search in:", "Search Facebook", &labels).build();
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
    let Some(path) = dialog.get_path() else { return };
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
    native_ocr::recognize_rgba(rgba.as_raw(), width, height)
        .map_err(|e| format!("OCR failed: {e}"))
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
