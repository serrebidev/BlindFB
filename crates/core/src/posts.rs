//! Isolated Facebook post readers and verified, thread-scoped comments.

use crate::{tf, Fb};
use patois::t;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    /// The commenter's profile path (/vanity or /profile.php?id=), when the
    /// comment linked one: enough to open the profile or send a friend request.
    pub author_ref: Option<String>,
    pub time: String,
    pub text: String,
    pub parent_id: Option<String>,
    /// For a reply: the person it answers, from the mention Facebook starts it with.
    /// A reply to a reply joins the thread of the first comment, so the parent's
    /// author is not always who it answers.
    pub reply_to: Option<String>,
    pub url: Option<String>,
    /// Browsable links inside the comment's own text (real targets, unwrapped
    /// from Facebook's redirect), for the reader's links list.
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub label: String,
    pub url: String,
    /// A post header name's profile path (/vanity or /profile.php?id=): the
    /// reader opens it in BlindFB instead of the browser.
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostDetail {
    pub url: String,
    pub author: String,
    pub time: String,
    pub text: String,
    pub comments: Vec<Comment>,
    /// Links in the message and pages outside Facebook, for the browser.
    pub links: Vec<Link>,
    /// The Facebook URL of the post's video or reel, for the player.
    pub video: Option<String>,
    pub reaction: Option<String>,
}

/// Keep article links first and the current Facebook post available at the end.
pub fn ordered_links(mut links: Vec<Link>, post: &str) -> Vec<Link> {
    let current = facebook_link_key(post);
    links.retain(|link| facebook_link_key(&link.url).is_none_or(|key| Some(key) != current));
    links.sort_by_key(|link| facebook_link_key(&link.url).is_some());
    links.push(Link {
        label: t("Full post on Facebook"),
        url: post.to_owned(),
        profile: None,
    });
    links
}

fn facebook_link_key(url: &str) -> Option<String> {
    let uri = url
        .split('#')
        .next()?
        .parse::<tungstenite::http::Uri>()
        .ok()?;
    let host = uri.host()?.to_ascii_lowercase();
    if host != "facebook.com" && !host.ends_with(".facebook.com") {
        return None;
    }
    let mut parameters = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|part| {
            matches!(
                part.split('=').next().unwrap_or(""),
                "id" | "story_fbid"
                    | "fbid"
                    | "v"
                    | "set"
                    | "multi_permalinks"
                    | "comment_id"
                    | "reply_comment_id"
            )
        })
        .collect::<Vec<_>>();
    parameters.sort_unstable();
    Some(format!(
        "{}?{}",
        uri.path().trim_end_matches('/'),
        parameters.join("&")
    ))
}

fn string(value: &Value, field: &str) -> Result<String, String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| tf!("Invalid post detail: {field} must be text", field = field))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, String> {
    match &value[field] {
        Value::Null => Ok(None),
        Value::String(text) if !text.is_empty() => Ok(Some(text.clone())),
        _ => Err(tf!("Invalid post detail: {field}", field = field)),
    }
}

