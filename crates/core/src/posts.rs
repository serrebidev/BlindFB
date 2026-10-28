//! Isolated Facebook post readers and verified, thread-scoped comments.

use crate::Fb;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub time: String,
    pub text: String,
    pub parent_id: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostDetail {
    pub url: String,
    pub author: String,
    pub time: String,
    pub text: String,
    pub comments: Vec<Comment>,
}

fn string(value: &Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("Invalid post detail: {field} must be text"))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, String> {
    match &value[field] {
        Value::Null => Ok(None),
        Value::String(text) if !text.is_empty() => Ok(Some(text.clone())),
        _ => Err(format!("Invalid post detail: {field}")),
    }
}

pub fn parse_detail(output: &str) -> Result<PostDetail, String> {
    let value: Value = serde_json::from_str(output)
        .map_err(|_| "The post reader returned invalid JSON".to_owned())?;
    let url = string(&value, "url")?;
    validate_target(&url)?;
    let rows =
        value["comments"].as_array().ok_or("Invalid post detail: comments must be a list")?;
    let mut comments = Vec::with_capacity(rows.len());
    let mut ids = HashSet::new();
    for row in rows {
        let id = string(row, "id")?;
        if id.is_empty() || !ids.insert(id.clone()) {
            return Err("Invalid post detail: empty or duplicate comment identity".to_owned());
        }
        let url = optional_string(row, "url")?;
        if let Some(url) = &url {
            validate_target(url)?;
        }
        comments.push(Comment {
            id,
            author: string(row, "author")?,
            time: string(row, "time")?,
            text: string(row, "text")?,
            parent_id: optional_string(row, "parent_id")?,
            url,
        });
    }
    let by_id: HashMap<_, _> = comments.iter().map(|comment| (comment.id.as_str(), comment)).collect();
    let mut validated = HashSet::new();
    for comment in &comments {
        let mut seen = HashSet::new();
        let mut current = comment;
        while let Some(parent) = &current.parent_id {
            if validated.contains(current.id.as_str()) {
                break;
            }
            if !seen.insert(current.id.as_str()) {
                return Err("Invalid post detail: cyclic comment thread".to_owned());
            }
            current = by_id
                .get(parent.as_str())
                .ok_or("Invalid post detail: missing parent comment")?;
        }
        validated.extend(seen);
    }
    Ok(PostDetail {
        url,
        author: string(&value, "author")?,
        time: string(&value, "time")?,
        text: string(&value, "text")?,
        comments,
    })
}

/// The URL the reader opens for a list row's target, if the reader can read it.
/// A photo shared inside a post (`set=pcb.<id>` or `set=gm.<id>`) opens that post.
pub fn reader_url(target: &str) -> Option<String> {
    let url = crate::fb::web_url(target);
    let parent = url
        .split_once('?')
        .filter(|(path, _)| path.trim_end_matches('/').ends_with("/photo"))
        .and_then(|(_, query)| {
            query
                .split('&')
                .find_map(|p| p.strip_prefix("set=pcb.").or_else(|| p.strip_prefix("set=gm.")))
        });
    let url = match parent {
        Some(id) => format!("https://www.facebook.com/{id}"),
        None => url,
    };
    validate_target(&url).ok().map(|_| url)
}

pub fn validate_target(target: &str) -> Result<(), String> {
    let rest = ["https://www.facebook.com/", "https://facebook.com/", "https://m.facebook.com/"]
        .iter()
        .find_map(|prefix| target.strip_prefix(prefix));
    if let Some(rest) =
        rest.filter(|_| !target.chars().any(|c| c.is_whitespace() || c.is_control() || c == '\\'))
    {
        let path = rest.split(['?', '#']).next().unwrap_or("");
        let segments: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
        let post_path = matches!(segments.as_slice(), [_, "posts", id] | ["groups", _, "posts", id] | ["groups", _, "permalink", id] if !id.is_empty())
            // A bare post id; Facebook redirects it to the post.
            || matches!(segments.as_slice(), [id] if id.bytes().all(|b| b.is_ascii_digit()));
        // Reels and photo viewers use other layouts the reader cannot parse yet.
        let query_post = matches!(path, "permalink.php" | "story.php")
            && rest.split('?').nth(1).is_some_and(|q| {
                q.split('&').any(|p| {
                    ["story_fbid=", "fbid="]
                        .iter()
                        .any(|key| p.strip_prefix(key).is_some_and(|v| !v.is_empty()))
                })
            });
        if post_path || query_post {
            return Ok(());
        }
    }
    Err("This item has no supported Facebook post URL. Refresh the list and open the post again."
        .to_owned())
}

