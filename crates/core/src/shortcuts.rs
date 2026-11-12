//! Customizable keyboard shortcuts. Each command has a settings key and a
//! default; the user's changes live in `shortcuts.txt` in the data dir, one
//! `key=Shortcut` line each (`key=` turns the shortcut off). Shortcuts are
//! written the way wx menus read accelerators: `Ctrl+Shift+C`, `F5`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::backend::data_dir;

const FILE: &str = "shortcuts.txt";

/// Keys a shortcut can use, in the order the picker lists them.
pub const KEYS: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "F1",
    "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "Enter", "Space",
    "Delete", "Insert", "Home", "End", "PgUp", "PgDn", "/", ",", ".", ";", "[", "]", "-", "=",
];

/// The user's changes, loaded once and kept current by [`set`].
static OVERRIDES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(FILE))
}

fn parse_file(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

fn with_overrides<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
    let mut guard = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(|| {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| parse_file(&text))
            .unwrap_or_default()
    });
    f(map)
}

fn resolve(overrides: &HashMap<String, String>, key: &str, default: &str) -> Option<String> {
    let value = overrides.get(key).map_or(default, String::as_str);
    (!value.is_empty()).then(|| value.to_owned())
}

/// The shortcut `key` runs with: the user's choice, else `default`. `None` when off.
pub fn get(key: &str, default: &str) -> Option<String> {
    with_overrides(|map| resolve(map, key, default))
}

/// Remember `shortcut` (`None` = off) for `key`. Choosing the default drops the line.
pub fn set(key: &str, default: &str, shortcut: Option<&str>) -> Result<(), String> {
    let value = shortcut.unwrap_or("");
    with_overrides(|map| {
        if value == default {
            map.remove(key);
        } else {
            map.insert(key.to_owned(), value.to_owned());
        }
        let mut lines: Vec<String> = map.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        lines.sort();
        let path = path().ok_or_else(|| "no data folder".to_owned())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, lines.concat()).map_err(|e| e.to_string())
    })
}

/// A shortcut split into its parts: Ctrl, Alt, Shift and the key name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

impl Parts {
    pub fn parse(shortcut: &str) -> Option<Self> {
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        let mut key = None;
        // "Ctrl+-" ends in an empty part after the split, so the key is taken whole.
        let mut rest = shortcut.trim();
        while let Some((head, tail)) =
            rest.split_once('+').filter(|(h, t)| !h.is_empty() && !t.is_empty())
        {
            match head.to_ascii_lowercase().as_str() {
                "ctrl" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => return None,
            }
            rest = tail;
        }
        if !rest.is_empty() {
            key = KEYS.iter().find(|k| k.eq_ignore_ascii_case(rest)).map(|k| (*k).to_owned());
        }
        Some(Self { ctrl, alt, shift, key: key? })
    }

    /// `Ctrl+Alt+Shift+Key`, the order wx and the menus use.
    pub fn label(&self) -> String {
        let mut out = String::new();
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.alt, "Alt+"), (self.shift, "Shift+")] {
            if on {
                out.push_str(name);
            }
        }
        out.push_str(&self.key);
        out
    }

    /// Letters, digits, space and punctuation type text, so they need Ctrl or Alt.
    pub fn types_text(&self) -> bool {
        let printable = self.key.len() == 1 || self.key == "Space";
        printable && !self.ctrl && !self.alt
    }
}

/// The key name for a wx key code, as [`KEYS`] spells it.
pub fn key_name(code: i32) -> Option<&'static str> {
    // wx key codes: letters arrive as their ASCII code, F1 = 340, and so on.
    const NAMED: &[(i32, &str)] = &[
        (13, "Enter"),
        (370, "Enter"), // numpad Enter
        (32, "Space"),
        (127, "Delete"),
        (322, "Insert"),
        (313, "Home"),
        (312, "End"),
        (366, "PgUp"),
        (367, "PgDn"),
    ];
    if let Some((_, name)) = NAMED.iter().find(|(c, _)| *c == code) {
        return Some(name);
    }
    if (340..=351).contains(&code) {
        return KEYS.iter().find(|k| **k == format!("F{}", code - 339)).copied();
    }
    let c = u8::try_from(code).ok()?.to_ascii_uppercase() as char;
    KEYS.iter().find(|k| k.len() == 1 && k.starts_with(c)).copied()
}

/// Whether a key press is `shortcut`.
pub fn matches(shortcut: Option<&str>, code: i32, ctrl: bool, alt: bool, shift: bool) -> bool {
    let (Some(want), Some(key)) = (shortcut.and_then(Parts::parse), key_name(code)) else {
        return false;
    };
    want == Parts { ctrl, alt, shift, key: key.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_parse_and_print_the_wx_way() {
        let p = Parts::parse("shift+ctrl+c").unwrap();
        assert_eq!(p.label(), "Ctrl+Shift+C");
        assert_eq!(Parts::parse("Ctrl+-").unwrap().key, "-");
        assert_eq!(Parts::parse("F5").unwrap().label(), "F5");
        assert!(Parts::parse("Ctrl+Banana").is_none());
        assert!(Parts::parse("Hyper+C").is_none());
        assert!(Parts::parse("").is_none());
    }

    #[test]
    fn plain_letters_would_type_text() {
        assert!(Parts::parse("C").unwrap().types_text());
        assert!(Parts::parse("Shift+/").unwrap().types_text());
        assert!(!Parts::parse("Alt+C").unwrap().types_text());
        assert!(!Parts::parse("F2").unwrap().types_text());
    }

    #[test]
    fn key_presses_match_their_shortcut() {
        assert!(matches(Some("Ctrl+Shift+C"), 99, true, false, true));
        assert!(matches(Some("Ctrl+Shift+C"), 67, true, false, true));
        assert!(!matches(Some("Ctrl+Shift+C"), 67, true, false, false));
        assert!(matches(Some("Ctrl+Enter"), 370, true, false, false));
        assert!(matches(Some("F5"), 344, false, false, false));
        assert!(!matches(None, 67, true, false, false));
    }

    #[test]
    fn overrides_win_and_empty_means_off() {
        let map = parse_file("copy_all=Alt+Shift+C\nexit=\n\nbad line\n");
        assert_eq!(resolve(&map, "copy_all", "Ctrl+Shift+C").as_deref(), Some("Alt+Shift+C"));
        assert_eq!(resolve(&map, "exit", "Ctrl+Q"), None);
        assert_eq!(resolve(&map, "new_post", "Ctrl+N").as_deref(), Some("Ctrl+N"));
        assert_eq!(resolve(&map, "ocr", ""), None);
    }
}