/// The `links` rows of a post detail or of one comment: only web pages leave
/// for the browser; anything else stays out of the list.
fn parse_links(value: &Value) -> Vec<Link> {
    value["links"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let label = string(row, "label").ok()?;
                    let profile = optional_string(row, "profile").ok()?;
                    Some(Link {
                        label: match profile {
                            Some(_) => tf!("Profile of {who}", who = label),
                            None => label,
                        },
                        url: string(row, "url").ok()?,
                        profile,
                    })
                })
                .filter(|link| {
                    (link.url.starts_with("https://") || link.url.starts_with("http://"))
                        && !link
                            .url
                            .chars()
                            .any(|c| c.is_whitespace() || c.is_control())
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_detail(output: &str) -> Result<PostDetail, String> {
    let value: Value = serde_json::from_str(output)
        .map_err(|_| t("The post reader returned invalid JSON"))?;
    let url = string(&value, "url")?;
    validate_target(&url)?;
    let rows = value["comments"]
        .as_array()
        .ok_or_else(|| t("Invalid post detail: comments must be a list"))?;
    let mut comments = Vec::with_capacity(rows.len());
    let mut ids = HashSet::new();
    for row in rows {
        let id = string(row, "id")?;
        if id.is_empty() || !ids.insert(id.clone()) {
            return Err(t("Invalid post detail: empty or duplicate comment identity"));
        }
        // A reel's comments can carry only their author's profile link (with a comment_id),
        // which leads to the profile, not the comment: drop it instead of failing the post.
        let url = optional_string(row, "url")?.filter(|url| validate_target(url).is_ok());
        comments.push(Comment {
            id,
            author: string(row, "author")?,
            author_ref: optional_string(row, "author_ref")?,
            time: string(row, "time")?,
            text: string(row, "text")?,
            parent_id: optional_string(row, "parent_id")?,
            reply_to: optional_string(row, "reply_to")?,
            url,
            links: parse_links(row),
        });
    }
    let by_id: HashMap<_, _> = comments
        .iter()
        .map(|comment| (comment.id.as_str(), comment))
        .collect();
    let mut validated = HashSet::new();
    for comment in &comments {
        let mut seen = HashSet::new();
        let mut current = comment;
        while let Some(parent) = &current.parent_id {
            if validated.contains(current.id.as_str()) {
                break;
            }
            if !seen.insert(current.id.as_str()) {
                return Err(t("Invalid post detail: cyclic comment thread"));
            }
            current = by_id
                .get(parent.as_str())
                .ok_or_else(|| t("Invalid post detail: missing parent comment"))?;
        }
        validated.extend(seen);
    }
    let links = parse_links(&value);
    // The player hands this to yt-dlp, so it must be a Facebook video or a site the player knows.
    let video = optional_string(&value, "video")?
        .filter(|video| crate::media::is_playable_video_url(video));
    let links = ordered_links(links, &url);
    Ok(PostDetail {
        url,
        author: string(&value, "author")?,
        time: string(&value, "time")?,
        text: string(&value, "text")?,
        comments,
        links,
        video,
        reaction: crate::fb::own_reaction(
            value["reaction_label"].as_str().unwrap_or(""),
            value["reaction_pressed"].as_bool(),
        ).map(str::to_owned),
    })
}

/// The URL the reader opens for a list row's target, if the reader can read it.
/// A photo shared inside a post (`set=pcb.<id>` or `set=gm.<id>`) opens that post.
pub fn reader_url(target: &str) -> Option<String> {
    let url = crate::fb::web_url(target);
    let url = group_notification_post(&url).unwrap_or(url);
    let parent = url
        .split_once('?')
        .filter(|(path, _)| {
            matches!(
                path.trim_end_matches('/').rsplit('/').next(),
                Some("photo" | "photo.php")
            )
        })
        .and_then(|(_, query)| {
            query.split('&').find_map(|p| {
                p.strip_prefix("set=pcb.")
                    .or_else(|| p.strip_prefix("set=gm."))
            })
        });
    let url = match parent {
        Some(id) => format!("https://www.facebook.com/{id}"),
        None => url,
    };
    validate_target(&url).ok().map(|_| url)
}

/// The group home address of a post URL, e.g.
/// `https://www.facebook.com/groups/123/posts/999` → `https://www.facebook.com/groups/123`.
/// `None` for non-group posts and non-Facebook targets. The backend reads a
/// group's posts through `posts <group url>` (fb.rs), so this is all the
/// Group's posts action needs.
pub fn group_url(url: &str) -> Option<String> {
    let url = url.trim();
    // Feed rows carry absolute Facebook URLs; a bare path is also a
    // Facebook target. Refs (`p3k9a`) and other hosts are not groups.
    let uri = url.parse::<tungstenite::http::Uri>().ok()?;
    if let Some(host) = uri.host() {
        let host = host.to_ascii_lowercase();
        if host != "facebook.com" && !host.ends_with(".facebook.com") {
            return None;
        }
    } else if !uri.path().starts_with('/') {
        return None;
    }
    let mut parts = uri.path().split('/').filter(|part| !part.is_empty());
    if parts.next()? != "groups" {
        return None;
    }
    let group = parts.next()?;
    if group.is_empty() || group == "?" {
        return None;
    }
    Some(format!("https://www.facebook.com/groups/{group}"))
}

/// A notification about several posts at once ("Ann shared 9 posts", "Group has 5 new
/// posts"): whose posts list shows them. Facebook links only one of them.
#[derive(Debug, PartialEq, Eq)]
pub enum NotificationPosts {
    /// A profile path for the posts view (/vanity or /profile.php?id=).
    Person(String),
    /// A group URL for the group posts view.
    Group(String),
}

/// Read from the link, not the words, so every language works: a group link names each
/// post in `multi_permalinks`; a person's batch is `close_friend_activity` with a count of
/// two or more in the text (a single shared post has no number).
pub fn notification_posts(text: &str, target: &str) -> Option<NotificationPosts> {
    let uri = target.trim().parse::<tungstenite::http::Uri>().ok()?;
    let query: Vec<&str> = uri.query().unwrap_or("").split('&').collect();
    let param = |key: &str| query.iter().find_map(|part| part.strip_prefix(key));
    if let Some(ids) = param("multi_permalinks=") {
        let ids = ids.replace("%2C", ",").replace("%2c", ",");
        let many = ids.split(',').filter(|id| !id.is_empty()).count() > 1;
        return many.then(|| group_url(target)).flatten().map(NotificationPosts::Group);
    }
    let count = text
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|digits| digits.parse::<u32>().ok())
        .any(|n| n > 1);
    if param("notif_t=") != Some("close_friend_activity") || !count {
        return None;
    }
    let mut parts = uri.path().split('/').filter(|part| !part.is_empty());
    match (parts.next()?, parts.next()) {
        ("permalink.php" | "story.php", _) => param("id=")
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .map(|id| NotificationPosts::Person(format!("/profile.php?id={id}"))),
        (vanity, Some("posts" | "videos")) if vanity != "groups" => {
            Some(NotificationPosts::Person(format!("/{vanity}")))
        }
        _ => None,
    }
}

fn group_notification_post(url: &str) -> Option<String> {
    let uri = url.parse::<tungstenite::http::Uri>().ok()?;
    if !matches!(
        uri.host()?,
        "www.facebook.com" | "facebook.com" | "m.facebook.com"
    ) {
        return None;
    }
    let parts: Vec<_> = uri
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let ["groups", group] = parts.as_slice() else {
        return None;
    };
    let query: Vec<_> = uri.query()?.split('&').collect();
    let raw = query
        .iter()
        .find_map(|part| part.strip_prefix("post_id="))
        .or_else(|| {
            query
                .iter()
                .find_map(|part| part.strip_prefix("multi_permalinks="))
        })?;
    let ids = raw.replace("%2C", ",").replace("%2c", ",");
    let id = ids.split(',').next()?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut address = format!("https://www.facebook.com/groups/{group}/posts/{id}");
    let comments: Vec<_> = query
        .into_iter()
        .filter(|part| {
            ["comment_id=", "reply_comment_id="].iter().any(|key| {
                part.strip_prefix(key).is_some_and(|value| {
                    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
                })
            })
        })
        .collect();
    if !comments.is_empty() {
        address.push('?');
        address.push_str(&comments.join("&"));
    }
    Some(address)
}

pub fn validate_target(target: &str) -> Result<(), String> {
    let rest = [
        "https://www.facebook.com/",
        "https://facebook.com/",
        "https://m.facebook.com/",
    ]
    .iter()
    .find_map(|prefix| target.strip_prefix(prefix));
    if let Some(rest) = rest.filter(|_| {
        !target
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    }) {
        let path = rest.split(['?', '#']).next().unwrap_or("");
        let segments: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
        let post_path = matches!(segments.as_slice(), [_, "posts", id] | ["groups", _, "posts", id] | ["groups", _, "permalink", id] | ["reel", id] | ["videos", id] | [_, "videos", id] if !id.is_empty())
            // A bare post id; Facebook redirects it to the post.
            || matches!(segments.as_slice(), [id] if id.bytes().all(|b| b.is_ascii_digit()));
        let query_post = matches!(path, "permalink.php" | "story.php")
            && rest.split('?').nth(1).is_some_and(|q| {
                q.split('&').any(|p| {
                    ["story_fbid=", "fbid="]
                        .iter()
                        .any(|key| p.strip_prefix(key).is_some_and(|v| !v.is_empty()))
                })
            });
        let watch = matches!(path.trim_end_matches('/'), "watch" | "watch/live")
            && rest.split('?').nth(1).is_some_and(|query| {
                query.split('&').any(|pair| {
                    pair.strip_prefix("v=")
                        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                })
            });
        let photo = matches!(path.trim_end_matches('/'), "photo" | "photo.php")
            && rest.split('?').nth(1).is_some_and(|query| {
                query.split('&').any(|pair| {
                    pair.strip_prefix("fbid=")
                        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                })
            });
        if post_path || query_post || watch || photo {
            return Ok(());
        }
    }
    Err(t(
        "This item has no supported Facebook post URL. Refresh the list and open the post again."
    ))
}

/// The post each reader page holds, so a page the backend lost can be reopened.
fn opened() -> &'static std::sync::Mutex<HashMap<String, String>> {
    static OPENED: std::sync::OnceLock<std::sync::Mutex<HashMap<String, String>>> =
        std::sync::OnceLock::new();
    OPENED.get_or_init(Default::default)
}