fn eval(fb: &Fb, page: &str, script: &str) -> Result<String, String> {
    fb.run(&["page", page, "eval", script])
}

pub fn load(fb: &Fb, page: &str, target: &str) -> Result<String, String> {
    validate_target(target)?;
    fb.run(&["page", page, "open", target])?;
    let script = format!(
        "{};window.__blindfbPost.initialize({})",
        include_str!("post_scripts/reader.js"),
        json!(target)
    );
    let result = eval(fb, page, &script).and_then(|output| {
        parse_detail(&output)?;
        Ok(output)
    });
    if result.is_err() {
        let _ = close(fb, page);
    }
    result
}

pub fn refresh(fb: &Fb, page: &str) -> Result<String, String> {
    let output = eval(fb, page, "window.__blindfbPost.read()")?;
    parse_detail(&output)?;
    Ok(output)
}

pub fn send_comment(
    fb: &Fb,
    page: &str,
    parent: Option<&str>,
    text: &str,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("Enter a comment before sending".to_owned());
    }
    // Every stage revalidates the same DOM editor and thread; no retry after Enter.
    let prepared = eval(
        fb,
        page,
        &format!("window.__blindfbPost.prepare({},{})", json!(parent), json!(text)),
    )?;
    let reference: String = serde_json::from_str(&prepared)
        .map_err(|_| "Could not identify the selected comment editor".to_owned())?;
    fb.run(&["page", page, "fill", &reference, text])?;
    eval(fb, page, "window.__blindfbPost.check()")?;
    fb.run(&["page", page, "key", "Enter"]).map_err(|_| "Submission could not be confirmed. Refresh before trying again; do not resend automatically.".to_owned())?;
    let output = eval(fb, page, "window.__blindfbPost.confirm()").map_err(|error| {
        format!("Submission could not be confirmed. Refresh before trying again; do not resend automatically. {error}")
    })?;
    parse_detail(&output)?;
    Ok(output)
}

/// Like or unlike one exact comment; returns a fresh detail JSON.
pub fn like_comment(fb: &Fb, page: &str, id: &str) -> Result<String, String> {
    let output = eval(fb, page, &format!("window.__blindfbPost.like({})", json!(id)))?;
    parse_detail(&output)?;
    Ok(output)
}

/// Open Facebook's share dialog for the post and return the audience it will use.
/// Nothing is shared until `share_now`.
pub fn share_open(fb: &Fb, page: &str) -> Result<String, String> {
    let output = eval(fb, page, "window.__blindfbPost.shareOpen()")?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|_| "Could not read Facebook's share audience. Nothing was shared.".to_owned())?;
    value["audience"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Could not read Facebook's share audience. Nothing was shared.".to_owned())
}

/// Share the post to the feed, with optional text, from the dialog `share_open` opened.
pub fn share_now(fb: &Fb, page: &str, text: &str) -> Result<String, String> {
    let result = (|| {
        if !text.trim().is_empty() {
            let prepared =
                eval(fb, page, &format!("window.__blindfbPost.shareFill({})", json!(text)))?;
            let reference: String = serde_json::from_str(&prepared).map_err(|_| {
                "Could not identify the share text box. Nothing was shared.".to_owned()
            })?;
            fb.run(&["page", page, "fill", &reference, text])?;
        }
        eval(fb, page, "window.__blindfbPost.shareNow()")
    })();
    if result.is_err() {
        let _ = share_cancel(fb, page);
    }
    result
}

