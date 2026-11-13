//! Type-ahead for list boxes: letters typed within a second of each other
//! build a prefix ("brando"), and the selection moves to the first row that
//! starts with it. The Windows list box itself matches only the first letter.
//! Typing one letter again and again steps through the rows starting with it.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use wxdragon::prelude::*;

/// Explorer's own type-ahead pause.
const PAUSE: Duration = Duration::from_millis(1000);

#[derive(Default)]
struct Typed {
    prefix: String,
    at: Option<Instant>,
}

impl Typed {
    /// Add `c`; true when it starts a new prefix (the pause ran out).
    fn push(&mut self, c: char, now: Instant) -> bool {
        let fresh = self.at.is_none_or(|at| now.duration_since(at) > PAUSE);
        if fresh {
            self.prefix.clear();
        }
        self.prefix.extend(c.to_lowercase());
        self.at = Some(now);
        fresh
    }
}

/// The row to select for `prefix`, searching from `current` (or the row after
/// it, when `next`) and wrapping. A prefix of one repeated letter ("bbb")
/// cycles through the rows starting with that letter.
fn find(labels: &[String], current: Option<usize>, prefix: &str, next: bool) -> Option<usize> {
    if labels.is_empty() || prefix.is_empty() {
        return None;
    }
    let mut chars = prefix.chars();
    let first = chars.next()?;
    let (prefix, next) = if prefix.chars().count() > 1 && chars.all(|c| c == first) {
        (first.to_string(), true)
    } else {
        (prefix.to_owned(), next)
    };
    let start = match current {
        Some(i) if next => i + 1,
        Some(i) => i,
        None => 0,
    };
    (0..labels.len())
        .map(|offset| (start + offset) % labels.len())
        .find(|&i| labels[i].trim_start().to_lowercase().starts_with(&prefix))
}

/// Give `list` type-ahead. `selected` runs after a typed jump, since a
/// programmatic selection fires no selection event.
pub(super) fn attach(list: &ListBox, selected: impl Fn(u32) + 'static) {
    let typed = Rc::new(RefCell::new(Typed::default()));
    let target = *list;
    list.on_char(move |event| {
        let WindowEventData::Keyboard(key) = &event else {
            event.skip(true);
            return;
        };
        let c = key.get_unicode_key().and_then(|code| char::from_u32(code as u32));
        let Some(c) = c.filter(|c| !c.is_control()) else {
            event.skip(true);
            return;
        };
        if key.control_down() || key.alt_down() || key.meta_down() {
            event.skip(true);
            return;
        }
        let mut typed = typed.borrow_mut();
        let fresh = typed.push(c, Instant::now());
        // A space only joins a prefix already being typed ("brandon b").
        if c == ' ' && fresh {
            typed.prefix.clear();
            typed.at = None;
            event.skip(true);
            return;
        }
        event.skip(false);
        let labels: Vec<String> =
            (0..target.get_count()).map(|i| target.get_string(i).unwrap_or_default()).collect();
        let current = target.get_selection().map(|i| i as usize);
        if let Some(index) = find(&labels, current, &typed.prefix, fresh) {
            if current != Some(index) {
                target.set_selection(index as u32, true);
                selected(index as u32);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_typed_prefix_finds_the_full_name() {
        let list = rows(&["Brad Pitt", "Brenda Mayhle", "Brandon Bracey", "Daniel Roth"]);
        let mut typed = Typed::default();
        let now = Instant::now();
        let mut at = Some(3);
        for (i, c) in "brando".chars().enumerate() {
            let fresh = typed.push(c, now + Duration::from_millis(200 * i as u64));
            at = find(&list, at, &typed.prefix, fresh);
        }
        assert_eq!(at, Some(2));
        // After the pause a new letter starts over.
        let fresh = typed.push('d', now + Duration::from_secs(5));
        assert!(fresh);
        assert_eq!(find(&list, at, &typed.prefix, fresh), Some(3));
    }

    #[test]
    fn a_repeated_letter_cycles_and_extending_keeps_the_row() {
        let list = rows(&["Bob", "Brandon", "Daniel", "Bree"]);
        assert_eq!(find(&list, Some(0), "b", true), Some(1));
        assert_eq!(find(&list, Some(1), "bb", false), Some(3));
        assert_eq!(find(&list, Some(3), "bbb", false), Some(0));
        // "br" from Brandon stays on Brandon instead of skipping ahead.
        assert_eq!(find(&list, Some(1), "br", false), Some(1));
        assert_eq!(find(&list, Some(1), "zz", false), None);
    }
}