/// The backend restarted or reconnected (its page is gone), or the tab left the post.
fn page_lost(raw: &str) -> bool {
    raw.contains("page expired") || raw.contains("Post changed or reader expired")
}

fn initialize_script(target: &str) -> String {
    format!(
        "{};window.__blindfbPost.initialize({})",
        concat!(
            include_str!("post_scripts/labels.js"),
            include_str!("post_scripts/questions.js"),
            include_str!("post_scripts/reader.js")
        ),
        json!(target)
    )
}

/// One page script, without retrying actions that may already have changed Facebook.
fn eval(fb: &Fb, page: &str, script: &str) -> Result<String, String> {
    fb.run(&["page", page, "eval", script])
        .map_err(|e| crate::page_errors::page_error(&e))
}

/// Reopen a lost page only for reading or preparing an editor, before anything is typed.
fn recover_eval(fb: &Fb, page: &str, script: &str) -> Result<String, String> {
    match fb.run(&["page", page, "eval", script]) {
        Err(raw) if page_lost(&raw) => {
            let target = opened().lock().unwrap().get(page).cloned();
            let Some(target) = target else {
                return Err(crate::page_errors::page_error(&raw));
            };
            fb.run(&["page", page, "open", &target])?;
            fb.run(&["page", page, "eval", &initialize_script(&target)])
                .map_err(|e| crate::page_errors::page_error(&e))?;
            fb.run(&["page", page, "eval", script])
        }
        result => result,
    }
    .map_err(|e| crate::page_errors::page_error(&e))
}

