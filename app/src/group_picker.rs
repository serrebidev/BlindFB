//! Choose a joined group without borrowing the main window's paged cache.
use blindfb_core::fb::Fb;
use blindfb_core::posts::group_url;
use blindfb_core::{ntf, tf};
use patois::t;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use wxdragon::keycode::{WXK_NUMPAD_ENTER, WXK_RETURN};
use wxdragon::prelude::*;

#[derive(Clone)]
struct Group {
    name: String,
    url: String,
    info: String,
}

fn merge_groups(rows: &mut Vec<Group>, output: &str) -> Result<(), String> {
    let data: serde_json::Value =
        serde_json::from_str(output).map_err(|_| t("Could not load groups. Try again."))?;
    if let Some(error) = data["err"].as_str() {
        return Err(error.to_owned());
    }
    let records = data["rows"].as_array().ok_or_else(|| t("Could not load groups. Try again."))?;
    for record in records {
        let Some(url) = record.get(1).and_then(serde_json::Value::as_str).and_then(group_url)
        else {
            continue;
        };
        let name = record.get(0).and_then(serde_json::Value::as_str).unwrap_or_default().trim();
        if !name.is_empty() && !rows.iter().any(|row| row.url == url) {
            let info =
                record.get(2).and_then(serde_json::Value::as_str).unwrap_or_default().to_owned();
            rows.push(Group { name: name.to_owned(), url, info });
        }
    }
    Ok(())
}

fn next_count(requested: u32, loaded: usize) -> u32 {
    requested.max(u32::try_from(loaded).unwrap_or(u32::MAX)).saturating_add(100)
}

fn load_groups(
    page: &str,
    requests: mpsc::Receiver<u32>,
    results: mpsc::Sender<Result<String, String>>,
    mut run: impl FnMut(&[&str]) -> Result<String, String>,
) {
    let mut opened = false;
    for count in requests {
        let result = (|| {
            if !opened {
                run(&["page", page, "open", "https://www.facebook.com/groups/joins"])?;
                opened = true;
            }
            // Keep Facebook's scroll position and accumulated rows between requests.
            run(&[
                "page",
                page,
                "eval",
                &format!("window.__fb.groups({count},true).then(JSON.stringify)"),
            ])
        })();
        if result.is_err() {
            // The UI clears old rows on failure; a later explicit Retry starts a fresh scan.
            let _ = run(&["page", page, "close"]);
            opened = false;
        }
        if results.send(result).is_err() {
            break;
        }
    }
    if opened {
        let _ = run(&["page", page, "close"]);
    }
}

struct Picker {
    list: ListBox,
    status: TextCtrl,
    choose: Button,
    more: Button,
    retry: Button,
    rows: Vec<Group>,
    requested: u32,
    requests: Option<mpsc::Sender<u32>>,
    receiver: mpsc::Receiver<Result<String, String>>,
    loading: bool,
}

impl Picker {
    fn load(&mut self) {
        if self.loading {
            return;
        }
        if !self.requests.as_ref().is_some_and(|sender| sender.send(self.requested).is_ok()) {
            self.status.change_value(&t("Could not load groups. Try again."));
            return;
        }
        self.loading = true;
        self.more.enable(false);
        self.retry.enable(false);
        self.status.change_value(&t("Loading your groups…"));
    }

    fn selected(&self) -> Option<Group> {
        self.list.get_selection().and_then(|index| self.rows.get(index as usize)).cloned()
    }

    fn poll(&mut self) {
        if !self.loading {
            return;
        }
        let result = match self.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(t("Could not load groups. Try again.")),
        };
        self.loading = false;
        self.more.enable(true);
        self.retry.enable(true);
        let result = result.and_then(|output| {
            merge_groups(&mut self.rows, &output)?;
            Ok(output)
        });
        match result {
            Ok(_) => {
                let old = self.list.get_count() as usize;
                for row in &self.rows[old..] {
                    let label = if row.info.is_empty() {
                        format!("{} — {}", row.name, row.url)
                    } else {
                        format!("{} — {} — {}", row.name, row.info, row.url)
                    };
                    self.list.append(&label);
                }
                if old == 0 && !self.rows.is_empty() {
                    self.list.set_selection(0, true);
                }
                self.choose.enable(self.selected().is_some());
                self.status.change_value(&if self.rows.is_empty() {
                    t("No joined groups were found. Retry to check again.")
                } else {
                    ntf!(
                        "{n} group loaded. Load more checks for other groups.",
                        "{n} groups loaded. Load more checks for other groups.",
                        self.rows.len()
                    )
                });
            }
            Err(error) => {
                self.rows.clear();
                self.list.clear();
                self.choose.enable(false);
                self.status.change_value(&tf!(
                    "Could not load groups: {error}",
                    error = error.trim_start_matches("error: ").trim()
                ));
            }
        }
    }
}

