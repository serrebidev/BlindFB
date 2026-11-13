//! File > Keyboard shortcuts: give any menu command a shortcut, change or
//! remove one, or go back to the defaults. The post window's comment keys
//! (copy, copy all, open link) are in the same list. Choices are saved by
//! `blindfb_core::shortcuts`; the menus are rebuilt as soon as the window
//! closes, so a new shortcut works right away.

use std::cell::RefCell;
use std::rc::Rc;

use blindfb_core::shortcuts::{self, KEYS, Parts};
use blindfb_core::{tf, tray_hotkey};
use patois::t;
use wx_utils::{confirm, show_error, show_warning};
use wxdragon::prelude::*;

use crate::*;

/// Where a shortcut works: the main window's menus, or the post window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Menus,
    PostWindow,
}

/// One row of the list: what runs, where, its settings key and default.
struct Command {
    scope: Scope,
    key: String,
    default: &'static str,
    label: String,
}

/// Every menu command a shortcut can run: (section, menu id, settings key, default).
/// The tray toggle is left out: File > Show/hide hotkey sets that one system-wide.
fn menu_table() -> Vec<(&'static str, i32, String, &'static str)> {
    let mut rows: Vec<(&str, i32, String, &str)> = [
        ("file", ID_OPEN_LINK, "open_link", "Ctrl+O"),
        ("file", ID_NEW_POST, "new_post", "Ctrl+N"),
        ("file", ID_NEW_POST_IMAGES, "new_post_media", ""),
        ("file", ID_SELL, "sell", ""),
        ("file", ID_SEARCH, "search", "Ctrl+/"),
        ("file", ID_OCR, "ocr", ""),
        ("file", ID_UPDATE, "check_updates", ""),
        ("file", ID_TRAY_HOTKEY, "tray_hotkey_settings", ""),
        ("file", ID_SHORTCUTS, "keyboard_shortcuts", ""),
        ("file", crate::ID_EXIT, "exit", "Ctrl+Q"),
        ("view", ID_FEED, "feed", "Ctrl+1"),
        ("view", ID_NOTIFS, "notifications", "Ctrl+2"),
        ("view", ID_FRIENDS, "friends", "Ctrl+3"),
        ("view", ID_MYPOSTS, "my_posts", "Ctrl+4"),
        ("view", ID_CHATS, "chats", "Ctrl+5"),
        ("view", ID_GROUPS_VIEW, "groups", "Ctrl+6"),
        ("view", ID_RECENT_FEED, "recent_feed", "Ctrl+7"),
        ("view", ID_REELS, "reels", "Ctrl+8"),
        ("view", ID_MARKETPLACE, "marketplace", "Ctrl+9"),
        ("view", ID_EVENTS_VIEW, "events", ""),
        ("view", ID_PAGES_VIEW, "pages", ""),
        ("view", ID_PLACES_VIEW, "places", ""),
        ("view", ID_UNREAD_CHATS, "unread_chats", ""),
        ("view", ID_REFRESH, "refresh", "F5"),
        ("view", ID_MORE, "load_more", "Ctrl+L"),
        ("view", ID_VOICE_ALERTS, "voice_alerts", ""),
        ("view", ID_SOUND_ALERTS, "sound_alerts", ""),
        ("view", ID_LIST_VOICE_ALERTS, "list_voice_alerts", ""),
        ("view", ID_LIST_SOUND_ALERTS, "list_sound_alerts", ""),
        ("post", ID_OPEN, "open", ""),
        ("post", ID_SHARE, "share", ""),
        ("post", ID_COMMENT, "comment", ""),
        ("post", ID_POST_MENU, "post_menu", ""),
        ("post", ID_PROFILE, "author_profile", ""),
        ("post", ID_AUTHOR_POSTS, "author_posts", ""),
        ("post", ID_MESSAGE, "message", ""),
        ("post", ID_WEB_POST, "web_post", ""),
        ("post", ID_WEB_PERSON, "web_person", ""),
        ("post", ID_DELETE, "delete_post", ""),
        ("chat", ID_WEB_CHAT, "web_chat", ""),
        ("chat", ID_UNSEND, "unsend", ""),
        ("chat", ID_UNLOCK, "unlock", ""),
        ("backend", ID_INSTALL_BACKEND, "install_backend", ""),
        ("backend", ID_STATUS, "backend_status", ""),
        ("backend", ID_LOGIN, "sign_in", ""),
        ("backend", ID_INSTALL_BROWSER, "install_browser", ""),
        ("backend", ID_STOP_DAEMON, "stop_daemon", ""),
        ("backend", ID_QUIT_DAEMON, "quit_daemon", ""),
        ("help", ID_KEYS, "keyboard_commands", "F1"),
        ("help", ID_GITHUB, "repository", ""),
        ("help", ID_DEBUG_LOGGING, "debug_logging", ""),
        ("help", ID_DEBUG_REPORT, "debug_report", ""),
        ("help", crate::ID_ABOUT, "about", ""),
    ]
    .into_iter()
    .map(|(section, id, key, default)| (section, id, key.to_owned(), default))
    .collect();
    // Reactions follow Delete post in the Post menu.
    let at = rows.iter().position(|r| r.1 == ID_DELETE).map_or(rows.len(), |i| i + 1);
    for (i, reaction) in REACTIONS.iter().enumerate() {
        rows.insert(at + i, ("post", ID_REACT_BASE + i as i32, format!("react_{reaction}"), ""));
    }
    rows
}