pub fn load(fb: &Fb, page: &str, target: &str) -> Result<String, String> {
    validate_target(target)?;
    fb.run(&["page", page, "open", target])?;
    let mut target = target.to_owned();
    if target.split('?').next().is_some_and(|path| {
        matches!(
            path.trim_end_matches('/').rsplit('/').next(),
            Some("photo" | "photo.php")
        )
    }) {
        let parent = eval(
            fb,
            page,
            r#"(async()=>{const links=await window.__fb.waitFor(()=>{const found=[...document.querySelectorAll('a[href]')].filter(a=>!a.closest('[role=article][aria-label]')&&window.__fb.visible(a)&&/^(View post|Voir la publication|Xem bài viết)$/i.test(window.__fb.clean(a.innerText||'')));return found.length===1?found[0].href:null;},3000);return JSON.stringify(links);})()"#,
        )?;
        if let Ok(Some(parent)) = serde_json::from_str::<Option<String>>(&parent) {
            if let Some(parent) = reader_url(&parent).filter(|parent| !parent.contains("/photo")) {
                fb.run(&["page", page, "open", &parent])?;
                target = parent;
            }
        }
    }
    // Before the first script: a page lost during load reopens this post, not the last one.
    opened().lock().unwrap().insert(page.to_owned(), target.clone());
    let script = initialize_script(&target);
    let result = recover_eval(fb, page, &script).and_then(|output| {
        let detail = parse_detail(&output)?;
        if detail.text.trim().is_empty()
            && detail
                .video
                .as_ref()
                .is_some_and(|video| video.contains("/reel/"))
        {
            if let Ok(info) = crate::media::stream_details(fb, page, &target)
                .and_then(|data| crate::media::parse_stream(&data))
            {
                if !info.title.is_empty() {
                    return eval(
                        fb,
                        page,
                        &format!("window.__blindfbPost.setTitle({})", json!(info.title)),
                    );
                }
            }
        }
        Ok(output)
    });
    if result.is_err() {
        let _ = close(fb, page);
    }
    result
}

/// Press Facebook's own translation on the post and its comments, or back to the
/// originals when nothing is left to translate; read-only for Facebook.
pub fn translate(fb: &Fb, page: &str) -> Result<String, String> {
    let output = recover_eval(fb, page, "window.__blindfbPost.translate()")?;
    parse_detail(&output)?;
    Ok(output)
}