pub(super) fn show(ui: &super::Ui) -> Option<(String, String)> {
    let dialog = Dialog::builder(&ui.frame, &t("Choose a group"))
        .with_size(760, 520)
        .with_style(DialogStyle::DefaultDialogStyle | DialogStyle::ResizeBorder)
        .build();
    dialog.set_escape_id(ID_CANCEL);
    let root = BoxSizer::builder(Orientation::Vertical).build();
    let label = StaticText::builder(&dialog).with_label(&t("Your &groups")).build();
    let list = ListBox::builder(&dialog).build();
    list.set_name(&t("Your groups. Type a name to find it; Enter chooses the selected group"));
    let status_label = StaticText::builder(&dialog).with_label(&t("Group loading status")).build();
    let status = TextCtrl::builder(&dialog).with_style(TextCtrlStyle::ReadOnly).build();
    status.set_name(&t("Group loading status"));
    status.enable_focus_from_keyboard();
    let choose = Button::builder(&dialog).with_id(ID_OK).with_label(&t("&Choose group")).build();
    choose.enable(false);
    let more = Button::builder(&dialog).with_label(&t("&Load more groups")).build();
    let retry = Button::builder(&dialog).with_label(&t("&Retry")).build();
    let cancel = Button::builder(&dialog).with_id(ID_CANCEL).with_label(&t("Cancel")).build();
    root.add(&label, 0, SizerFlag::All, 6);
    root.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 6);
    root.add(&status_label, 0, SizerFlag::All, 6);
    root.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 6);
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    for button in [&choose, &more, &retry, &cancel] {
        buttons.add(button, 0, SizerFlag::All, 6);
    }
    root.add_sizer(&buttons, 0, SizerFlag::All, 6);
    dialog.set_sizer(root, true);
    static NEXT_PAGE: AtomicU64 = AtomicU64::new(1);
    let page =
        format!("bg-{:x}-{:x}", std::process::id(), NEXT_PAGE.fetch_add(1, Ordering::Relaxed));
    let (requests, request_receiver) = mpsc::channel();
    let (results, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let fb = Fb::discover();
        load_groups(&page, request_receiver, results, |args| {
            fb.as_ref()
                .ok_or_else(|| t("The Facebook backend is not installed."))
                .and_then(|fb| fb.run(args))
        });
    });
    let picker = Rc::new(RefCell::new(Picker {
        list,
        status,
        choose,
        more,
        retry,
        rows: Vec::new(),
        requested: 100,
        requests: Some(requests),
        receiver,
        loading: false,
    }));
    let picked = Rc::new(RefCell::new(None));
    let (state, result) = (picker.clone(), picked.clone());
    let accept = Rc::new(move || {
        let selected = state.borrow().selected();
        if let Some(row) = selected {
            *result.borrow_mut() = Some((row.name, row.url));
            dialog.end_modal(ID_OK);
        }
    });
    let action = accept.clone();
    choose.on_click(move |_| action());
    let action = accept.clone();
    list.on_item_double_clicked(move |_| action());
    dialog.on_char_hook(move |event| {
        if let WindowEventData::Keyboard(key) = &event {
            if list.has_focus() && matches!(key.get_key_code(), Some(WXK_RETURN | WXK_NUMPAD_ENTER))
            {
                event.skip(false);
                accept();
                return;
            }
        }
        event.skip(true);
    });
    let state = picker.clone();
    list.on_selection_changed(move |_| choose.enable(state.borrow().selected().is_some()));
    super::type_ahead::attach(&list, move |_| choose.enable(true));
    let state = picker.clone();
    more.on_click(move |_| {
        let mut picker = state.borrow_mut();
        if !picker.loading {
            picker.requested = next_count(picker.requested, picker.rows.len());
            picker.load();
        }
    });
    let state = picker.clone();
    retry.on_click(move |_| state.borrow_mut().load());
    cancel.on_click(move |_| dialog.end_modal(ID_CANCEL));
    let timer = Timer::new(&dialog);
    let state = picker.clone();
    timer.on_tick(move |_| state.borrow_mut().poll());
    *ui.group_modal.borrow_mut() = Some(dialog);
    picker.borrow_mut().load();
    timer.start(120, false);
    list.set_focus();
    dialog.show_modal();
    timer.stop();
    drop(timer);
    let result = picked.borrow_mut().take();
    // A read in flight finishes on its worker, then closes the named page.
    picker.borrow_mut().requests.take();
    ui.group_modal.borrow_mut().take();
    dialog.destroy();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_reads_keep_old_groups_and_distinguish_duplicate_names() {
        let mut rows = Vec::new();
        merge_groups(&mut rows, r#"{"rows":[["Same name","/groups/1","Private"]]}"#).unwrap();
        merge_groups(
            &mut rows,
            r#"{"rows":[["Same name","/groups/2","Public"],["Same name","/groups/1/","Private"]]}"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url, "https://www.facebook.com/groups/1");
        assert_eq!(rows[1].url, "https://www.facebook.com/groups/2");
    }

    #[test]
    fn invalid_rows_never_become_post_destinations() {
        let mut rows = Vec::new();
        merge_groups(
            &mut rows,
            r#"{"rows":[["Bad","https://example.com/groups/1",""],["No URL"],["","/groups/2",""]]}"#,
        ).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn requesting_more_has_no_hundred_group_limit() {
        assert_eq!(next_count(100, 100), 200);
        assert_eq!(next_count(300, 247), 400);
        assert_eq!(next_count(1000, 1050), 1150);
    }

    #[test]
    fn more_groups_reuse_the_same_page_and_close_it() {
        let (requests, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        requests.send(100).unwrap();
        requests.send(200).unwrap();
        drop(requests);
        let mut commands = Vec::new();
        load_groups("test", receiver, sender, |args| {
            commands.push(args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            Ok(r#"{"rows":[]}"#.to_owned())
        });
        assert_eq!(results.try_iter().count(), 2);
        assert_eq!(commands.iter().filter(|args| args[2] == "open").count(), 1);
        assert_eq!(commands[1][3], "window.__fb.groups(100,true).then(JSON.stringify)");
        assert_eq!(commands[2][3], "window.__fb.groups(200,true).then(JSON.stringify)");
        assert_eq!(commands.last().unwrap()[2], "close");
    }

    #[test]
    fn closing_during_a_read_closes_the_page_without_reading_again() {
        let (requests, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        requests.send(100).unwrap();
        requests.send(200).unwrap();
        drop(results);
        let mut commands = Vec::new();
        load_groups("test", receiver, sender, |args| {
            commands.push(args[2].to_owned());
            Ok(String::new())
        });
        assert_eq!(commands, ["open", "eval", "close"]);
    }

    #[test]
    fn an_expired_page_reopens_only_on_the_next_explicit_request() {
        let (requests, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        requests.send(100).unwrap();
        requests.send(100).unwrap();
        drop(requests);
        let mut commands = Vec::new();
        let mut evaluations = 0;
        load_groups("test", receiver, sender, |args| {
            commands.push(args[2].to_owned());
            if args[2] == "eval" {
                evaluations += 1;
                if evaluations == 1 {
                    return Err("page expired; reopen explicitly".to_owned());
                }
            }
            Ok(r#"{"rows":[]}"#.to_owned())
        });
        let results: Vec<_> = results.try_iter().collect();
        assert!(results[0].as_ref().unwrap_err().contains("page expired"));
        assert!(results[1].is_ok());
        assert_eq!(commands, ["open", "eval", "close", "open", "eval", "close"]);
    }

    #[test]
    fn a_failed_open_still_closes_its_reserved_page() {
        let (requests, receiver) = mpsc::channel();
        let (sender, _results) = mpsc::channel();
        requests.send(100).unwrap();
        drop(requests);
        let mut commands = Vec::new();
        load_groups("test", receiver, sender, |args| {
            commands.push(args[2].to_owned());
            Err("could not open page".to_owned())
        });
        assert_eq!(commands, ["open", "close"]);
    }
}
