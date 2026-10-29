//! Recommended reels and exact multi-reel notifications, using isolated read-only pages.
use crate::{Fb, Record, posts};
use serde_json::{Value, json};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReelSource {
    Recommended,
    Notification { target: String, author: String, urls: Vec<String>, account: Option<String> },
}

impl ReelSource {
    pub fn title(&self) -> String {
        match self {
            Self::Recommended => "Reels".to_owned(),
            Self::Notification { author, .. } => format!("Reels from {author}"),
        }
    }
}

pub fn notification_source(record: &Record) -> Option<ReelSource> {
    let text = record.field(2);
    if !text.to_lowercase().contains(" new reels") || !is_reel(record.field(3)) {
        return None;
    }
    let metadata = serde_json::from_str::<Value>(record.field(4)).unwrap_or(Value::Null);
    Some(ReelSource::Notification {
        target: crate::fb::web_url(record.field(3)),
        author: text.split(" posted ").next().unwrap_or("this account").to_owned(),
        urls: metadata["urls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().filter(|url| is_reel(url)).map(str::to_owned))
            .take(20)
            .collect(),
        account: metadata["account"].as_str().filter(|url| account_url(url)).map(str::to_owned),
    })
}

fn is_reel(target: &str) -> bool {
    let url = crate::fb::web_url(target);
    posts::validate_target(&url).is_ok()
        && url.split(['?', '#']).next().is_some_and(|path| {
            path.split_once("/reel/").is_some_and(|(_, id)| {
                let id = id.trim_end_matches('/');
                !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

pub fn parse(output: &str) -> Result<(Vec<Record>, Option<String>), String> {
    let value: Value =
        serde_json::from_str(output).map_err(|_| "Facebook returned an unreadable reels list")?;
    let rows = value["rows"].as_array().ok_or("Facebook returned no reels list")?;
    let mut seen = std::collections::HashSet::new();
    let mut records = Vec::new();
    for row in rows {
        let url = row["url"]
            .as_str()
            .filter(|url| is_reel(url))
            .ok_or("Facebook returned an invalid reel link")?;
        if !seen.insert(url.to_owned()) {
            continue;
        }
        let fields = vec![
            url.to_owned(),
            row["author"].as_str().unwrap_or("").to_owned(),
            String::new(),
            String::new(),
            row["text"].as_str().unwrap_or("Reel").to_owned(),
        ];
        records.push(Record { raw: fields.join("\t"), fields });
    }
    let notice = value["notice"].as_str().filter(|text| !text.is_empty()).map(str::to_owned);
    Ok((records, notice))
}

fn eval(fb: &Fb, page: &str, script: &str) -> Result<String, String> {
    fb.run(&["page", page, "eval", script])
}

fn initialize(fb: &Fb, page: &str) -> Result<(), String> {
    eval(fb, page, include_str!("reels.js")).map(|_| ())
}

fn account_url(url: &str) -> bool {
    crate::media::is_facebook_url(url)
        && (url
            .split('?')
            .next()
            .is_some_and(|path| path.trim_end_matches('/').ends_with("/reels"))
            || (url.contains("/profile.php?") && url.contains("sk=reels_tab")))
}

static PAGES: Mutex<Vec<(ReelSource, String)>> = Mutex::new(Vec::new());

fn source_id(source: &ReelSource) -> u64 {
    let mut hash = DefaultHasher::new();
    source.hash(&mut hash);
    hash.finish()
}

pub fn page_name(source: &ReelSource) -> String {
    format!("bf-r-{}-{:016x}", std::process::id(), source_id(source))
}

pub fn close_all(fb: &Fb) {
    let pages = PAGES.lock().map(|mut pages| std::mem::take(&mut *pages)).unwrap_or_default();
    for (_, page) in pages {
        let _ = fb.run(&["page", &page, "close"]);
    }
}

fn owner_reels(url: &str) -> Option<String> {
    if account_url(url) {
        return Some(url.to_owned());
    }
    if !crate::media::is_facebook_url(url) {
        return None;
    }
    let rest = ["https://www.facebook.com/", "https://facebook.com/", "https://m.facebook.com/"]
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = rest.split(['?', '#']).next()?.trim_end_matches('/');
    if path == "profile.php" {
        let id = rest.split_once('?')?.1.split('&').find_map(|pair| pair.strip_prefix("id="))?;
        return (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| format!("https://www.facebook.com/profile.php?id={id}&sk=reels_tab"));
    }
    (!path.is_empty()
        && !path.contains('/')
        && !matches!(path, "reel" | "watch" | "groups" | "notifications"))
    .then(|| format!("https://www.facebook.com/{path}/reels/"))
}

/// Blocking; the UI runs this independently of comment and reaction jobs.
pub fn load(fb: &Fb, source: &ReelSource, append: bool) -> Result<String, String> {
    // Serialize browsing jobs, with a separate cursor for each cached source.
    static BROWSE: Mutex<()> = Mutex::new(());
    let _browse = BROWSE.lock().map_err(|_| "Reel loading was interrupted. Refresh to retry.")?;
    let page = page_name(source);
    let key = source_id(source).to_string();
    let evicted = {
        let mut pages = PAGES.lock().map_err(|_| "Reel loading was interrupted")?;
        let existing = pages.iter().position(|(saved, _)| saved == source);
        if append && existing.is_none() {
            return Err("This older reels list expired. Refresh it to load more.".to_owned());
        }
        if let Some(index) = existing {
            pages.remove(index);
        }
        // Leave room for Groups, the player and its temporary preview page in the backend's six-page limit.
        let evicted = if pages.len() >= 3 {
            let index = pages
                .iter()
                .position(|(saved, _)| !matches!(saved, ReelSource::Recommended))
                .unwrap_or(0);
            Some(pages.remove(index).1)
        } else {
            None
        };
        pages.push((source.clone(), page.clone()));
        evicted
    };
    if let Some(page) = evicted {
        let _ = fb.run(&["page", &page, "close"]);
    }
    if append {
        return eval(fb, &page, &format!("window.__blindfbReels.more({},6)", json!(key)));
    }
    let (url, mode, notice) = match source {
        ReelSource::Recommended => {
            ("https://www.facebook.com/reel/".to_owned(), "recommended", String::new())
        }
        ReelSource::Notification { target, author, urls: saved_urls, account: saved_account } => {
            if !is_reel(target) {
                return Err("This notification has no valid reel link".to_owned());
            }
            fb.run(&["page", &page, "open", "https://www.facebook.com/notifications/"])?;
            initialize(fb, &page)?;
            let metadata: Value = if saved_urls.len() > 1 {
                json!({"urls":saved_urls,"account":saved_account})
            } else {
                serde_json::from_str(&eval(
                    fb,
                    &page,
                    &format!("window.__blindfbReels.notification({})", json!(target)),
                )?)
                .map_err(|_| "Could not read the notification's reels")?
            };
            let urls: Vec<_> = metadata["urls"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|url| is_reel(url))
                .take(20)
                .collect();
            if urls.len() > 1
                && urls.iter().any(|url| {
                    url.trim_end_matches('/')
                        == target.split('?').next().unwrap_or(target).trim_end_matches('/')
                })
            {
                eval(
                    fb,
                    &page,
                    &format!("window.__blindfbReels.initialize({},'exact')", json!(key)),
                )?;
                let preview = format!("blindfb-reel-preview-{}", std::process::id());
                let mut rows = Vec::new();
                for (index, url) in urls.iter().enumerate() {
                    let detail = posts::load(fb, &preview, url)
                        .and_then(|output| posts::parse_detail(&output));
                    let _ = posts::close(fb, &preview);
                    rows.push(match detail {
                        Ok(detail) => json!({"url":url,"author":detail.author,"text":if detail.text.is_empty() {format!("Reel {}",index+1)} else {detail.text}}),
                        Err(_) => json!({"url":url,"author":author,"text":format!("Reel {}. Preview unavailable; open this reel to retry.",index+1)}),
                    });
                }
                return Ok(json!({"rows":rows}).to_string());
            }
            let mut account = metadata["account"]
                .as_str()
                .filter(|url| account_url(url))
                .map(str::to_owned)
                .or_else(|| saved_account.as_ref().filter(|url| account_url(url)).cloned());
            if account.is_none() {
                let preview = format!("blindfb-reel-preview-{}", std::process::id());
                let detail = posts::load(fb, &preview, target)
                    .and_then(|output| posts::parse_detail(&output));
                let _ = posts::close(fb, &preview);
                let detail = detail?;
                account = detail.links.iter().find_map(|link| {
                    if account_url(&link.url)
                        || link.label == detail.author
                        || link.label.to_lowercase().contains("owner profile")
                    {
                        owner_reels(&link.url)
                    } else {
                        None
                    }
                });
            }
            (
                account
                    .ok_or("Facebook did not expose the other reels or the account's reels page")?,
                "account",
                format!("Facebook did not expose the exact set. Showing {author}'s reels instead."),
            )
        }
    };
    fb.run(&["page", &page, "open", &url])?;
    initialize(fb, &page)?;
    eval(
        fb,
        &page,
        &format!(
            "window.__blindfbReels.initialize({},{},{})",
            json!(key),
            json!(mode),
            json!(notice)
        ),
    )?;
    eval(fb, &page, &format!("window.__blindfbReels.more({},6)", json!(key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouped_reel_notification_keeps_its_identity() {
        let record = Record {
            fields: vec![
                "3h".into(),
                "!".into(),
                "The Verge posted 3 new reels, including a caption".into(),
                "/reel/123/?notif_id=456".into(),
            ],
            raw: String::new(),
        };
        assert_eq!(
            notification_source(&record),
            Some(ReelSource::Notification {
                target: "https://www.facebook.com/reel/123/?notif_id=456".into(),
                author: "The Verge".into(),
                urls: Vec::new(),
                account: None
            })
        );
        let mut bad = record;
        bad.fields[3] = "https://www.facebook.com.evil/reel/123/".into();
        assert!(notification_source(&bad).is_none());
    }
    #[test]
    fn notification_keeps_exact_ids_after_facebook_drops_the_original_row() {
        let rows = crate::fb::parse_notifications(r#"{"rows":[["3h","!","The Verge posted 3 new reels",{"v":"/reel/1/?notif_id=7"},{"urls":["https://www.facebook.com/reel/1/","https://www.facebook.com/reel/2/","https://www.facebook.com/reel/3/"],"account":"https://www.facebook.com/verge/reels/"}]]}"#).unwrap();
        let source = notification_source(&rows[0]).unwrap();
        let ReelSource::Notification { urls, account, .. } = source else {
            panic!("expected a notification source")
        };
        assert_eq!(urls.len(), 3);
        assert_eq!(account.as_deref(), Some("https://www.facebook.com/verge/reels/"));
    }
    #[test]
    fn reel_rows_keep_whole_captions_and_reject_unsafe_links() {
        let text = "Full caption\n".repeat(100);
        let data = json!({"rows":[{"url":"https://www.facebook.com/reel/123/","author":"A","text":text}],"notice":"Account fallback"});
        let (rows, notice) = parse(&data.to_string()).unwrap();
        assert_eq!(rows[0].field(4), text);
        assert_eq!(notice.as_deref(), Some("Account fallback"));
        assert!(parse(r#"{"rows":[{"url":"file:///secret"}]}"#).is_err());
    }
    #[test]
    fn account_fallback_uses_the_owner_profile() {
        assert_eq!(
            owner_reels("https://www.facebook.com/verge?tracking=1").as_deref(),
            Some("https://www.facebook.com/verge/reels/")
        );
        assert_eq!(
            owner_reels("https://www.facebook.com/profile.php?id=123").as_deref(),
            Some("https://www.facebook.com/profile.php?id=123&sk=reels_tab")
        );
        assert!(owner_reels("https://facebook.com.evil/verge").is_none());
        assert!(owner_reels("https://www.facebook.com/reel/123/").is_none());
    }
    #[test]
    fn cached_sources_have_distinct_bounded_page_names() {
        let recommended = page_name(&ReelSource::Recommended);
        let note = page_name(&ReelSource::Notification {
            target: "https://www.facebook.com/reel/1/?notif_id=2".into(),
            author: "A".into(),
            urls: Vec::new(),
            account: None,
        });
        assert_ne!(recommended, note);
        assert_eq!(recommended, page_name(&ReelSource::Recommended));
        assert!(note.len() <= 32);
        assert!(note.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));
    }
}