pub fn refresh(fb: &Fb, page: &str) -> Result<String, String> {
    let output = recover_eval(fb, page, "window.__blindfbPost.read()")?;
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
        return Err(t("Enter a comment before sending"));
    }
    // Every stage revalidates the same DOM editor and thread; no retry after Enter.
    let prepared = recover_eval(
        fb,
        page,
        &format!(
            "window.__blindfbPost.prepare({},{})",
            json!(parent),
            json!(text)
        ),
    )?;
    let typed: String = serde_json::from_str(&prepared)
        .map_err(|_| t("Could not identify the selected comment editor"))?;
    // Types at the caret: `page fill` would replace the name Facebook prefills in a reply.
    let typed = json!({ "text": typed }).to_string();
    fb.run(&["page", page, "cdp", "Input.insertText", &typed])?;
    eval(fb, page, "window.__blindfbPost.check()")?;
    fb.run(&["page", page, "key", "Enter"])
        .map_err(|_| {
            t("Submission could not be confirmed. Refresh before trying again; do not resend automatically.")
        })?;
    let output = eval(fb, page, "window.__blindfbPost.confirm()").map_err(|error| {
        tf!(
            "Submission could not be confirmed. Refresh before trying again; do not resend automatically. {error}",
            error = error
        )
    })?;
    if parse_participation(&output)?.is_none() {
        parse_detail(&output)?;
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipationField {
    pub kind: String,
    pub prompt: String,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participation {
    pub token: String,
    pub text: String,
    pub fields: Vec<ParticipationField>,
    pub answers: Vec<Value>,
    pub error: String,
}

pub fn parse_participation(output: &str) -> Result<Option<Participation>, String> {
    let value: Value = serde_json::from_str(output)
        .map_err(|_| t("The post reader returned invalid JSON"))?;
    let Some(form) = value.get("participation") else { return Ok(None) };
    let rows = form["fields"].as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 30)
        .ok_or_else(|| t("Invalid participation questions"))?;
    let mut fields = Vec::new();
    for row in rows {
        let kind = string(row, "kind")?;
        let prompt = string(row, "prompt")?;
        if prompt.trim().is_empty() || !matches!(kind.as_str(), "text" | "choice" | "check") {
            return Err(t("Invalid participation question"));
        }
        let choices: Vec<String> = row["choices"].as_array()
            .ok_or_else(|| t("Invalid participation choices"))?
            .iter().map(|v| v.as_str().filter(|s| !s.trim().is_empty()).map(str::to_owned)
                .ok_or_else(|| t("Invalid participation choice"))).collect::<Result<_, _>>()?;
        if kind == "choice" && choices.is_empty() {
            return Err(t("Invalid participation choices"));
        }
        fields.push(ParticipationField { kind, prompt, choices });
    }
    let token = string(form, "token")?;
    if token.is_empty() { return Err(t("Invalid participation questions")); }
    let answers = form["answers"].as_array().filter(|a| a.len() == fields.len())
        .cloned().unwrap_or_default();
    let error = form["error"].as_str().unwrap_or_default().to_owned();
    Ok(Some(Participation { token, text: string(form, "text")?, fields, answers, error }))
}

/// Submit answers once, or close the review. Never resends the pending comment.
pub fn participation(fb: &Fb, page: &str, token: &str, answers: Option<&Value>) -> Result<String, String> {
    let script = match answers {
        Some(answers) => format!("window.__blindfbQuestions.submit({},{answers})", json!(token)),
        None => format!("window.__blindfbQuestions.cancel({})", json!(token)),
    };
    eval(fb, page, &script)
}

/// Like or unlike one exact comment; returns a fresh detail JSON.
pub fn like_comment(fb: &Fb, page: &str, id: &str) -> Result<String, String> {
    let output = eval(
        fb,
        page,
        &format!("window.__blindfbPost.like({})", json!(id)),
    )?;
    parse_detail(&output)?;
    Ok(output)
}

/// React to the reel open in the reader; returns a fresh detail JSON.
pub fn reel_react(fb: &Fb, page: &str, reaction: &str) -> Result<String, String> {
    let output = eval(
        fb,
        page,
        &format!("window.__blindfbPost.reelReact({})", json!(reaction)),
    )?;
    parse_detail(&output)?;
    Ok(output)
}

/// Open Facebook's share dialog for the post and return the audience it will use.
/// Nothing is shared until `share_now`.
pub fn share_open(fb: &Fb, page: &str) -> Result<String, String> {
    // Facebook ignores script clicks on Share inside a post dialog: click it through the
    // browser when the page can name it, else let the script press it.
    let target = eval(fb, page, "window.__blindfbPost.shareTarget()")?;
    let clicked = match serde_json::from_str::<Option<String>>(&target) {
        Ok(Some(reference)) => fb.run(&["page", page, "click", &reference]).is_ok(),
        _ => false,
    };
    let output = eval(
        fb,
        page,
        &format!("window.__blindfbPost.shareOpen({clicked})"),
    )?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|_| t("Could not read Facebook's share audience. Nothing was shared."))?;
    value["audience"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| t("Could not read Facebook's share audience. Nothing was shared."))
}

