//! Voice and sound alerts for new notifications and unread chats. Both start
//! off; each is remembered across restarts. Spoken text never reaches the log.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use crate::backend::data_dir;
use crate::fb::{parse_notifications, parse_records};

/// Speech and sound are one at a time, so two alerts never talk over each other.
static PLAYING: Mutex<()> = Mutex::new(());
/// Each alert stays short: the next one may be waiting behind it.
const MOST_CHARS: usize = 200;
const MOST_ITEMS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Voice,
    Sound,
    /// A list finished loading, spoken (issue #17).
    ListVoice,
    /// A list finished loading, chimed (issue #17).
    ListSound,
}

fn marker(kind: Kind) -> Option<PathBuf> {
    let name = match kind {
        Kind::Voice => "alerts-voice-on",
        Kind::Sound => "alerts-sound-on",
        Kind::ListVoice => "alerts-list-voice-on",
        Kind::ListSound => "alerts-list-sound-on",
    };
    data_dir().map(|dir| dir.join(name))
}

pub fn enabled(kind: Kind) -> bool {
    marker(kind).is_some_and(|path| path.is_file())
}

pub fn any_enabled() -> bool {
    enabled(Kind::Voice) || enabled(Kind::Sound)
}

pub fn any_list_enabled() -> bool {
    enabled(Kind::ListVoice) || enabled(Kind::ListSound)
}

fn items(count: usize) -> String {
    if count == 1 {
        "1 item".to_owned()
    } else {
        format!("{count} items")
    }
}

/// The words for a list that just finished loading: its name and how much
/// arrived, so nobody has to poll the status bar to know it is ready (#17).
pub fn loaded(title: &str, total: usize, added: usize, append: bool) -> String {
    if append {
        let fresh = if added == 1 {
            "1 new item".to_owned()
        } else {
            format!("{added} new items")
        };
        format!("{title}: {fresh} loaded. {} total.", items(total))
    } else if total == 0 {
        format!("{title}: no items to show.")
    } else {
        format!("{title} loaded. {}.", items(total))
    }
}

pub fn set_enabled(kind: Kind, on: bool) -> Result<(), String> {
    let path = marker(kind).ok_or("could not find BlindFB's data folder")?;
    if on {
        let dir = path.parent().ok_or("could not find BlindFB's data folder")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("could not save the setting: {e}"))?;
        std::fs::write(&path, "").map_err(|e| format!("could not save the setting: {e}"))?;
    } else if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| format!("could not save the setting: {e}"))?;
    }
    Ok(())
}

/// What has already been seen from one source. The first reading only takes
/// note of what is there, so turning alerts on never reads out a backlog.
#[derive(Default)]
pub struct Watch {
    seen: HashSet<String>,
    ready: bool,
}

impl Watch {
    /// Takes `(identity, text)` pairs; returns the text of the ones not seen before.
    pub fn check(&mut self, items: Vec<(String, String)>) -> Vec<String> {
        let first = !std::mem::replace(&mut self.ready, true);
        let fresh: Vec<String> = items
            .into_iter()
            .filter(|(key, _)| self.seen.insert(key.clone()))
            .map(|(_, text)| text)
            .collect();
        if first { Vec::new() } else { fresh }
    }
}

/// Unread notifications. The time is left out of the identity: "2m" becomes "3m".
pub fn notifications(output: &str) -> Result<Vec<(String, String)>, String> {
    Ok(parse_notifications(output)?
        .iter()
        .filter(|record| record.field(1) == "!")
        .map(|record| {
            (format!("{}\t{}", record.field(3), record.field(2)), record.field(2).to_owned())
        })
        .collect())
}

/// Unread chats. Their identity holds the last message, so another message alerts again.
pub fn chats(output: &str) -> Vec<(String, String)> {
    parse_records(output)
        .iter()
        .filter(|record| record.fields.len() >= 5 && record.field(1).contains('!'))
        .map(|record| {
            let text = record.fields[4..].join(" ");
            (format!("{}\t{text}", record.field(3)), format!("{}: {text}", record.field(3)))
        })
        .collect()
}

/// The words to say for some new items. The fixed start also means the text
/// can never be taken for a command-line option by a speech program.
pub fn spoken(new: &[String]) -> String {
    let shown: Vec<String> =
        new.iter().take(MOST_ITEMS).map(|text| text.chars().take(MOST_CHARS).collect()).collect();
    let more = new.len().saturating_sub(MOST_ITEMS);
    let tail = if more > 0 { format!(". And {more} more") } else { String::new() };
    format!("Facebook: {}{tail}.", shown.join(". "))
}

