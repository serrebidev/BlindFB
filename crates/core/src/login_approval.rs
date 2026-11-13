//! Facebook's "Someone's trying to log in. Let us know if it's you."
//! notification. Its page (`/afad/?approval_id=…`) shows the device, place
//! and time with two answers; BlindFB reads it in its own tab and presses the
//! answer the user picks.

use crate::Fb;
use patois::t;

/// The backend tab the request stays open in between reading and answering.
pub const PAGE: &str = "login";

const SCRIPT: &str = include_str!("login_approval.js");

/// A notification target that is a login request.
pub fn is_request(target: &str) -> bool {
    target.starts_with("/afad/")
        || target.contains("facebook.com/afad/")
        || target.contains("notif_t=approve_from_another_device")
}

/// What the request page says, and Facebook's own answer labels
/// (empty once the request has expired or was answered).
#[derive(Debug, PartialEq)]
pub struct Request {
    pub text: String,
    pub choices: Vec<String>,
    pub answered: bool,
}

fn script(choice: Option<&str>) -> String {
    let choice = serde_json::to_string(&choice).unwrap_or_else(|_| "null".to_owned());
    format!("{}({choice})", SCRIPT.trim_end())
}

/// Open the request in its tab and read it. Returns the raw page answer for [`parse`].
pub fn read(fb: &Fb, target: &str) -> Result<String, String> {
    let url = if target.starts_with('/') {
        format!("https://www.facebook.com{target}")
    } else {
        target.to_owned()
    };
    fb.run(&["page", PAGE, "open", &url])?;
    fb.run(&["page", PAGE, "eval", &script(None)])
}

/// Press the answer labelled `choice` on the open request, then close its tab.
pub fn answer(fb: &Fb, choice: &str) -> Result<String, String> {
    let result = fb.run(&["page", PAGE, "eval", &script(Some(choice))]);
    let _ = fb.run(&["page", PAGE, "close"]);
    result
}

pub fn parse(output: &str) -> Result<Request, String> {
    let value: serde_json::Value = serde_json::from_str(output.trim())
        .map_err(|_| t("Facebook's login request page could not be read."))?;
    if value.get("err").is_some() {
        return Err(t(
            "The answer button is gone from Facebook's login request. Open the notification again.",
        ));
    }
    let strings = |key: &str| -> Vec<String> {
        value[key]
            .as_array()
            .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };
    Ok(Request {
        text: strings("lines").join("\n"),
        choices: strings("choices"),
        answered: value["answered"].as_bool().unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_login_requests() {
        assert!(is_request("/afad/?approval_id=1&notif_t=approve_from_another_device&ref=notif"));
        assert!(is_request("https://www.facebook.com/afad/?approval_id=1"));
        assert!(!is_request("/groups/x/?multi_permalinks=1&notif_t=group_activity"));
    }

    #[test]
    fn reads_the_request_and_its_answers() {
        let raw = r#"{"lines":["Brandon, is this you trying to log in?","Login attempt from Windows","Maple Ridge, BC, Canada"],"choices":["Yes, it’s me","No, it’s not me"]}"#;
        let request = parse(raw).unwrap();
        assert_eq!(request.choices, ["Yes, it’s me", "No, it’s not me"]);
        assert!(request.text.contains("Maple Ridge"));
        assert!(!request.answered);
        assert!(parse(r#"{"err":"answer not found"}"#).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn script_passes_the_answer_as_a_json_string() {
        assert!(script(None).ends_with("})(null)"));
        assert!(script(Some("It's \"me\"")).ends_with(r#"})("It's \"me\"")"#));
    }
}
