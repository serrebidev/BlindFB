//! Update check against the GitHub releases of serrebidev/BlindFB.
//!
//! Releases are packaged and signed with shipfitter (see the repo's xtask);
//! this module only answers "is there a newer one, and where is it".

pub const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/serrebidev/BlindFB/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/serrebidev/BlindFB/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub url: String,
}

/// Fetch the latest published release. Blocking; call from a worker thread.
pub fn latest_release() -> Result<Release, String> {
    let mut response = ureq::get(RELEASES_LATEST_URL)
        .header("User-Agent", concat!("BlindFB/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("update check failed: {e}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("update check failed: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("update check failed: {e}"))?;
    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("update check failed: no tag_name in response")?
        .to_owned();
    let url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or(RELEASES_PAGE)
        .to_owned();
    Ok(Release {
        version: tag.trim_start_matches('v').to_owned(),
        tag,
        url,
    })
}

/// Compare dotted versions numerically: `is_newer("0.1.0", "0.2.0")`.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let parse = |v: &str| {
        v.trim_start_matches('v')
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect::<Vec<u64>>()
    };
    let current = parse(current);
    let candidate = parse(candidate);
    for i in 0..current.len().max(candidate.len()) {
        let a = current.get(i).copied().unwrap_or(0);
        let b = candidate.get(i).copied().unwrap_or(0);
        if b != a {
            return b > a;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("1.9.9", "1.10.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.2.0", "0.1.9"));
    }
}