pub fn share_cancel(fb: &Fb, page: &str) -> Result<String, String> {
    eval(fb, page, "window.__blindfbPost.shareClose()")
}

pub fn close(fb: &Fb, page: &str) -> Result<String, String> {
    fb.run(&["page", page, "close"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_post_targets_before_browser_access() {
        for bad in [
            "1h",
            "p123",
            "https://facebook.com.evil/posts/1",
            "https://www.facebook.com/",
            "https://www.facebook.com/messages/t/1",
            "https://user@www.facebook.com/a/posts/1",
            "https://www.facebook.com/reel/1/",
            "https://www.facebook.com/photo/?fbid=1",
            "https://www.facebook.com/afad/?approval_id=1",
        ] {
            assert!(validate_target(bad).is_err(), "{bad}");
        }
        for good in [
            "https://www.facebook.com/alice/posts/123",
            "https://www.facebook.com/groups/7/posts/8",
            "https://www.facebook.com/permalink.php?story_fbid=1&id=2",
            "https://www.facebook.com/2284937812074776",
        ] {
            assert!(validate_target(good).is_ok(), "{good}");
        }
        assert_eq!(
            reader_url("/photo/?fbid=1&set=pcb.22").as_deref(),
            Some("https://www.facebook.com/22")
        );
        assert_eq!(
            reader_url("/photo/?fbid=1&set=gm.33&idorvanity=4").as_deref(),
            Some("https://www.facebook.com/33")
        );
        assert_eq!(reader_url("/photo/?fbid=1&set=a.5"), None);
        assert_eq!(reader_url("/2284937812074776/x"), None);
        assert!(reader_url("/alice/posts/pfbid0x").is_some());
    }

    #[test]
    fn validates_long_threads_and_shared_ancestors() {
        let comments: Vec<_> = (0..1000).map(|i| serde_json::json!({
            "id": i.to_string(), "author": "A", "time": "now", "text": "reply",
            "parent_id": (i > 0).then(|| (i - 1).to_string()), "url": null
        })).collect();
        let mut data = serde_json::json!({
            "url": "https://www.facebook.com/a/posts/1", "author": "A",
            "time": "now", "text": "post", "comments": comments
        });
        assert_eq!(parse_detail(&data.to_string()).unwrap().comments.len(), 1000);
        data["comments"][0]["parent_id"] = "999".into();
        assert!(parse_detail(&data.to_string()).unwrap_err().contains("cyclic"));
        data["comments"][0]["parent_id"] = "missing".into();
        assert!(parse_detail(&data.to_string()).unwrap_err().contains("missing parent"));
    }

    #[test]
    fn preserves_full_text_and_thread_identity() {
        let text = "long comment ".repeat(100);
        let data = serde_json::json!({"url":"https://www.facebook.com/a/posts/1","author":"A","time":"now","text":text,"comments":[
            {"id":"1","author":"B","time":"now","text":text,"parent_id":null,"url":null},
            {"id":"2","author":"C","time":"now","text":"reply","parent_id":"1","url":null}
        ]});
        let detail = parse_detail(&data.to_string()).unwrap();
        assert_eq!(detail.text, text);
        assert_eq!(detail.comments[0].text, text);
        assert_eq!(detail.comments[1].parent_id.as_deref(), Some("1"));
        let mut bad = data.clone();
        bad["comments"][1]["id"] = "1".into();
        assert!(parse_detail(&bad.to_string()).is_err());
        bad = data.clone();
        bad["comments"][1]["parent_id"] = "missing".into();
        assert!(parse_detail(&bad.to_string()).is_err());
        bad = data;
        bad["comments"][0]["parent_id"] = "2".into();
        assert!(parse_detail(&bad.to_string()).is_err());
        assert!(parse_detail("{}").is_err());
    }
}