/// Play the sound and say the words, on their own thread. A system without the
/// tools stays silent: an alert must never get in the way of using the app.
pub fn play(sound: bool, voice: Option<String>) {
    std::thread::spawn(move || {
        let _turn = PLAYING.lock();
        run(sound, voice.as_deref());
    });
}

#[cfg(windows)]
fn run(sound: bool, voice: Option<&str>) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // The words travel in the environment, so they are never read as script.
    let mut script = String::new();
    if sound {
        script.push_str(
            r#"(New-Object Media.SoundPlayer "$env:windir\Media\Windows Notify Messaging.wav").PlaySync();"#,
        );
    }
    if voice.is_some() {
        script.push_str(
            "Add-Type -AssemblyName System.Speech;(New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak($env:BLINDFB_ALERT)",
        );
    }
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env("BLINDFB_ALERT", voice.unwrap_or(""))
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(target_os = "macos")]
fn run(sound: bool, voice: Option<&str>) {
    if sound {
        let _ = Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").status();
    }
    if let Some(text) = voice {
        let _ = Command::new("say").arg(text).status();
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run(sound: bool, voice: Option<&str>) {
    if sound {
        let _ = Command::new("canberra-gtk-play").args(["-i", "message-new-instant"]).status();
    }
    if let Some(text) = voice {
        let _ = Command::new("spd-say").args(["-w", text]).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(names: &[&str]) -> Vec<(String, String)> {
        names.iter().map(|name| ((*name).to_owned(), format!("text {name}"))).collect()
    }

    #[test]
    fn first_reading_is_a_baseline_and_only_new_items_alert() {
        let mut watch = Watch::default();
        assert!(watch.check(items(&["a", "b"])).is_empty());
        assert!(watch.check(items(&["a", "b"])).is_empty());
        assert_eq!(watch.check(items(&["c", "a"])), ["text c"]);
        assert!(watch.check(items(&["c"])).is_empty());
    }

    #[test]
    fn an_empty_first_reading_still_counts_as_the_baseline() {
        let mut watch = Watch::default();
        assert!(watch.check(Vec::new()).is_empty());
        assert_eq!(watch.check(items(&["a"])), ["text a"]);
    }

    #[test]
    fn only_unread_notifications_count_and_time_is_not_identity() {
        let raw = |time: &str| {
            format!(
                r#"{{"rows":[["{time}","!","Ann commented",{{"v":"/a/posts/1"}}],["1h","","Old news",{{"v":"/a/posts/2"}}]]}}"#
            )
        };
        let found = notifications(&raw("2m")).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, "Ann commented");
        assert_eq!(found, notifications(&raw("3m")).unwrap());
    }

    #[test]
    fn a_notification_the_backend_could_not_read_is_an_error_not_an_alert() {
        assert!(notifications(r#"{"err":"logged out"}"#).is_err());
    }

    #[test]
    fn only_unread_chats_count_and_a_new_message_alerts_again() {
        let old = "c1\t!\t1m\tBob\thi there\nc2\t\t1h\tSue\tseen already\n";
        let new = "c1\t!\tnow\tBob\tare you there\n";
        let mut watch = Watch::default();
        assert_eq!(chats(old).len(), 1);
        assert_eq!(chats(old)[0].1, "Bob: hi there");
        assert!(watch.check(chats(old)).is_empty());
        assert_eq!(watch.check(chats(new)), ["Bob: are you there"]);
    }

    #[test]
    fn group_chat_flags_still_count_as_unread() {
        assert_eq!(chats("c1\t!g\t1m\tTeam\thello\n").len(), 1);
    }

    #[test]
    fn a_loaded_list_is_named_with_its_size() {
        assert_eq!(loaded("News feed", 25, 25, false), "News feed loaded. 25 items.");
        assert_eq!(loaded("Chats", 1, 1, false), "Chats loaded. 1 item.");
        assert_eq!(loaded("Friends", 0, 0, false), "Friends: no items to show.");
        assert_eq!(
            loaded("News feed", 30, 5, true),
            "News feed: 5 new items loaded. 30 items total."
        );
        assert_eq!(
            loaded("Chats", 11, 1, true),
            "Chats: 1 new item loaded. 11 items total."
        );
    }

    #[test]
    fn spoken_text_is_short_and_counts_the_rest() {
        assert_eq!(spoken(&["one".to_owned()]), "Facebook: one.");
        let many: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(spoken(&many), "Facebook: a. b. c. And 2 more.");
        let long = spoken(&["x".repeat(500)]);
        assert!(long.chars().count() < 220);
    }
}