/// The post window's comment keys: (settings key, default).
pub const COMMENT_COPY: (&str, &str) = ("comment_copy", "Ctrl+C");
pub const COMMENT_COPY_ALL: (&str, &str) = ("comment_copy_all", "Ctrl+Shift+C");
pub const COMMENT_OPEN_LINK: (&str, &str) = ("comment_open_link", "Ctrl+Enter");

/// The current shortcut of a post window key, for its handler and help text.
pub fn post_key((key, default): (&str, &str)) -> Option<String> {
    shortcuts::get(key, default)
}

/// A menu item label with its shortcut after the tab, the way wx reads accelerators.
pub fn menu_label(id: i32, label: String) -> String {
    let shortcut = menu_table()
        .into_iter()
        .find(|row| row.1 == id)
        .and_then(|(_, _, key, default)| shortcuts::get(&key, default));
    match shortcut {
        Some(shortcut) => format!("{label}\t{shortcut}"),
        None => label,
    }
}

fn section_name(section: &str) -> String {
    match section {
        "file" => t("&File"),
        "view" => t("&View"),
        "post" => t("&Post"),
        "chat" => t("&Chat"),
        "backend" => t("&Backend"),
        _ => t("&Help"),
    }
    .replace('&', "")
}

/// A menu item's label without its mnemonic and shortcut.
fn plain(label: &str) -> String {
    label.split('\t').next().unwrap_or_default().replace('&', "")
}

fn commands(frame: &Frame) -> Vec<Command> {
    let menu_bar = frame.get_menu_bar();
    let mut out: Vec<Command> = menu_table()
        .into_iter()
        .map(|(section, id, key, default)| {
            let item = menu_bar
                .as_ref()
                .and_then(|bar| bar.find_item(id))
                .map(|item| plain(&item.get_label()))
                .unwrap_or_else(|| key.clone());
            Command {
                scope: Scope::Menus,
                key,
                default,
                label: format!("{}: {item}", section_name(section)),
            }
        })
        .collect();
    let post = t("Post window");
    for ((key, default), what) in [
        (COMMENT_COPY, t("copy the selected comment")),
        (COMMENT_COPY_ALL, t("copy the post and all comments")),
        (COMMENT_OPEN_LINK, t("open the comment's first link")),
    ] {
        out.push(Command {
            scope: Scope::PostWindow,
            key: key.to_owned(),
            default,
            label: format!("{post}: {what}"),
        });
    }
    out
}

fn row_text(command: &Command) -> String {
    let shortcut =
        shortcuts::get(&command.key, command.default).unwrap_or_else(|| t("no shortcut"));
    format!("{}, {shortcut}", command.label)
}

/// Ctrl plus these edits text everywhere, so menus may not take them.
fn edits_text(parts: &Parts) -> bool {
    parts.ctrl && !parts.alt && ["A", "C", "V", "X", "Y", "Z"].contains(&parts.key.as_str())
}

