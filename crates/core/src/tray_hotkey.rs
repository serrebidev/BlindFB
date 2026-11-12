//! Show/hide hotkey: the system-wide shortcut that hides BlindFB to the
//! tray and brings it back. Default Alt+Ctrl+F; the user can pick another
//! preset or turn it off. Saved in the data dir like the language choice.

use std::path::PathBuf;

use patois::t;

use crate::backend::data_dir;
use crate::tf;

const FILE: &str = "tray-hotkey";

/// The hotkey choices. `AltCtrlF` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// No hotkey: only the tray icon and menus toggle.
    Off,
    /// Alt+Ctrl+F (default).
    AltCtrlF,
    /// Ctrl+T (the old default).
    CtrlT,
    /// Alt+T.
    AltT,
    /// Ctrl+Alt+T.
    CtrlAltT,
    /// Ctrl+Shift+T.
    CtrlShiftT,
    /// Alt+F9.
    AltF9,
}

impl Hotkey {
    /// Stored code.
    pub fn code(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::AltCtrlF => "alt-ctrl-f",
            Self::CtrlT => "ctrl-t",
            Self::AltT => "alt-t",
            Self::CtrlAltT => "ctrl-alt-t",
            Self::CtrlShiftT => "ctrl-shift-t",
            Self::AltF9 => "alt-f9",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "off" => Some(Self::Off),
            "alt-ctrl-f" => Some(Self::AltCtrlF),
            "ctrl-t" => Some(Self::CtrlT),
            "alt-t" => Some(Self::AltT),
            "ctrl-alt-t" => Some(Self::CtrlAltT),
            "ctrl-shift-t" => Some(Self::CtrlShiftT),
            "alt-f9" => Some(Self::AltF9),
            _ => None,
        }
    }

    /// Name for menus, accelerators and dialogs. `None` when off.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::AltCtrlF => Some("Alt+Ctrl+F"),
            Self::CtrlT => Some("Ctrl+T"),
            Self::AltT => Some("Alt+T"),
            Self::CtrlAltT => Some("Ctrl+Alt+T"),
            Self::CtrlShiftT => Some("Ctrl+Shift+T"),
            Self::AltF9 => Some("Alt+F9"),
        }
    }

    /// Dialog entries in pick order, with the label to show.
    pub fn choices() -> [(Self, String); 7] {
        [
            (Self::AltCtrlF, "Alt+Ctrl+F (default)".to_owned()),
            (Self::CtrlT, "Ctrl+T".to_owned()),
            (Self::AltT, "Alt+T".to_owned()),
            (Self::CtrlAltT, "Ctrl+Alt+T".to_owned()),
            (Self::CtrlShiftT, "Ctrl+Shift+T".to_owned()),
            (Self::AltF9, "Alt+F9".to_owned()),
            (Self::Off, t("Off (no hotkey)").to_owned()),
        ]
    }

    /// Win32 RegisterHotKey (modifiers, virtual-key); `None` when off.
    /// MOD_NOREPEAT stops holding the key from strobing the window.
    pub fn win_spec(self) -> Option<(u32, u32)> {
        const MOD_ALT: u32 = 0x0001;
        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;
        const MOD_NOREPEAT: u32 = 0x4000;
        const VK_F: u32 = 0x46;
        const VK_T: u32 = 0x54;
        const VK_F9: u32 = 0x78;
        match self {
            Self::Off => None,
            Self::AltCtrlF => Some((MOD_ALT | MOD_CONTROL | MOD_NOREPEAT, VK_F)),
            Self::CtrlT => Some((MOD_CONTROL | MOD_NOREPEAT, VK_T)),
            Self::AltT => Some((MOD_ALT | MOD_NOREPEAT, VK_T)),
            Self::CtrlAltT => Some((MOD_ALT | MOD_CONTROL | MOD_NOREPEAT, VK_T)),
            Self::CtrlShiftT => Some((MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_T)),
            Self::AltF9 => Some((MOD_ALT | MOD_NOREPEAT, VK_F9)),
        }
    }
}

fn path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(FILE))
}

/// The saved hotkey; Alt+Ctrl+F when nothing was saved.
pub fn saved() -> Hotkey {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| Hotkey::from_code(&text))
        .unwrap_or(Hotkey::AltCtrlF)
}

/// Remember the choice for the next start; the Windows hotkey thread
/// also picks it up live (it re-reads the file on change).
pub fn save(hotkey: Hotkey) -> Result<(), String> {
    let p = path().ok_or_else(|| t("could not find BlindFB's data folder"))?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| tf!("could not save the setting: {error}", error = e))?;
    }
    std::fs::write(&p, hotkey.code())
        .map_err(|e| tf!("could not save the setting: {error}", error = e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_codes_round_trip() {
        for hotkey in [
            Hotkey::Off,
            Hotkey::AltCtrlF,
            Hotkey::CtrlT,
            Hotkey::AltT,
            Hotkey::CtrlAltT,
            Hotkey::CtrlShiftT,
            Hotkey::AltF9,
        ] {
            assert_eq!(Hotkey::from_code(hotkey.code()), Some(hotkey));
            assert!(Hotkey::choices()
                .iter()
                .any(|(choice, _)| *choice == hotkey));
        }
        assert_eq!(Hotkey::from_code("garbage"), None);
    }

    #[test]
    fn default_is_alt_ctrl_f() {
        assert_eq!(Hotkey::AltCtrlF.label(), Some("Alt+Ctrl+F"));
        // MOD_ALT | MOD_CONTROL | MOD_NOREPEAT, VK_F.
        assert_eq!(Hotkey::AltCtrlF.win_spec(), Some((0x4003, 0x46)));
        assert_eq!(Hotkey::CtrlT.win_spec(), Some((0x4002, 0x54)));
        assert_eq!(Hotkey::Off.win_spec(), None);
        assert_eq!(Hotkey::Off.label(), None);
        // Every preset except Off has a spec; nothing else does.
        for (choice, _) in Hotkey::choices() {
            assert_eq!(choice.win_spec().is_some(), choice != Hotkey::Off);
        }
    }

    #[test]
    fn choices_are_unique_and_complete() {
        let choices = Hotkey::choices();
        assert_eq!(choices.len(), 7);
        let mut codes: Vec<&str> = choices.iter().map(|(h, _)| h.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), choices.len());
    }
}