/// Share the post to the feed, with optional text, from the dialog `share_open` opened.
pub fn share_now(fb: &Fb, page: &str, text: &str) -> Result<String, String> {
    let result = (|| {
        if !text.trim().is_empty() {
            let prepared = eval(
                fb,
                page,
                &format!("window.__blindfbPost.shareFill({})", json!(text)),
            )?;
            let reference: String = serde_json::from_str(&prepared).map_err(|_| {
                t("Could not identify the share text box. Nothing was shared.")
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
    opened().lock().unwrap().remove(page);
    fb.run(&["page", page, "close"])
}

#[cfg(test)]
mod tests {
    #[test]
    fn post_detail_keeps_the_accounts_reaction_when_exposed() {
        let mut value = serde_json::json!({"url":"https://www.facebook.com/a/posts/1","author":"A","time":"1h","text":"Body","comments":[]});
        assert_eq!(parse_detail(&value.to_string()).unwrap().reaction, None);
        value["reaction_label"] = "Remove Wow from A's post".into();
        value["reaction_pressed"] = true.into();
        assert_eq!(parse_detail(&value.to_string()).unwrap().reaction.as_deref(), Some("wow"));
    }

    #[test]
    fn participation_is_distinct_from_a_confirmed_comment() {
        let form = parse_participation(r#"{"participation":{"token":"1","text":"Group rules","fields":[{"kind":"choice","prompt":"Live in Hope?","choices":["yes","no"]},{"kind":"text","prompt":"About Hope?","choices":[]},{"kind":"check","prompt":"Agree to rules","choices":[]}]}}"#).unwrap().unwrap();
        assert_eq!(form.fields[0].choices, ["yes", "no"]);
        assert_eq!(form.fields[1].kind, "text");
        assert_eq!(form.fields[2].kind, "check");
        assert!(parse_participation(r#"{"comments":[]}"#).unwrap().is_none());
        for raw in [
            r#"{"participation":{"token":"1","text":"","fields":[]}}"#,
            r#"{"participation":{"token":"1","text":"","fields":[{"kind":"choice","prompt":"Q","choices":[]}]}}"#,
            r#"{"participation":{"token":"1","text":"","fields":[{"kind":"unknown","prompt":"Q","choices":[]}]}}"#,
        ] {
            assert!(parse_participation(raw).is_err());
        }
    }

    #[test]
    fn only_lost_pages_allow_preparation_recovery() {
        assert!(page_lost("page expired or unknown"));
        assert!(page_lost("Post changed or reader expired. Refresh or reopen the post."));
        assert!(!page_lost("Submission was not confirmed in the selected thread."));
        assert!(!page_lost("The Facebook editor already contains a draft."));
    }

    #[test]
    fn a_comment_link_to_a_profile_is_dropped_not_fatal() {
        let detail = parse_detail(r#"{"url":"https://www.facebook.com/reel/1/","author":"A","time":"","text":"Reel","comments":[{"id":"c1","author":"B","time":"7 h","text":"Hi","parent_id":null,"url":"https://www.facebook.com/someone?comment_id=Y29t&__tn__=R"}]}"#).unwrap();
        assert_eq!(detail.comments.len(), 1);
        assert_eq!(detail.comments[0].url, None);
    }

    #[test]
    fn comment_links_parse_and_bad_rows_are_dropped() {
        let detail = parse_detail(r#"{"url":"https://www.facebook.com/a/posts/1","author":"A","time":"","text":"Post","comments":[{"id":"c1","author":"B","time":"7 h","text":"see this","parent_id":null,"url":null,"links":[{"label":"Example","url":"https://example.com/story"},{"label":"Bad","url":"javascript:alert(1)"},{"label":"Worse","url":"https://exa mple.com"}]},{"id":"c2","author":"C","time":"8 h","text":"no links field","parent_id":null,"url":null}]}"#).unwrap();
        assert_eq!(
            detail.comments[0].links,
            [Link {
                label: "Example".to_owned(),
                url: "https://example.com/story".to_owned(),
                profile: None,
            }]
        );
        assert!(detail.comments[1].links.is_empty());
    }

    #[test]
    fn header_profile_links_are_named_and_keep_their_path() {
        // A group post sharing a page's post: the reader can open the page.
        let detail = parse_detail(r#"{"url":"https://www.facebook.com/groups/1/posts/2","author":"A","time":"","text":"Post","comments":[],"links":[{"label":"The Sibling Groupchat","url":"https://www.facebook.com/profile.php?id=615","profile":"/profile.php?id=615"}]}"#).unwrap();
        let link = &detail.links[0];
        assert_eq!(link.label, "Profile of The Sibling Groupchat");
        assert_eq!(link.profile.as_deref(), Some("/profile.php?id=615"));
    }

    use super::*;

    #[test]
    fn links_put_external_pages_first_and_the_current_post_last() {
        let post = "https://www.facebook.com/a/posts/1";
        let data = json!({"url":post,"author":"A","time":"","text":"","comments":[],"links":[
            {"label":"Shared post","url":"https://www.facebook.com/b/posts/2"},
            {"label":"Current post duplicate","url":format!("{post}/")},
            {"label":"Article","url":"https://example.org/story"},
            {"label":"Another article","url":"https://facebook.com.example.org/story"}
        ]});
        let links = parse_detail(&data.to_string()).unwrap().links;
        assert_eq!(
            links
                .iter()
                .map(|link| link.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Article",
                "Another article",
                "Shared post",
                "Full post on Facebook"
            ]
        );
        assert_eq!(links.last().unwrap().url, post);
    }

    #[test]
    fn current_post_aliases_are_removed_but_comment_and_other_post_links_remain() {
        let post = "https://www.facebook.com/a/posts/1";
        let urls = [
            "https://m.facebook.com/a/posts/1?ref=share#fragment",
            "https://www.facebook.com/a/posts/1?__cft__[0]=tracking&fbclid=tracking",
            "https://www.facebook.com/a/posts/1?comment_id=3",
            "https://www.facebook.com/a/posts/2",
        ];
        let links = urls
            .iter()
            .map(|url| Link {
                label: (*url).to_owned(),
                url: (*url).to_owned(),
                profile: None,
            })
            .collect();
        let ordered = ordered_links(links, post);
        assert_eq!(
            ordered
                .iter()
                .map(|link| link.url.as_str())
                .collect::<Vec<_>>(),
            [urls[2], urls[3], post]
        );
        let post = "https://www.facebook.com/permalink.php?story_fbid=1&id=2";
        let links = vec![Link {
            label: "Other post".into(),
            url: "https://www.facebook.com/permalink.php?story_fbid=3&id=2".into(),
            profile: None,
        }];
        assert_eq!(ordered_links(links, post).len(), 2);
    }

    #[test]
    fn detail_exposes_web_links_and_only_playable_video() {
        let mut data = json!({"url":"https://www.facebook.com/a/posts/1","author":"A","time":"","text":"","comments":[],
            "links":[{"label":"Article","url":"https://example.org/story"},{"label":"Unsafe","url":"file:///secret"}],
            "video":"https://www.facebook.com/reel/123/"});
        let detail = parse_detail(&data.to_string()).unwrap();
        assert_eq!(
            detail.links[0],
            Link {
                label: "Article".to_owned(),
                url: "https://example.org/story".to_owned(),
                profile: None,
            }
        );
        assert_eq!(
            detail.links.last().unwrap().url,
            "https://www.facebook.com/a/posts/1"
        );
        assert_eq!(
            detail.video.as_deref(),
            Some("https://www.facebook.com/reel/123/")
        );
        // A YouTube clip shared into a post plays too, without the Facebook session.
        data["video"] = "https://www.youtube.com/watch?v=1".into();
        assert_eq!(
            parse_detail(&data.to_string()).unwrap().video.as_deref(),
            Some("https://www.youtube.com/watch?v=1")
        );
        for bad in [
            "https://example.org/video",
            "https://youtube.com.evil/watch?v=1",
            "file:///etc/passwd",
        ] {
            data["video"] = bad.into();
            assert!(
                parse_detail(&data.to_string()).unwrap().video.is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn validates_post_targets_before_browser_access() {
        for bad in [
            "1h",
            "p123",
            "https://facebook.com.evil/posts/1",
            "https://www.facebook.com/",
            "https://www.facebook.com/messages/t/1",
            "https://user@www.facebook.com/a/posts/1",
            "https://www.facebook.com/reel/",
            "https://www.facebook.com/photo/?fbid=not-a-number",
            "https://www.facebook.com/afad/?approval_id=1",
        ] {
            assert!(validate_target(bad).is_err(), "{bad}");
        }
        for good in [
            "https://www.facebook.com/alice/posts/123",
            "https://www.facebook.com/groups/7/posts/8",
            "https://www.facebook.com/permalink.php?story_fbid=1&id=2",
            "https://www.facebook.com/2284937812074776",
            "https://www.facebook.com/reel/1413444364110358/",
            "https://www.facebook.com/somepage/videos/123/",
            "https://www.facebook.com/videos/123/",
            "https://www.facebook.com/watch/?v=123",
            "https://www.facebook.com/photo/?fbid=1&set=a.5",
            "https://www.facebook.com/photo.php?fbid=1",
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
        assert_eq!(
            reader_url("/photo/?fbid=1&set=a.5").as_deref(),
            Some("https://www.facebook.com/photo/?fbid=1&set=a.5")
        );
        assert_eq!(
            reader_url("/photo.php?fbid=1&set=pcb.22").as_deref(),
            Some("https://www.facebook.com/22")
        );
        assert_eq!(reader_url("/2284937812074776/x"), None);
        assert!(reader_url("/alice/posts/pfbid0x").is_some());
        // Group window rows (`posts <group>`) carry relative links.
        assert_eq!(
            reader_url("/groups/aiofanclub/posts/4662680307309535/").as_deref(),
            Some("https://www.facebook.com/groups/aiofanclub/posts/4662680307309535/")
        );
    }

    #[test]
    fn group_url_only_comes_from_group_posts() {
        assert_eq!(
            group_url("https://www.facebook.com/groups/123/posts/999"),
            Some("https://www.facebook.com/groups/123".to_owned())
        );
        assert_eq!(
            group_url("https://www.facebook.com/groups/vanity/permalink/55?x=1"),
            Some("https://www.facebook.com/groups/vanity".to_owned())
        );
        assert_eq!(
            group_url("/groups/7/?post_id=8"),
            Some("https://www.facebook.com/groups/7".to_owned()),
        );
        assert_eq!(group_url("https://www.facebook.com/jdoe/posts/1"), None);
        assert_eq!(group_url("https://example.com/groups/7"), None);
        assert_eq!(group_url("p3k9a"), None);
        assert_eq!(group_url(""), None);
    }

    #[test]
    fn group_notifications_resolve_posts_and_keep_comment_identity() {
        assert_eq!(
            reader_url("/groups/7/?post_id=8&comment_id=9&reply_comment_id=10&notif_id=11")
                .as_deref(),
            Some("https://www.facebook.com/groups/7/posts/8?comment_id=9&reply_comment_id=10")
        );
        assert_eq!(
            reader_url("/groups/aisaas/?multi_permalinks=12%2C13&notif_t=group_highlights")
                .as_deref(),
            Some("https://www.facebook.com/groups/aisaas/posts/12")
        );
        assert!(reader_url("/groups/7/?post_id=invalid").is_none());
    }

    #[test]
    fn several_posts_notifications_open_the_posts_list() {
        // Live 2026-11-12: Facebook links only the first of the batch.
        assert_eq!(
            notification_posts(
                "Maria Saieva shared 9 posts.",
                "/maria.s.kovacs.3/posts/pfbid0sy?notif_id=1&notif_t=close_friend_activity&ref=notif"
            ),
            Some(NotificationPosts::Person("/maria.s.kovacs.3".to_owned()))
        );
        assert_eq!(
            notification_posts(
                "Ann shared 3 posts.",
                "/permalink.php?story_fbid=pfbid02&id=61573851815718&notif_t=close_friend_activity"
            ),
            Some(NotificationPosts::Person("/profile.php?id=61573851815718".to_owned()))
        );
        assert_eq!(
            notification_posts(
                "Disabled People who want Equality has 5 new posts.",
                "/groups/466408572614151/?multi_permalinks=11%2C12%2C13&notif_t=group_activity"
            ),
            Some(NotificationPosts::Group(
                "https://www.facebook.com/groups/466408572614151".to_owned()
            ))
        );
        // One post, or no count: the reader opens it as before.
        for (text, target) in [
            ("Group has a new post.", "/groups/7/?multi_permalinks=12&notif_t=group_activity"),
            ("Maria shared a post.", "/m/posts/1?notif_t=close_friend_activity"),
            ("Maria shared 1 post.", "/m/posts/1?notif_t=close_friend_activity"),
            ("Bob commented 3 times.", "/b/posts/1?notif_t=feedback_reaction_generic"),
        ] {
            assert_eq!(notification_posts(text, target), None, "{text}");
        }
    }

    #[test]
    fn validates_long_threads_and_shared_ancestors() {
        let comments: Vec<_> = (0..1000)
            .map(|i| {
                serde_json::json!({
                    "id": i.to_string(), "author": "A", "time": "now", "text": "reply",
                    "parent_id": (i > 0).then(|| (i - 1).to_string()), "url": null
                })
            })
            .collect();
        let mut data = serde_json::json!({
            "url": "https://www.facebook.com/a/posts/1", "author": "A",
            "time": "now", "text": "post", "comments": comments
        });
        assert_eq!(
            parse_detail(&data.to_string()).unwrap().comments.len(),
            1000
        );
        data["comments"][0]["parent_id"] = "999".into();
        assert!(parse_detail(&data.to_string())
            .unwrap_err()
            .contains("cyclic"));
        data["comments"][0]["parent_id"] = "missing".into();
        assert!(parse_detail(&data.to_string())
            .unwrap_err()
            .contains("missing parent"));
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
