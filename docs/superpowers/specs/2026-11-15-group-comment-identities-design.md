# Group comment identities

Use Facebook's current group-post identity controls to expose main-profile, anonymous and nickname participation through BlindFB's native reader. Show the active identity and any restriction. Offer only actions Facebook exposes, including nickname setup prompts. Keep drafts and reply targets when changing identity. A requested identity must be verified immediately before sending; never fall back to a different identity. Refresh identity and reactions after switching. Non-group posts retain existing behavior.

Reuse the isolated post reader and native dialogs. Add a page-side identity adapter, a typed Rust bridge and an accessible native action. Keep Facebook's rules, persistence and validation authoritative. Do not call undocumented publishing APIs or modify group settings. No live comments, posts or reactions will be submitted during development.

Acceptance: available choices and disabled/locked states are exposed; nested identity/setup dialogs work; cancel preserves drafts; stale menu tokens and identity changes block dispatch; normal comments and exact-thread replies continue working; keyboard users can reach every new control. Tests cover group and non-group pages, all identity kinds, setup, cancellation, unavailable/locked controls and identity drift.
