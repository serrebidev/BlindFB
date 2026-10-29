//! Session-only preparation of nearby reels; no video files or signed URLs are persisted.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const STREAM_AGE: Duration = Duration::from_secs(5 * 60);
const RETRY_DELAY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct ReelCache {
    ready: HashMap<String, (String, Instant)>,
    pending: HashSet<String>,
    attempted: HashMap<String, Instant>,
}

impl ReelCache {
    pub fn ready(&self, target: &str, now: Instant) -> Option<&str> {
        self.ready
            .get(target)
            .filter(|(_, at)| now.saturating_duration_since(*at) < STREAM_AGE)
            .map(|(url, _)| url.as_str())
    }

    pub fn is_preparing(&self, target: &str) -> bool {
        self.pending.contains(target)
    }

    pub fn plan(&mut self, targets: &[String], now: Instant) -> Vec<String> {
        self.ready.retain(|_, (_, at)| now.saturating_duration_since(*at) < STREAM_AGE);
        self.attempted.retain(|_, at| now.saturating_duration_since(*at) < RETRY_DELAY);
        let mut jobs = Vec::new();
        for target in targets {
            if self.pending.len() >= 2 {
                break;
            }
            if self.ready.contains_key(target)
                || self.pending.contains(target)
                || self.attempted.contains_key(target)
            {
                continue;
            }
            self.pending.insert(target.clone());
            self.attempted.insert(target.clone(), now);
            jobs.push(target.clone());
        }
        jobs
    }

    pub fn finish(&mut self, target: &str, result: &Result<String, String>, now: Instant) {
        if !self.pending.remove(target) {
            return;
        }
        if let Ok(stream) = result {
            self.ready.insert(target.to_owned(), (stream.clone(), now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn prepares_only_two_neighbors_and_reuses_ready_streams() {
        let now = Instant::now();
        let mut cache = ReelCache::default();
        let targets = vec!["first".into(), "next".into(), "later".into()];
        assert_eq!(cache.plan(&targets, now), ["first", "next"]);
        assert!(cache.plan(&targets, now).is_empty());
        cache.finish("first", &Ok("https://video.example/first".into()), now);
        assert_eq!(cache.ready("first", now), Some("https://video.example/first"));
        assert_eq!(cache.plan(&targets, now), ["later"]);
    }

    #[test]
    fn failures_do_not_loop_and_expired_urls_are_not_reused() {
        let now = Instant::now();
        let mut cache = ReelCache::default();
        let targets = vec!["first".into()];
        cache.plan(&targets, now);
        cache.finish("first", &Err("offline".into()), now);
        assert!(cache.plan(&targets, now + Duration::from_secs(1)).is_empty());
        assert_eq!(cache.plan(&targets, now + Duration::from_secs(31)), ["first"]);
        cache.finish(
            "first",
            &Ok("https://video.example/first".into()),
            now + Duration::from_secs(31),
        );
        assert!(cache.ready("first", now + Duration::from_secs(332)).is_none());
        assert!(!ReelCache::default().is_preparing("first"));
    }
}
