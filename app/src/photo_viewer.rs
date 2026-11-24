//! A Facebook photo alone on the screen, at full size: what Be My Eyes, the
//! screen reader's own picture description or its OCR need to see. Nothing
//! loads until asked, so reading posts stays as fast as before.

use super::{Job, Tag, Task, Ui, show_error};
use blindfb_core::tf;
use patois::t;
use wx_utils::show_info;
use wxdragon::prelude::*;

/// Fetch the full-size picture behind a Facebook photo link; [`show`] opens it.
pub(super) fn request(ui: &Ui, url: &str) {
    let message = t("Opening the photo…");
    match ui.post_reader.borrow().as_ref() {
        Some(reader) => reader.message(ui, &message),
        None => ui.status(&message),
    }
    let _ =
        ui.jobs.send(Job { task: Task::Photo(blindfb_core::fb::web_url(url)), tag: Tag::Photo });
}

/// The downloaded photo, full screen over `modal` (the window the user is in).
pub(super) fn show(ui: &Ui, modal: Option<Dialog>, result: Result<String, String>) {
    let parent: &dyn WxWidget = match &modal {
        Some(dialog) => dialog,
        None => &ui.frame,
    };
    let opened = result.and_then(|raw| {
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let path = value["path"].as_str().unwrap_or_default().to_owned();
        let alt = value["alt"].as_str().unwrap_or_default().trim().to_owned();
        let picture = image::open(&path)
            .map_err(|e| tf!("The photo could not be opened: {error}", error = e))?
            .to_rgba8();
        let bitmap = Bitmap::from_rgba(picture.as_raw(), picture.width(), picture.height())
            .ok_or_else(|| t("The photo could not be opened."))?;
        Ok((path, alt, bitmap))
    });
    let (path, alt, bitmap) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            ui.status(&t("Error"));
            show_error(parent, &error, &t("View photo"));
            return;
        }
    };
    ui.status(&t("Done"));
    let description =
        if alt.is_empty() { t("Photo") } else { tf!("Photo: {description}", description = alt) };
    let title = tf!(
        "{description}. Escape closes, Ctrl+C copies the photo, Ctrl+S saves it.",
        description = description
    );
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
    frame.on_char_hook(move |event| {
        let WindowEventData::Keyboard(key) = &event else {
            event.skip(true);
            return;
        };
        let code = key.get_key_code();
        if code == Some(wxdragon::keycode::WXK_ESCAPE) {
            event.skip(false);
            frame.close(true);
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
    frame.show(true);
    frame.show_full_screen(true);
    frame.raise();
    frame.set_focus();
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
