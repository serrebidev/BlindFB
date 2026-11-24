//! A Facebook photo alone on the screen, at full size: what Be My Eyes, the
//! screen reader's own picture description or its OCR need to see. Nothing
//! loads until asked, so reading posts stays as fast as before. The arrow
//! keys walk the other photos of the same post or album.

use super::{Job, Tag, Task, Ui, show_error};
use blindfb_core::posts::photo_id;
use blindfb_core::tf;
use patois::t;
use std::cell::RefCell;
use wx_utils::show_info;
use wxdragon::keycode::{WXK_ESCAPE, WXK_LEFT, WXK_PAGEDOWN, WXK_PAGEUP, WXK_RIGHT};
use wxdragon::prelude::*;

/// The viewer on screen: where the arrows move from, and the window a new photo replaces.
struct Open {
    frame: Frame,
    /// The photo page now showing.
    url: String,
    /// The photo first opened, to tell when the walk has gone all the way round.
    start: String,
    title: String,
    loading: bool,
}

thread_local! {
    // GUI thread only: every caller is a wx event or the results timer.
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

/// Fetch the full-size picture behind a Facebook photo link; [`show`] opens it.
pub(super) fn request(ui: &Ui, url: &str) {
    let message = t("Opening the photo…");
    match ui.post_reader.borrow().as_ref() {
        Some(reader) => reader.message(ui, &message),
        None => ui.status(&message),
    }
    send(ui, &blindfb_core::fb::web_url(url), 0);
}

fn send(ui: &Ui, url: &str, step: i32) {
    let _ = ui
        .jobs
        .send(Job { task: Task::Photo { url: url.to_owned(), step }, tag: Tag::Photo(step) });
}

/// Move the open viewer to the next (1) or previous (-1) photo.
fn step(ui: &Ui, step: i32) {
    let url = OPEN.with(|open| {
        let mut open = open.borrow_mut();
        let open = open.as_mut().filter(|open| !open.loading)?;
        open.loading = true;
        open.frame.set_title(&if step > 0 {
            t("Loading the next photo…")
        } else {
            t("Loading the previous photo…")
        });
        Some(open.url.clone())
    });
    if let Some(url) = url {
        send(ui, &url, step);
    }
}

/// The downloaded photo, full screen over `modal` (the window the user is in).
/// A photo the arrows asked for replaces the one showing.
pub(super) fn show(ui: &Ui, modal: Option<Dialog>, step: i32, result: Result<String, String>) {
    // Only a step reuses the walk: a photo opened afresh starts its own.
    let previous = OPEN.with(|open| open.borrow_mut().take());
    if step != 0 && previous.is_none() {
        // The viewer was closed while the next photo loaded.
        return;
    }
    let opened = result.and_then(|raw| {
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let path = value["path"].as_str().unwrap_or_default().to_owned();
        let url = value["url"].as_str().unwrap_or_default().to_owned();
        let alt = value["alt"].as_str().unwrap_or_default().trim().to_owned();
        let picture = image::open(&path)
            .map_err(|e| tf!("The photo could not be opened: {error}", error = e))?
            .to_rgba8();
        let bitmap = Bitmap::from_rgba(picture.as_raw(), picture.width(), picture.height())
            .ok_or_else(|| t("The photo could not be opened."))?;
        Ok((path, url, alt, bitmap))
    });
    let (path, url, alt, bitmap) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            ui.status(&t("Error"));
            match previous {
                // A failed step leaves the photo that was showing, and says why over it.
                Some(mut open) if step != 0 => {
                    open.loading = false;
                    open.frame.set_title(&open.title);
                    show_error(&open.frame, &error, &t("View photo"));
                    let frame = open.frame;
                    OPEN.with(|slot| *slot.borrow_mut() = Some(open));
                    frame.set_focus();
                }
                previous => {
                    OPEN.with(|slot| *slot.borrow_mut() = previous);
                    let parent: &dyn WxWidget = match &modal {
                        Some(dialog) => dialog,
                        None => &ui.frame,
                    };
                    show_error(parent, &error, &t("View photo"));
                }
            }
            return;
        }
    };
    ui.status(&t("Done"));
    let start = match &previous {
        Some(open) if step != 0 => open.start.clone(),
        _ => photo_id(&url).to_owned(),
    };
    let description =
        if alt.is_empty() { t("Photo") } else { tf!("Photo: {description}", description = alt) };
    let mut title = tf!(
        "{description}. Right and Left arrows: next and previous photo. Escape closes, Ctrl+C copies the photo, Ctrl+S saves it.",
        description = description
    );
    if step != 0 && photo_id(&url) == start {
        title = tf!("Back at the first photo you opened. {title}", title = title);
    }
    let frame = match &modal {
        Some(dialog) => Frame::builder().with_parent(dialog),
        None => Frame::builder().with_parent(&ui.frame),
    }
    .with_title(&title)
    .build();
    frame.set_background_color(wxdragon::color::colours::BLACK);
    let picture =
        wxdragon::widgets::GenericStaticBitmap::new_with_bitmap(&frame, ID_ANY as Id, &bitmap);
    picture.set_scale_mode(ScaleMode::AspectFit);
    picture.set_name(&description);
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    sizer.add(&picture, 1, SizerFlag::Expand, 0);
    frame.set_sizer(sizer, true);
    let u = ui.clone();
    frame.on_char_hook(move |event| {
        let WindowEventData::Keyboard(key) = &event else {
            event.skip(true);
            return;
        };
        let code = key.get_key_code();
        if code == Some(WXK_ESCAPE) {
            event.skip(false);
            frame.close(true);
        } else if matches!(code, Some(WXK_RIGHT | WXK_PAGEDOWN)) {
            event.skip(false);
            self::step(&u, 1);
        } else if matches!(code, Some(WXK_LEFT | WXK_PAGEUP)) {
            event.skip(false);
            self::step(&u, -1);
        } else if key.control_down() && code == Some('C' as i32) {
            event.skip(false);
            let clipboard = Clipboard::get();
            let copied = clipboard.open() && {
                let copied = clipboard.set_data(&mut BitmapDataObject::new(&bitmap));
                clipboard.flush();
                clipboard.close();
                copied
            };
            if copied {
                show_info(&frame, t("Photo copied."), &t("View photo"));
            } else {
                show_error(&frame, t("Could not copy to the clipboard."), &t("View photo"));
            }
        } else if key.control_down() && code == Some('S' as i32) {
            event.skip(false);
            save(&frame, &path);
        } else {
            event.skip(true);
        }
    });
    let id = photo_id(&url).to_owned();
    frame.on_close(move |event| {
        // Escape or Alt+F4: forget this window, unless a newer photo already replaced it.
        OPEN.with(|open| {
            let mut open = open.borrow_mut();
            if open.as_ref().is_some_and(|open| photo_id(&open.url) == id) {
                *open = None;
            }
        });
        event.skip(true);
    });
    frame.show(true);
    frame.show_full_screen(true);
    frame.raise();
    frame.set_focus();
    OPEN.with(|open| {
        *open.borrow_mut() = Some(Open { frame, url, start, title, loading: false });
    });
    // Close the old window after the new one has the screen, so focus never drops to the reader.
    if let Some(old) = previous {
        old.frame.close(true);
    }
}

fn save(frame: &Frame, path: &str) {
    let dialog = FileDialog::builder(frame)
        .with_message(&t("Save photo"))
        .with_default_file("photo.jpg")
        // TRANSLATORS: a file-type filter; translate "JPEG pictures" only, keep "(*.jpg)|*.jpg".
        .with_wildcard(&t("JPEG pictures (*.jpg)|*.jpg"))
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    if dialog.show_modal() != ID_OK {
        return;
    }
    if let Some(target) = dialog.get_path() {
        if let Err(error) = std::fs::copy(path, &target) {
            show_error(
                frame,
                tf!("The photo could not be saved: {error}", error = error),
                &t("Save photo"),
            );
        }
    }
}