/// The File > Keyboard shortcuts window. Returns once it is closed.
pub fn show(frame: &Frame) {
    let dialog = Dialog::builder(frame, &t("Keyboard shortcuts"))
        .with_size(720, 520)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&dialog).with_label(&t("&Commands")).build();
    let list = ListBox::builder(&dialog).build();
    list.set_name(&t("Commands and their shortcuts"));
    let rows = Rc::new(commands(frame));
    let fill = {
        let rows = rows.clone();
        move |list: &ListBox, keep: u32| {
            list.clear();
            for command in rows.iter() {
                list.append(&row_text(command));
            }
            list.set_selection(keep.min(rows.len().saturating_sub(1) as u32), true);
        }
    };
    fill(&list, 0);
    let change = Button::builder(&dialog).with_label(&t("C&hange shortcut…")).build();
    let remove = Button::builder(&dialog).with_label(&t("&Remove shortcut")).build();
    let reset = Button::builder(&dialog).with_label(&t("Reset to &default")).build();
    let reset_all = Button::builder(&dialog).with_label(&t("Reset &all")).build();
    let close = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("C&lose")).build();
    let actions = BoxSizer::builder(Orientation::Horizontal).build();
    for button in [&change, &remove, &reset, &reset_all, &close] {
        actions.add(button, 0, SizerFlag::All, 4);
    }
    root.add(&label, 0, SizerFlag::All, 4);
    root.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&actions, 0, SizerFlag::Expand, 4);
    dialog.set_sizer(root, true);

    let fill = Rc::new(fill);
    let store = move |dialog: &Dialog, command: &Command, shortcut: Option<&str>| {
        if let Err(error) = shortcuts::set(&command.key, command.default, shortcut) {
            show_error(dialog, &error, &t("Keyboard shortcuts"));
        }
    };
    let selected = move || list.get_selection().map(|i| i as usize);

    let (r, f, d, l, sel) = (rows.clone(), fill.clone(), dialog, list, selected);
    let edit = Rc::new(move || {
        let Some(index) = sel() else { return };
        let command = &r[index];
        let current = shortcuts::get(&command.key, command.default);
        let Some(choice) = pick(&d, &command.label, current.as_deref()) else { return };
        if let Some(parts) = choice.as_ref().and_then(|s| Parts::parse(s)) {
            if command.scope == Scope::Menus && edits_text(&parts) {
                show_warning(
                    &d,
                    tf!(
                        "{key} copies, pastes and edits text, so a menu cannot use it.",
                        key = parts.label()
                    ),
                    &t("Keyboard shortcuts"),
                );
                return;
            }
            let tray = tray_hotkey::saved().label().and_then(Parts::parse);
            if command.scope == Scope::Menus && tray.as_ref() == Some(&parts) {
                show_warning(
                    &d,
                    tf!(
                        "{key} is the show/hide hotkey. Pick another, or change the hotkey first.",
                        key = parts.label()
                    ),
                    &t("Keyboard shortcuts"),
                );
                return;
            }
            let clash = r.iter().enumerate().find(|(i, other)| {
                *i != index
                    && other.scope == command.scope
                    && shortcuts::get(&other.key, other.default)
                        .and_then(|s| Parts::parse(&s))
                        .as_ref()
                        == Some(&parts)
            });
            if let Some((_, other)) = clash {
                let question = tf!(
                    "{key} already runs \"{other}\". Move it to \"{this}\"?",
                    key = parts.label(),
                    other = other.label,
                    this = command.label
                );
                if !confirm(&d, &question, &t("Keyboard shortcuts")) {
                    return;
                }
                store(&d, other, None);
            }
        }
        store(&d, command, choice.as_deref());
        f(&l, index as u32);
    });
    let e = edit.clone();
    change.on_click(move |_| e());
    let e = edit.clone();
    list.on_item_double_clicked(move |_| e());
    let e = edit.clone();
    let l = list;
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if l.has_focus() && matches!(key.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER)) {
                event.skip(false);
                e();
                return;
            }
        }
        event.skip(true);
    });

    let (r, f, d, l, sel) = (rows.clone(), fill.clone(), dialog, list, selected);
    remove.on_click(move |_| {
        if let Some(index) = sel() {
            store(&d, &r[index], None);
            f(&l, index as u32);
        }
    });
    let (r, f, d, l, sel) = (rows.clone(), fill.clone(), dialog, list, selected);
    reset.on_click(move |_| {
        if let Some(index) = sel() {
            let command = &r[index];
            store(&d, command, Some(command.default).filter(|s| !s.is_empty()));
            f(&l, index as u32);
        }
    });
    let (r, f, d, l) = (rows.clone(), fill.clone(), dialog, list);
    reset_all.on_click(move |_| {
        if !confirm(&d, t("Put every shortcut back to its default?"), &t("Keyboard shortcuts")) {
            return;
        }
        for command in r.iter() {
            store(&d, command, Some(command.default).filter(|s| !s.is_empty()));
        }
        f(&l, l.get_selection().unwrap_or(0));
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
}

/// Ask for a shortcut: Ctrl, Alt and Shift boxes and a key list, whose first
/// entry is "no shortcut". `None` when cancelled; `Some(None)` for no shortcut.
fn pick(parent: &Dialog, label: &str, current: Option<&str>) -> Option<Option<String>> {
    let dialog = Dialog::builder(parent, &t("Change shortcut")).build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let what = StaticText::builder(&dialog)
        .with_label(&tf!("Shortcut for {command}", command = label))
        .build();
    let ctrl = CheckBox::builder(&dialog).with_label(&t("&Ctrl")).build();
    let alt = CheckBox::builder(&dialog).with_label(&t("&Alt")).build();
    let shift = CheckBox::builder(&dialog).with_label(&t("&Shift")).build();
    let key_label = StaticText::builder(&dialog).with_label(&t("&Key")).build();
    let key = Choice::builder(&dialog).build();
    key.append(&t("no shortcut"));
    for name in KEYS {
        key.append(name);
    }
    let parts = current.and_then(Parts::parse);
    if let Some(parts) = &parts {
        ctrl.set_value(parts.ctrl);
        alt.set_value(parts.alt);
        shift.set_value(parts.shift);
        let at = KEYS.iter().position(|k| *k == parts.key).map_or(0, |i| i + 1);
        key.set_selection(at as u32);
    } else {
        key.set_selection(0);
    }
    let ok = Button::builder(&dialog).with_id(ID_OK).with_label(&t("OK")).build();
    let cancel = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Cancel")).build();
    ok.set_default();
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    buttons.add(&ok, 0, SizerFlag::All, 4);
    buttons.add(&cancel, 0, SizerFlag::All, 4);
    root.add(&what, 0, SizerFlag::All, 4);
    for control in [&ctrl, &alt, &shift] {
        root.add(control, 0, SizerFlag::All, 4);
    }
    root.add(&key_label, 0, SizerFlag::All, 4);
    root.add(&key, 0, SizerFlag::Expand | SizerFlag::All, 4);
    root.add_sizer(&buttons, 0, SizerFlag::Expand, 4);
    dialog.set_sizer_and_fit(root, true);

    let picked: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));
    let (p, d) = (picked.clone(), dialog);
    ok.on_click(move |_| {
        let index = key.get_selection().unwrap_or(0) as usize;
        if index == 0 {
            *p.borrow_mut() = Some(None);
            d.end_modal(ID_OK);
            return;
        }
        let parts = Parts {
            ctrl: ctrl.get_value(),
            alt: alt.get_value(),
            shift: shift.get_value(),
            key: KEYS[index - 1].to_owned(),
        };
        if parts.types_text() {
            show_warning(
                &d,
                t("A letter, digit, space or punctuation key needs Ctrl or Alt, or it would type text."),
                &t("Change shortcut"),
            );
            return;
        }
        *p.borrow_mut() = Some(Some(parts.label()));
        d.end_modal(ID_OK);
    });
    let d = dialog;
    cancel.on_click(move |_| d.end_modal(ID_CANCEL));
    ctrl.set_focus();
    dialog.show_modal();
    dialog.destroy();
    picked.take()
}
