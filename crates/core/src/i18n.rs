//! Interface language: BlindFB's own words in English or French, through patois.
//!
//! Text the user sees is written in English in the code and looked up in the
//! catalog `app/locale/<lang>/LC_MESSAGES/blindfb.po`:
//! - `t("…")` for plain text,
//! - `tf!("Loading {view}…", view = title)` for text with named placeholders
//!   (patois itself does not interpolate),
//! - `ntf!("{n} item", "{n} items", count)` for counts; `{n}` is the count.
//!
//! Never translate text that BlindFB compares or parses: the backend's status
//! words ("logged in", "still restoring"…), reaction and search keywords, or
//! raw chat lines.

use std::path::PathBuf;

pub use patois::{nt, t};

// The extractor (xgettext) only sees unqualified `tf!(…)` calls.
use crate::tf;

/// The catalog domain every crate falls back to.
pub const DOMAIN: &str = "blindfb";

/// The language the user chose in Help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Follow the system: French when the system is in French, English otherwise.
    System,
    English,
    French,
}

impl Language {
    fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en",
            Self::French => "fr",
        }
    }

    fn from_code(code: &str) -> Self {
        match code.trim() {
            "en" => Self::English,
            "fr" => Self::French,
            _ => Self::System,
        }
    }

    /// The patois locale this choice activates.
    pub fn locale(self) -> String {
        match self {
            Self::System => patois::system_locale(),
            Self::English => "en".to_owned(),
            Self::French => "fr".to_owned(),
        }
    }
}

fn setting_path() -> Option<PathBuf> {
    crate::backend::data_dir().map(|dir| dir.join("language"))
}

/// The saved choice; the system language when nothing was saved.
pub fn saved() -> Language {
    setting_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or(Language::System, |code| Language::from_code(&code))
}

/// Remember the choice for the next start.
pub fn save(language: Language) -> Result<(), String> {
    let path = setting_path().ok_or_else(|| t("could not find BlindFB's data folder"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| tf!("could not save the setting: {error}", error = e))?;
    }
    std::fs::write(&path, language.code())
        .map_err(|e| tf!("could not save the setting: {error}", error = e))
}

/// Activate a language for every later `t!`. Call once at startup, before any window.
pub fn apply(language: Language) {
    patois::set_default_domain(DOMAIN);
    patois::set_locale(&language.locale());
}

/// Translate `msgid` and fill its `{name}` placeholders.
/// Placeholders are read from the translated text only, in one pass, so a value
/// that itself contains "{name}" (a post's text, say) is never filled in again.
#[doc(hidden)]
pub fn __fill(text: String, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let filled = after.find('}').and_then(|close| {
            let name = &after[..close];
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (value, close))
        });
        match filled {
            Some((value, close)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `t!` with named `{placeholders}`: `tf!("Chat with {name}", name = who)`.
#[macro_export]
macro_rules! tf {
    ($msgid:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::__fill(
            ::patois::t($msgid),
            &[$((stringify!($name), ::std::string::ToString::to_string(&$value))),*],
        )
    };
}

/// A count with singular and plural forms; `{n}` is the count, other names as in `tf!`.
#[macro_export]
macro_rules! ntf {
    ($singular:literal, $plural:literal, $n:expr $(, $name:ident = $value:expr)* $(,)?) => {{
        let n = $n;
        $crate::i18n::__fill(
            ::patois::nt($singular, $plural, n as u64),
            &[("n", ::std::string::ToString::to_string(&n)) $(, (stringify!($name), ::std::string::ToString::to_string(&$value)))*],
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_by_name() {
        assert_eq!(
            __fill("Chat with {name}, {name}".to_owned(), &[("name", "Ann".to_owned())]),
            "Chat with Ann, Ann"
        );
        assert_eq!(crate::tf!("Posts by {who}", who = "Bo"), "Posts by Bo");
        // A value is never filled in again, whatever it contains.
        assert_eq!(
            __fill(
                "{name}: {text}".to_owned(),
                &[("name", "{text}".to_owned()), ("text", "hi {name}".to_owned())]
            ),
            "{text}: hi {name}"
        );
        assert_eq!(__fill("{unknown} {".to_owned(), &[]), "{unknown} {");
        assert_eq!(crate::ntf!("{n} item", "{n} items", 1), "1 item");
        assert_eq!(crate::ntf!("{n} item", "{n} items", 3), "3 items");
    }

    #[test]
    fn saved_codes_round_trip() {
        for language in [Language::System, Language::English, Language::French] {
            assert_eq!(Language::from_code(language.code()), language);
        }
        assert_eq!(Language::from_code("garbage"), Language::System);
    }
}
