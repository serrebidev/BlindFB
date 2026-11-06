//! Facebook's own words that the Rust side reads in backend output, per language.
//! The page-side table is `post_scripts/labels.js`; French values come from
//! `docs/fr-labels.md`. Facebook writes the apostrophe as ’ (U+2019).

/// Badge and status parts of a feed author line that are not a name.
pub const FEED_NOISE: &[&str] = &[
    "Online status indicator",
    "Active",
    "Verified account",
    "Suggested for you",
    "Shared with Public",
    "Follow",
    "View more comments",
    "Indicateur de statut en ligne",
    "En ligne",
    "Compte vérifié",
    "Suggestions pour vous",
    "Partagé avec Public",
    "Partagé avec : Public",
    "Suivre",
    "Voir plus de commentaires",
];

/// Words that join two names in one author line ("A follows B", "A est avec B").
pub const AUTHOR_JOINERS: &[&str] = &[
    "and", "follows", "is with", "is in", "is at", "et", "suit", "est avec", "est à",
];

/// The truncation marker Facebook appends to long feed text.
pub const SEE_MORE_SUFFIXES: &[&str] = &["… See more", "… En voir plus", "… Voir plus"];

/// A notification about several reels at once ("X posted 3 new reels"); French supposed.
pub const MANY_REELS: &[&str] = &[" new reels", " nouveaux reels", " nouvelles vidéos reels"];

/// The verb between the author and the rest of a reel notification.
pub const POSTED: &[&str] = &[" posted ", " a publié "];

/// The link to a reel's owner ("View owner profile", "Voir profil du propriétaire").
pub const OWNER_PROFILE: &[&str] = &["owner profile", "profil du propriétaire"];

/// An author part that is only one of Facebook's own badge words.
pub fn feed_noise(part: &str) -> bool {
    FEED_NOISE.contains(&part) || part.starts_with("Vous suivez ")
}

/// The Follow link Facebook glues to a page name ("Oh My Mag · Suivre").
pub const FOLLOW_SUFFIXES: &[&str] = &[" · Follow", " · Suivre"];

/// An author part without a trailing " · Follow" / " · Suivre".
pub fn without_follow(part: &str) -> &str {
    FOLLOW_SUFFIXES
        .iter()
        .find_map(|suffix| part.strip_suffix(suffix))
        .unwrap_or(part)
        .trim()
}

/// Feed text without Facebook's "… See more" marker, in any language.
pub fn without_see_more(text: &str) -> &str {
    SEE_MORE_SUFFIXES
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(text)
}

/// The author of a reel notification: the text before "posted".
pub fn reel_notification_author(text: &str) -> &str {
    POSTED
        .iter()
        .filter_map(|verb| text.split_once(verb).map(|(author, _)| author))
        .min_by_key(|author| author.len())
        .unwrap_or(text)
}

/// Whether a notification announces several reels at once.
pub fn many_reels(text: &str) -> bool {
    let text = text.to_lowercase();
    MANY_REELS.iter().any(|words| text.contains(words))
}

/// Whether a link label names the reel's owner profile.
pub fn owner_profile(label: &str) -> bool {
    let label = label.to_lowercase();
    OWNER_PROFILE.iter().any(|words| label.contains(words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_feed_badges_are_noise() {
        assert!(feed_noise("Compte vérifié"));
        assert!(feed_noise("Suivre"));
        assert!(feed_noise("Vous suivez JECK"));
        assert!(!feed_noise("Oh My Mag"));
    }

    #[test]
    fn see_more_markers_are_removed_in_each_language() {
        assert_eq!(
            without_see_more("On a un problème… En voir plus"),
            "On a un problème"
        );
        assert_eq!(without_see_more("Text… See more"), "Text");
        assert_eq!(without_see_more("No marker"), "No marker");
    }

    #[test]
    fn reel_notifications_read_in_french() {
        assert!(many_reels("JECK a publié 3 nouveaux reels."));
        assert!(many_reels("Ann posted 3 new reels."));
        assert_eq!(
            reel_notification_author("JECK a publié 3 nouveaux reels."),
            "JECK"
        );
        assert_eq!(reel_notification_author("Ann posted 3 new reels."), "Ann");
        assert!(owner_profile("JECK, voir le profil du propriétaire"));
        assert!(owner_profile("Voir profil du propriétaire"));
        assert!(owner_profile("View owner profile"));
    }
}
