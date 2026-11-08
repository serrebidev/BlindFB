//! Errors thrown by BlindFB's page scripts (`post_scripts/reader.js`, `reels.js`), in the
//! user's language. The backend relays them as `js: Error: <message>` followed by a stack
//! trace; the scripts stay in English and the message is translated here.

use patois::t;

/// A page error as the user should read it: the message alone, translated when known.
/// Anything else passes through unchanged.
pub fn page_error(raw: &str) -> String {
    // The fb command line prints its errors as "error: <message>".
    let raw = raw.strip_prefix("error: ").unwrap_or(raw);
    let Some(rest) = raw.strip_prefix("js: ") else {
        return raw.to_owned();
    };
    let first = rest.lines().next().unwrap_or(rest).trim();
    let message = first.strip_prefix("Error: ").unwrap_or(first);
    translated(message).unwrap_or_else(|| message.to_owned())
}

fn translated(message: &str) -> Option<String> {
    Some(match message {
        "Ambiguous comment permalink; refresh the post." => t("Ambiguous comment permalink; refresh the post."),
        "Duplicate comment identity; refresh the post." => t("Duplicate comment identity; refresh the post."),
        "Facebook did not confirm the Like. Refresh to check." => t("Facebook did not confirm the Like. Refresh to check."),
        "Facebook did not confirm the reaction. Refresh to check." => t("Facebook did not confirm the reaction. Refresh to check."),
        "Facebook did not confirm the share. Check your profile before trying again; do not share again automatically." => t("Facebook did not confirm the share. Check your profile before trying again; do not share again automatically."),
        "Facebook did not open its share dialog. Nothing was shared." => t("Facebook did not open its share dialog. Nothing was shared."),
        "Facebook did not show its reactions. Nothing was changed." => t("Facebook did not show its reactions. Nothing was changed."),
        "Facebook has not expanded all text. Refresh the reader to retry." => t("Facebook has not expanded all text. Refresh the reader to retry."),
        "Facebook has not expanded the reel caption. Refresh the reader to retry." => t("Facebook has not expanded the reel caption. Refresh the reader to retry."),
        "Facebook has not expanded the reel caption. Refresh to retry." => t("Facebook has not expanded the reel caption. Refresh to retry."),
        "More than one post dialog is open." => t("More than one post dialog is open."),
        "More than one post is visible. Open the individual post permalink." => t("More than one post is visible. Open the individual post permalink."),
        "No unique comment box on this post. Nothing was sent." => t("No unique comment box on this post. Nothing was sent."),
        "No unique editor inside the selected comment thread. Nothing was sent." => t("No unique editor inside the selected comment thread. Nothing was sent."),
        "Post changed or reader expired. Refresh or reopen the post." => t("Post changed or reader expired. Refresh or reopen the post."),
        "Selected comment changed or disappeared. Refresh and select it again." => t("Selected comment changed or disappeared. Refresh and select it again."),
        "Selected comment is ambiguous or stale." => t("Selected comment is ambiguous or stale."),
        "Submission was not confirmed in the selected thread. Refresh before trying again; do not resend automatically." => t("Submission was not confirmed in the selected thread. Refresh before trying again; do not resend automatically."),
        "The Facebook editor already contains a draft. Nothing was overwritten." => t("The Facebook editor already contains a draft. Nothing was overwritten."),
        "The Share now button is missing. Nothing was shared." => t("The Share now button is missing. Nothing was shared."),
        "The comment editor has no reliable accessible name. Nothing was sent." => t("The comment editor has no reliable accessible name. Nothing was sent."),
        "The comment editor is ambiguous. Nothing was sent." => t("The comment editor is ambiguous. Nothing was sent."),
        "The post could not be read." => t("The post could not be read."),
        "The reel could not be read." => t("The reel could not be read."),
        "The reel's comments could not be opened." => t("The reel's comments could not be opened."),
        "The selected comment editor changed. Nothing was sent." => t("The selected comment editor changed. Nothing was sent."),
        "The selected comment has no unique Like button. Nothing was changed." => t("The selected comment has no unique Like button. Nothing was changed."),
        "The selected comment has no unique Reply action. Nothing was sent." => t("The selected comment has no unique Reply action. Nothing was sent."),
        "The share dialog closed. Nothing was shared." => t("The share dialog closed. Nothing was shared."),
        "The share dialog has no unique text box. Nothing was shared." => t("The share dialog has no unique text box. Nothing was shared."),
        "The share text box already contains text. Nothing was shared." => t("The share text box already contains text. Nothing was shared."),
        "The share text box is ambiguous. Nothing was shared." => t("The share text box is ambiguous. Nothing was shared."),
        "The share text did not match. Nothing was shared." => t("The share text did not match. Nothing was shared."),
        "The video comments could not be opened." => t("The video comments could not be opened."),
        "There is no pending submission." => t("There is no pending submission."),
        "This post has no unique Share button. Nothing was shared." => t("This post has no unique Share button. Nothing was shared."),
        "This reel has no Like button. Nothing was changed." => t("This reel has no Like button. Nothing was changed."),
        "This reel has no unique Comment button." => t("This reel has no unique Comment button."),
        "This reel has no unique React button. Nothing was changed." => t("This reel has no unique React button. Nothing was changed."),
        "This reels list expired. Refresh it to continue." => t("This reels list expired. Refresh it to continue."),
        "Unknown reaction. Nothing was changed." => t("Unknown reaction. Nothing was changed."),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_trace_is_dropped_and_other_errors_pass_through() {
        assert_eq!(
            page_error("js: Error: The reel could not be read.\n    at Object.initialize (<anonymous>:431:26)"),
            "The reel could not be read."
        );
        assert_eq!(page_error("js: TypeError: x is null\n    at y"), "TypeError: x is null");
        assert_eq!(page_error("not logged in"), "not logged in");
        assert_eq!(
            page_error("error: js: Error: The post could not be read.\n    at x"),
            "The post could not be read."
        );
    }
}
